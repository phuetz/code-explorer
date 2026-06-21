import type { WorkDocument, WorkDocumentQuestion } from '../types/chat';
import { repairMermaidSource } from './mermaid';
import { reformulateChatPrompt } from './prompt-rewrite';
import { relatedSourcesFilename } from './related-sources-export';

export type WorkDocumentQualitySeverity = 'error' | 'warning' | 'info';
export type WorkDocumentQualityLevel = 'blocked' | 'review' | 'ready';

export interface WorkDocumentQualityIssue {
  id: string;
  label: string;
  severity: WorkDocumentQualitySeverity;
  detail: string;
}

export interface WorkDocumentQualityCheck {
  id: string;
  label: string;
  value: string;
  ok: boolean;
}

export interface WorkDocumentQualityReport {
  level: WorkDocumentQualityLevel;
  score: number;
  checks: WorkDocumentQualityCheck[];
  issues: WorkDocumentQualityIssue[];
  summary: {
    total: number;
    answered: number;
    pending: number;
    errors: number;
    sourceReferences: number;
    sourceFiles: number;
    diagrams: number;
    codeBlocks: number;
    shortAnswers: number;
    answeredWithoutSources: number;
    renderArtifacts: number;
  };
}

export interface WorkDocumentSourceReferenceSummary {
  path: string;
  count: number;
  questionLabels: string[];
}

export function buildWorkQuestionPrompt({
  document,
  question,
  repositoryName,
}: {
  document: Pick<WorkDocument, 'filename' | 'repoName'>;
  question: Pick<WorkDocumentQuestion, 'label' | 'text' | 'context'>;
  repositoryName?: string | null;
}): string {
  const structuredQuestion = reformulateChatPrompt(
    question.text,
    repositoryName ?? document.repoName
  );
  const context = question.context.trim();
  return [
    `Atelier document Code Explorer : ${document.filename}`,
    `Question extraite : ${question.label}`,
    '',
    structuredQuestion,
    '',
    'Contexte extrait du document source :',
    context || '- Aucun contexte adjacent exploitable dans le document source.',
    '',
    'Consignes atelier document :',
    '- Traite cette question comme une tâche autonome du document de travail.',
    '- Utilise les outils Code Explorer avant de conclure et lis les fichiers nécessaires.',
    '- Ne recopie pas une ancienne réponse du document source sans la vérifier dans le code.',
    '- Si une recherche ne trouve rien, explique précisément les recherches effectuées et leurs limites.',
    '- Cite les chemins exacts des fichiers réellement consultés dans une section Sources.',
    '- Rédige une réponse exploitable dans un livrable final, pas seulement dans le chat.',
    '',
    'Structure de réponse attendue :',
    '1. Synthèse courte : réponse directe en 3 à 6 lignes.',
    '2. Explication détaillée : fonctionnement, responsabilité métier, cas d’utilisation.',
    '3. Preuves dans le code : classes, méthodes, fichiers et extraits utiles.',
    '4. Diagramme Mermaid si la question porte sur un flux, des dépendances ou un enchaînement.',
    '5. Impacts, limites et points d’attention pour une équipe projet.',
    '6. Sources : chemins exacts des fichiers réellement consultés.',
    '',
    'Règles de couverture des demandes :',
    '- Si la question demande une "section à part", crée un titre dédié et visible, pas seulement un paragraphe intégré.',
    '- Si la question demande "tous", "toutes", "exhaustif", "écrans", "courriers", "flux" ou "où se trouve", ajoute une cartographie par canal : écran/vue, contrôleur, service, calcul métier, document/courrier, flux ou batch, et indique clairement les recherches sans résultat.',
    '- Si la question demande un nombre d’exemples, fournis au moins ce nombre d’exemples concrets et compréhensibles par un lecteur métier.',
    '- Si la question demande les cas possibles avec des Oui/Non, produis une matrice de cas couvrant les combinaisons utiles et les effets attendus.',
    '',
    'Règles de rendu obligatoires :',
    '- Mermaid : liens simples (`A --> B`) ou libellés explicites (`A -->|Oui| B`) ; jamais de libellé vide (`A -->|| B`).',
    '- Booléens et valeurs techniques : écrire `true`, `false`, `null` en code inline ; ne jamais utiliser `<true>`, `<false>` ou des chevrons.',
  ].join('\n');
}

export function auditWorkDocumentQuality(document: WorkDocument): WorkDocumentQualityReport {
  const total = document.questions.length;
  const answeredQuestions = document.questions.filter(
    (question) => question.status === 'answered' && !!question.answer?.trim()
  );
  const answered = answeredQuestions.length;
  const errors = document.questions.filter((question) => question.status === 'error').length;
  const pending = document.questions.filter(
    (question) => question.status === 'pending' || question.status === 'answering'
  ).length;
  const sourceReferences = answeredQuestions.reduce(
    (count, question) => count + countSourceReferences(question.answer ?? ''),
    0
  );
  const sourceIndex = collectWorkDocumentSourceReferences(document);
  const diagrams = answeredQuestions.reduce(
    (count, question) => count + countMermaidDiagrams(question.answer ?? ''),
    0
  );
  const codeBlocks = answeredQuestions.reduce(
    (count, question) => count + countCodeBlocks(question.answer ?? ''),
    0
  );
  const shortAnswers = answeredQuestions.filter((question) =>
    isShortAnswer(question.answer ?? '')
  ).length;
  const answeredWithoutSources = answeredQuestions.filter(
    (question) => countSourceReferences(question.answer ?? '') === 0
  ).length;
  const renderArtifacts = answeredQuestions.filter((question) =>
    hasRenderArtifacts(normalizeWorkDocumentAnswer(question.answer ?? ''))
  ).length;

  const issues: WorkDocumentQualityIssue[] = [];
  if (total === 0) {
    issues.push({
      id: 'empty-document',
      label: 'Aucune question',
      severity: 'error',
      detail: 'Le document importé ne contient aucune question exploitable.',
    });
  }
  if (answered === 0 && total > 0) {
    issues.push({
      id: 'no-answer',
      label: 'Aucune réponse générée',
      severity: 'error',
      detail: 'Le livrable ne doit pas être finalisé avant au moins une réponse Code Explorer.',
    });
  }
  if (pending > 0) {
    issues.push({
      id: 'pending-questions',
      label: 'Questions restantes',
      severity: 'warning',
      detail: `${pending} question(s) ne sont pas encore traitées.`,
    });
  }
  if (errors > 0) {
    issues.push({
      id: 'failed-questions',
      label: 'Questions en erreur',
      severity: 'warning',
      detail: `${errors} question(s) ont échoué et doivent être relancées ou corrigées.`,
    });
  }
  if (answeredWithoutSources > 0) {
    issues.push({
      id: 'missing-sources',
      label: 'Sources insuffisantes',
      severity: 'warning',
      detail: `${answeredWithoutSources} réponse(s) ne citent aucun fichier source détectable.`,
    });
  }
  if (shortAnswers > 0) {
    issues.push({
      id: 'short-answers',
      label: 'Réponses trop courtes',
      severity: 'warning',
      detail: `${shortAnswers} réponse(s) semblent trop synthétiques pour un livrable technique.`,
    });
  }
  if (renderArtifacts > 0) {
    issues.push({
      id: 'render-artifacts',
      label: 'Rendu ou valeur à vérifier',
      severity: 'warning',
      detail: `${renderArtifacts} réponse(s) contiennent des artefacts de rendu ou des valeurs techniques à vérifier.`,
    });
  }
  if (answered > 0 && diagrams === 0) {
    issues.push({
      id: 'no-diagram',
      label: 'Aucun diagramme',
      severity: 'info',
      detail: 'Ajoute un diagramme Mermaid aux réponses qui décrivent un flux ou une dépendance.',
    });
  }

  const answeredRatio = total === 0 ? 0 : answered / total;
  const sourceRatio = answered === 0 ? 0 : (answered - answeredWithoutSources) / answered;
  const detailRatio = answered === 0 ? 0 : (answered - shortAnswers) / answered;
  const score = Math.max(
    0,
    Math.min(
      100,
      Math.round(
        answeredRatio * 45 +
          sourceRatio * 20 +
          detailRatio * 15 +
          (diagrams > 0 ? 10 : 0) +
          (errors === 0 ? 10 : 0) -
          renderArtifacts * 10
      )
    )
  );

  const blocking = issues.some((issue) => issue.severity === 'error');
  const level: WorkDocumentQualityLevel = blocking ? 'blocked' : score >= 80 ? 'ready' : 'review';

  return {
    level,
    score,
    checks: [
      {
        id: 'coverage',
        label: 'Couverture des questions',
        value: `${answered}/${total}`,
        ok: total > 0 && answered === total,
      },
      {
        id: 'sources',
        label: 'Fichiers sources cités',
        value: `${sourceIndex.length}`,
        ok: answered > 0 && answeredWithoutSources === 0,
      },
      {
        id: 'diagrams',
        label: 'Diagrammes Mermaid',
        value: `${diagrams}`,
        ok: diagrams > 0,
      },
      {
        id: 'code',
        label: 'Blocs de code',
        value: `${codeBlocks}`,
        ok: codeBlocks > 0,
      },
      {
        id: 'errors',
        label: 'Questions en erreur',
        value: `${errors}`,
        ok: errors === 0,
      },
    ],
    issues,
    summary: {
      total,
      answered,
      pending,
      errors,
      sourceReferences,
      sourceFiles: sourceIndex.length,
      diagrams,
      codeBlocks,
      shortAnswers,
      answeredWithoutSources,
      renderArtifacts,
    },
  };
}

export function normalizeWorkDocumentAnswer(answer: string): string {
  return repairOrphanHeadingMarkers(
    replaceEmptyInlineCodeSpans(repairMermaidFences(normalizeTechnicalValuePlaceholders(answer)))
  );
}

function normalizeTechnicalValuePlaceholders(answer: string): string {
  return answer.replace(/<\s*(true|false|null)\s*>/gi, (_match, value: string) => {
    return `\`${value.toLowerCase()}\``;
  });
}

export function buildWorkDocumentMarkdown(document: WorkDocument, createdAt = Date.now()): string {
  const quality = auditWorkDocumentQuality(document);
  const readiness = workDocumentReadinessLabel(document);
  const lines = [
    `# ${workDocumentExportTitle(document)}`,
    '',
    '| Métadonnée | Valeur |',
    '|---|---|',
    `| Projet | ${escapeMarkdownTableCell(document.repoName ?? document.repo ?? 'non sélectionné')} |`,
    `| Document source | ${escapeMarkdownTableCell(document.filename)} |`,
    `| Import | ${escapeMarkdownTableCell(new Date(document.importedAt).toLocaleString())} |`,
    `| Export | ${escapeMarkdownTableCell(new Date(createdAt).toLocaleString())} |`,
    `| Questions extraites | ${document.questions.length} |`,
    `| Questions répondues | ${quality.summary.answered} |`,
    `| Statut du livrable | ${escapeMarkdownTableCell(readiness)} |`,
    '',
    '> [!NOTE]',
    `> Statut : ${readiness}. Livrable généré depuis un document de travail importé dans Code Explorer Chat. Les réponses ci-dessous sont destinées à être relues puis intégrées dans un document final de niveau professionnel.`,
    '',
    '## Ce que contient ce livrable',
    '',
    '- Le document de travail source, enrichi quand Code Explorer a pu repositionner les réponses.',
    '- Une réponse détaillée par question, organisée comme un mini-chapitre technique.',
    '- Les sources réellement citées par les réponses et les diagrammes Mermaid détectés.',
    '- Un contrôle qualité documentaire pour préparer la relecture finale.',
    '',
    '## Parcours de lecture recommandé',
    '',
    '1. Lire la table des questions pour identifier les thèmes couverts.',
    '2. Relire les chapitres avec des avertissements qualité.',
    '3. Vérifier les fichiers cités dans les sections Sources.',
    '4. Générer le DOCX ou le PDF final après correction des questions restantes.',
    '',
    '## Table des questions',
    '',
    '| # | Question | État | Sources | Diagrammes |',
    '|---|---|---|---:|---:|',
  ];

  for (const question of document.questions) {
    lines.push(
      `| ${question.order} | ${escapeMarkdownTableCell(question.text)} | ${questionStatusLabel(question)} | ${countSourceReferences(
        question.answer ?? ''
      )} | ${countMermaidDiagrams(question.answer ?? '')} |`
    );
  }
  lines.push('');
  lines.push(buildReviewActionPlanMarkdown(document), '');

  if (document.sourceMarkdown?.trim()) {
    lines.push('## Document source enrichi', '');
    lines.push(buildSourceMarkdownWithAnswers(document), '');
  }

  lines.push('## Questions et réponses détaillées', '');

  for (const question of document.questions) {
    const generatedAt = question.answeredAt
      ? escapeMarkdownTableCell(new Date(question.answeredAt).toLocaleString())
      : '-';
    lines.push(`### Chapitre ${question.order} - ${question.label}`, '');
    lines.push('#### Question', '', `> ${question.text}`, '');
    lines.push('#### Trace Code Explorer', '');
    lines.push('| Élément | Valeur |', '|---|---|');
    lines.push(`| État | ${questionStatusLabel(question)} |`);
    lines.push(`| Génération | ${generatedAt} |`);
    lines.push(`| Session chat | ${escapeMarkdownTableCell(document.sessionId ?? '-')} |`);
    lines.push(
      `| Message assistant | ${escapeMarkdownTableCell(question.messageIds?.assistant ?? '-')} |`
    );
    lines.push('');
    if (question.context.trim()) {
      lines.push('#### Contexte documentaire', '', question.context.trim(), '');
    }
    lines.push('#### Réponse technique générée', '');
    if (question.answer?.trim()) {
      lines.push(indentAnswerHeadings(normalizeWorkDocumentAnswer(question.answer.trim())), '');
    } else if (question.error) {
      lines.push('> [!WARNING]', `> Réponse non générée : ${question.error}`, '');
    } else {
      lines.push('> [!NOTE]', '> Réponse non générée.', '');
    }
  }

  lines.push(buildSourceIndexMarkdown(document), '');
  lines.push(buildQualityMarkdown(quality), '');
  return lines.join('\n').trimEnd() + '\n';
}

function buildReviewActionPlanMarkdown(document: WorkDocument): string {
  const actions = collectReviewActions(document);
  const lines = ['## Plan de relecture final', ''];
  if (actions.length === 0) {
    lines.push(
      '> [!TIP]',
      '> Aucune action bloquante détectée. Relis les sources citées puis génère le DOCX ou le PDF final.',
      ''
    );
  } else {
    lines.push('| Question | État | Action recommandée |', '|---|---|---|');
    for (const action of actions) {
      lines.push(
        `| ${escapeMarkdownTableCell(action.question.label)} | ${questionStatusLabel(
          action.question
        )} | ${escapeMarkdownTableCell(action.action)} |`
      );
    }
    lines.push('');
  }

  lines.push(
    '### Checklist avant diffusion',
    '',
    '- Toutes les questions sont répondues ou explicitement exclues.',
    '- Chaque réponse cite les fichiers sources réellement consultés.',
    '- Les réponses courtes ont été enrichies avec les impacts, limites et preuves utiles.',
    '- Les diagrammes Mermaid importants ont été relus dans l’export HTML/PDF.',
    '- Le DOCX final a été ouvert une fois après génération.'
  );
  return lines.join('\n');
}

function collectReviewActions(
  document: WorkDocument
): Array<{ question: WorkDocumentQuestion; action: string }> {
  const actions: Array<{ question: WorkDocumentQuestion; action: string }> = [];
  for (const question of document.questions) {
    if (question.status === 'pending' || question.status === 'answering') {
      actions.push({
        question,
        action: 'Générer la réponse Code Explorer avant diffusion.',
      });
      continue;
    }
    if (question.status === 'error') {
      actions.push({
        question,
        action: question.error
          ? `Relancer la question après correction: ${question.error}`
          : 'Relancer la question après correction.',
      });
      continue;
    }
    if (!question.answer?.trim()) {
      actions.push({
        question,
        action: 'Vérifier pourquoi la réponse est absente.',
      });
      continue;
    }
    if (countSourceReferences(question.answer) === 0) {
      actions.push({
        question,
        action: 'Ajouter des sources exactes issues du code consulté.',
      });
    }
    if (isShortAnswer(question.answer)) {
      actions.push({
        question,
        action: 'Enrichir la réponse avec preuves, impacts et limites.',
      });
    }
    if (hasRenderArtifacts(normalizeWorkDocumentAnswer(question.answer))) {
      actions.push({
        question,
        action: 'Corriger les artefacts de rendu ou valeurs techniques à vérifier avant diffusion.',
      });
    }
  }
  return actions;
}

export function collectWorkDocumentSourceReferences(
  document: WorkDocument
): WorkDocumentSourceReferenceSummary[] {
  const byPath = new Map<string, WorkDocumentSourceReferenceSummary>();
  for (const question of document.questions) {
    if (!question.answer?.trim()) continue;
    for (const path of extractSourceReferences(question.answer)) {
      const existing = byPath.get(path);
      if (existing) {
        existing.count += 1;
        if (!existing.questionLabels.includes(question.label)) {
          existing.questionLabels.push(question.label);
        }
      } else {
        byPath.set(path, { path, count: 1, questionLabels: [question.label] });
      }
    }
  }
  return [...byPath.values()].sort((left, right) => {
    if (right.count !== left.count) return right.count - left.count;
    return left.path.localeCompare(right.path);
  });
}

export function workDocumentExportFilename(document: WorkDocument): string {
  return workDocumentExportFilenameFor(document, 'md');
}

export function workDocumentExportFilenameFor(
  document: WorkDocument,
  extension: 'md' | 'docx' | 'pdf' | 'html'
): string {
  return relatedSourcesFilename(
    document.repoName ?? document.repo ?? document.filename,
    `document-travail-${slugifyFilename(document.filename)}`
  ).replace(/\.md$/, `.${extension}`);
}

export function workDocumentReadinessLabel(document: WorkDocument): string {
  const quality = auditWorkDocumentQuality(document);
  if (quality.level === 'blocked') return 'Brouillon bloqué';
  if (quality.level === 'review') return 'Brouillon à relire';
  return 'Prêt pour relecture finale';
}

export function workDocumentExportTitle(document: WorkDocument): string {
  const readiness = workDocumentReadinessLabel(document);
  if (readiness === 'Prêt pour relecture finale') {
    return `Livrable Code Explorer - ${document.filename}`;
  }
  return `Livrable Code Explorer (${readiness}) - ${document.filename}`;
}

export function buildWorkDocumentPrintableHtml(
  document: WorkDocument,
  createdAt = Date.now()
): string {
  const title = workDocumentExportTitle(document);
  const markdown = buildWorkDocumentMarkdown(document, createdAt);
  const quality = auditWorkDocumentQuality(document);
  const readiness = workDocumentReadinessLabel(document);
  return `<!doctype html>
<html lang="fr">
<head>
  <meta charset="utf-8" />
  <title>${escapeHtml(title)}</title>
  <script src="https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.min.js"></script>
  <style>
    * { box-sizing: border-box; print-color-adjust: exact; -webkit-print-color-adjust: exact; }
    html { color-scheme: light; }
    body {
      margin: 0;
      background: #fff;
      color: #202124;
      font-family: Georgia, "Times New Roman", serif;
      font-size: 10.6pt;
      line-height: 1.62;
    }
    main, header { max-width: 820px; margin: 0 auto; }
    header { padding: 0 28px; }
    main { background: #fff; padding: 24px 30px 42px; }
    .cover {
      align-items: center;
      background: #fff;
      color: #202124;
      display: flex;
      flex-direction: column;
      justify-content: center;
      min-height: 960px;
      overflow: hidden;
      padding: 42px 34px;
      position: relative;
      text-align: center;
    }
    .cover::after {
      background: #1f4e79;
      content: "";
      height: 1.5px;
      left: 50%;
      position: absolute;
      top: 55%;
      transform: translateX(-50%);
      width: 108px;
    }
    .kicker { color: #a0a4aa; font-size: 8pt; font-weight: 700; letter-spacing: .42em; text-transform: uppercase; }
    h1 { color: #1f4e79; font-family: Georgia, "Times New Roman", serif; font-size: 24pt; line-height: 1.18; margin: 20px 0 14px; }
    .cover h1 { max-width: 720px; }
    .cover p { color: #3f3f46; font-size: 10.5pt; margin-bottom: 6px; max-width: 560px; }
    .cover-status { border: 1px solid #d8dee9; color: #1f4e79; display: inline-block; font-size: 8.8pt; font-weight: 700; letter-spacing: .08em; margin: 8px 0 10px; padding: 6px 12px; text-transform: uppercase; }
    .cover-meta { display: grid; gap: 0; grid-template-columns: repeat(4, minmax(0, 1fr)); margin-top: 130px; max-width: 520px; width: 100%; }
    .cover-stat { border-top: 1px solid #d8dee9; padding: 12px 8px 4px; }
    .cover-stat + .cover-stat { border-left: 1px solid #d8dee9; }
    .cover-stat strong { color: #1f4e79; display: block; font-size: 13pt; }
    .cover-stat span { color: #6b7280; display: block; font-size: 8pt; margin-top: 2px; }
    h2 { border-bottom: 1px solid #d8dee9; color: #1f4e79; font-family: Georgia, "Times New Roman", serif; font-size: 17pt; margin-top: 26px; padding-bottom: 7px; }
    h3 { color: #1f4e79; font-family: Georgia, "Times New Roman", serif; font-size: 13pt; margin-top: 22px; }
    h4 { color: #334155; font-size: 11pt; margin-top: 16px; }
    h2, h3, h4 { break-after: avoid; page-break-after: avoid; }
    p { margin: 0 0 10px; }
    a { color: #1d4ed8; text-decoration: none; }
    blockquote { border-left: 4px solid #8b5cf6; background: #f5f3ff; color: #334155; margin: 12px 0; padding: 9px 13px; }
    .callout { border: 1px solid #dbeafe; border-left-width: 5px; border-radius: 8px; margin: 14px 0; padding: 11px 14px; break-inside: avoid; }
    .callout-title { color: #0f172a; font-weight: 800; margin-bottom: 4px; }
    .callout-note { background: #eff6ff; border-color: #93c5fd; }
    .callout-tip { background: #ecfdf5; border-color: #5eead4; }
    .callout-warning { background: #fffbeb; border-color: #f59e0b; }
    .callout-danger { background: #fef2f2; border-color: #f87171; }
    code { font-family: ui-monospace, SFMono-Regular, Consolas, monospace; font-size: 9.4pt; }
    p code, li code, td code { background: #f1f5f9; border-radius: 4px; padding: 1px 4px; }
    pre { background: #f8fafc; border: 1px solid #e2e8f0; border-left: 4px solid #0891b2; border-radius: 7px; overflow-wrap: anywhere; padding: 10px 12px; white-space: pre-wrap; word-break: break-word; }
    table { border-collapse: collapse; font-size: 9.3pt; margin: 14px 0; width: 100%; }
    th, td { border: 1px solid #d1d5db; padding: 7px 9px; text-align: left; vertical-align: top; }
    th { background: #1f4e79; color: white; }
    tr:nth-child(even) td { background: #f8fafc; }
    tr { break-inside: avoid; }
    ul, ol { padding-left: 24px; }
    li { margin-bottom: 4px; }
    .print-diagram { background: #f8fafc; border: 1px solid #dbeafe; border-radius: 10px; margin: 16px 0; padding: 12px; break-inside: avoid; }
    .print-diagram figcaption { color: #0f766e; font-size: 8.5pt; font-weight: 800; letter-spacing: .04em; margin-bottom: 8px; text-transform: uppercase; }
    .mermaid { align-items: center; background: #fff; border: 1px solid #e2e8f0; border-radius: 8px; display: flex; justify-content: center; min-height: 90px; overflow: auto; padding: 12px; text-align: center; white-space: pre-wrap; }
    .mermaid-source summary { color: #475569; cursor: pointer; font-size: 8.5pt; font-weight: 700; margin-top: 8px; }
    .source-capture { break-inside: avoid; margin: 16px 0 20px; text-align: center; }
    .source-capture img { border: 1px solid #d1d5db; max-height: 235mm; max-width: 100%; object-fit: contain; }
    .source-capture figcaption { color: #64748b; font-size: 8.5pt; font-style: italic; margin-top: 6px; }
    @page { margin: 17mm; }
    @media print {
      body { background: #fff; }
      main, header { max-width: none; padding-left: 0; padding-right: 0; }
      main { border: 0; padding-top: 18px; }
      header { padding-top: 0; }
      .cover { break-after: page; min-height: 270mm; padding: 32mm 28mm; }
      .cover-meta { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    }
  </style>
</head>
<body data-document-profile="technical-book">
  <header>
    <div class="cover">
      <div class="kicker">Code Explorer document de travail</div>
      <h1>${escapeHtml(title)}</h1>
      <div class="cover-status">${escapeHtml(readiness)}</div>
      <p>Projet : ${escapeHtml(document.repoName ?? document.repo ?? 'non sélectionné')}</p>
      <div class="cover-meta">
        <div class="cover-stat"><strong>${document.questions.length}</strong><span>questions</span></div>
        <div class="cover-stat"><strong>${quality.summary.answered}</strong><span>réponses</span></div>
        <div class="cover-stat"><strong>${quality.summary.sourceFiles}</strong><span>fichiers sources</span></div>
        <div class="cover-stat"><strong>${escapeHtml(readiness)}</strong><span>statut</span></div>
      </div>
      <p>Export : ${escapeHtml(new Date(createdAt).toLocaleString())}</p>
    </div>
  </header>
  <main>
    ${workMarkdownToHtml(markdown)}
  </main>
  <script>
    (function () {
      function revealMermaidSource() {
        document.querySelectorAll('.mermaid-source').forEach(function (details) {
          details.open = true;
        });
      }
      if (!window.mermaid) {
        revealMermaidSource();
        return;
      }
      window.mermaid.initialize({
        startOnLoad: false,
        securityLevel: 'strict',
        theme: 'base',
        themeVariables: {
          primaryColor: '#e0f2fe',
          primaryBorderColor: '#0284c7',
          primaryTextColor: '#0f172a',
          lineColor: '#64748b',
          secondaryColor: '#f0fdfa',
          tertiaryColor: '#fff7ed'
        }
      });
      Promise.resolve(window.mermaid.run({ querySelector: '.mermaid' })).catch(revealMermaidSource);
    })();
  </script>
</body>
</html>`;
}

function buildQualityMarkdown(report: WorkDocumentQualityReport): string {
  const levelLabel =
    report.level === 'ready'
      ? 'Prêt pour relecture finale'
      : report.level === 'blocked'
        ? 'Bloqué'
        : 'À relire';
  const calloutType =
    report.level === 'ready' ? 'TIP' : report.level === 'blocked' ? 'WARNING' : 'NOTE';
  const lines = [
    '## Contrôle qualité documentaire',
    '',
    `> [!${calloutType}]`,
    `> ${levelLabel}.`,
    '',
    '| Contrôle | Valeur | Statut |',
    '|---|---:|---|',
  ];
  for (const check of report.checks) {
    lines.push(
      `| ${escapeMarkdownTableCell(check.label)} | ${escapeMarkdownTableCell(check.value)} | ${check.ok ? 'OK' : 'À vérifier'} |`
    );
  }
  if (report.issues.length > 0) {
    lines.push('', '### Points de relecture', '');
    for (const issue of report.issues) {
      lines.push(`- **${issue.label}** : ${issue.detail}`);
    }
  }
  return lines.join('\n');
}

function buildSourceIndexMarkdown(document: WorkDocument): string {
  const sources = collectWorkDocumentSourceReferences(document);
  const lines = ['## Index des sources citées', ''];
  if (sources.length === 0) {
    lines.push(
      '> [!WARNING]',
      '> Aucun fichier source n’a été détecté dans les réponses. Relance ou enrichis les réponses avant diffusion.',
      ''
    );
    return lines.join('\n').trimEnd();
  }

  lines.push('| Fichier source | Questions | Occurrences |', '|---|---|---:|');
  for (const source of sources) {
    lines.push(
      `| ${escapeMarkdownTableCell(source.path)} | ${escapeMarkdownTableCell(
        source.questionLabels.join(', ')
      )} | ${source.count} |`
    );
  }
  return lines.join('\n');
}

function slugifyFilename(value: string): string {
  return (
    value
      .replace(/\.[^.]+$/, '')
      .toLowerCase()
      .normalize('NFD')
      .replace(/[\u0300-\u036f]/g, '')
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+|-+$/g, '')
      .slice(0, 48) || 'questions'
  );
}

function workMarkdownToHtml(markdown: string): string {
  const lines = markdown.replace(/\r\n?/g, '\n').split('\n');
  const html: string[] = [];
  let paragraph: string[] = [];
  let code: string[] = [];
  let list: string[] = [];
  let listType: 'ul' | 'ol' | null = null;
  let inCode = false;
  let codeLanguage = '';

  const flushParagraph = () => {
    const text = paragraph.join('\n').trim();
    if (text) html.push(`<p>${inlineMarkdownToHtml(text).replace(/\n/g, '<br />')}</p>`);
    paragraph = [];
  };
  const flushList = () => {
    if (list.length > 0 && listType) {
      html.push(
        `<${listType}>${list.map((item) => `<li>${inlineMarkdownToHtml(item)}</li>`).join('')}</${listType}>`
      );
      list = [];
      listType = null;
    }
  };
  const flushCode = () => {
    const source = code.join('\n');
    if (isMermaidLanguage(codeLanguage)) {
      const repairedSource = repairMermaidSource(source);
      html.push(
        `<figure class="print-diagram print-diagram-mermaid"><figcaption>Diagramme Mermaid</figcaption><div class="mermaid">${escapeHtml(repairedSource)}</div><details class="mermaid-source"><summary>Source Mermaid</summary><pre><code class="language-mermaid">${escapeHtml(repairedSource)}</code></pre></details></figure>`
      );
    } else {
      html.push(
        `<pre><code${codeLanguage ? ` class="language-${escapeHtml(codeLanguage)}"` : ''}>${escapeHtml(source)}</code></pre>`
      );
    }
    code = [];
    codeLanguage = '';
    inCode = false;
  };
  const pushListItem = (type: 'ul' | 'ol', item: string) => {
    if (listType && listType !== type) flushList();
    listType = type;
    list.push(item);
  };

  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    const trimmed = line.trim();
    const fence = /^```([^\s`]*)?/.exec(trimmed);
    if (fence && !inCode) {
      flushParagraph();
      flushList();
      inCode = true;
      codeLanguage = fence[1] ?? '';
      continue;
    }
    if (trimmed === '```' && inCode) {
      flushCode();
      continue;
    }
    if (inCode) {
      code.push(line);
      continue;
    }

    const heading = /^(#{1,4})\s+(.+)$/.exec(trimmed);
    if (heading) {
      flushParagraph();
      flushList();
      const level = heading[1].length;
      html.push(`<h${level}>${inlineMarkdownToHtml(heading[2])}</h${level}>`);
      continue;
    }

    if (trimmed.startsWith('|') && trimmed.endsWith('|')) {
      flushParagraph();
      flushList();
      const table = collectMarkdownTable(lines, index);
      html.push(markdownTableToHtml(table.rows));
      index = table.endIndex - 1;
      continue;
    }

    const image = parseMarkdownImageLine(trimmed);
    if (image) {
      flushParagraph();
      flushList();
      html.push(
        `<figure class="source-capture"><img src="${escapeHtml(image.src)}" alt="${escapeHtml(
          image.alt
        )}" /><figcaption>${escapeHtml(image.alt)}</figcaption></figure>`
      );
      continue;
    }

    const callout = /^>\s*\[!(NOTE|TIP|WARNING|DANGER|INFO)\]\s*(.*)$/i.exec(trimmed);
    if (callout) {
      flushParagraph();
      flushList();
      const collected = collectBlockquoteLines(lines, index + 1);
      html.push(calloutToHtml(callout[1], callout[2], collected.lines));
      index = collected.endIndex - 1;
      continue;
    }

    if (trimmed.startsWith('>')) {
      flushParagraph();
      flushList();
      const collected = collectBlockquoteLines(lines, index);
      html.push(
        `<blockquote>${collected.lines.map((item) => inlineMarkdownToHtml(item)).join('<br />')}</blockquote>`
      );
      index = collected.endIndex - 1;
      continue;
    }

    const bullet = /^[-*]\s+(.+)$/.exec(trimmed);
    if (bullet) {
      flushParagraph();
      pushListItem('ul', bullet[1]);
      continue;
    }

    const ordered = /^\d+[.)]\s+(.+)$/.exec(trimmed);
    if (ordered) {
      flushParagraph();
      pushListItem('ol', ordered[1]);
      continue;
    }

    if (!trimmed) {
      flushParagraph();
      flushList();
      continue;
    }
    paragraph.push(line);
  }
  if (inCode) flushCode();
  flushParagraph();
  flushList();
  return html.join('\n');
}

function parseMarkdownImageLine(line: string): { alt: string; src: string } | null {
  const match = /^!\[([^\]]*)\]\((data:image\/(?:png|jpeg|jpg|webp);base64,[^)]+)\)$/.exec(
    line.trim()
  );
  if (!match) return null;
  return {
    alt: match[1].trim() || 'Capture fonctionnelle',
    src: match[2],
  };
}

function collectBlockquoteLines(
  allLines: string[],
  startIndex: number
): { lines: string[]; endIndex: number } {
  const lines: string[] = [];
  let index = startIndex;
  for (; index < allLines.length; index += 1) {
    const trimmed = allLines[index].trim();
    if (!trimmed.startsWith('>')) break;
    lines.push(trimmed.replace(/^>\s?/, ''));
  }
  return { lines, endIndex: index };
}

function calloutToHtml(type: string, title: string, lines: string[]): string {
  const normalized = type.toLowerCase() === 'info' ? 'note' : type.toLowerCase();
  const defaultTitle =
    {
      note: 'Note',
      tip: 'Conseil',
      warning: 'Point d’attention',
      danger: 'Risque',
    }[normalized] ?? 'Note';
  const content = lines.length > 0 ? lines : [title.trim()].filter(Boolean);
  return `<aside class="callout callout-${escapeHtml(normalized)}"><div class="callout-title">${escapeHtml(
    title.trim() || defaultTitle
  )}</div>${content
    .filter((line) => line.trim())
    .map((line) => `<p>${inlineMarkdownToHtml(line)}</p>`)
    .join('')}</aside>`;
}

function collectMarkdownTable(
  allLines: string[],
  startIndex: number
): { rows: string[][]; endIndex: number } {
  const rows: string[][] = [];
  let index = startIndex;
  for (; index < allLines.length; index += 1) {
    const line = allLines[index].trim();
    if (!line.startsWith('|') || !line.endsWith('|')) break;
    const cells = line
      .slice(1, -1)
      .split('|')
      .map((cell) => cell.trim());
    if (cells.every((cell) => /^:?-{3,}:?$/.test(cell))) continue;
    rows.push(cells);
  }
  return { rows, endIndex: index };
}

function markdownTableToHtml(rows: string[][]): string {
  if (rows.length === 0) return '';
  const [head, ...body] = rows;
  return `<table><thead><tr>${head.map((cell) => `<th>${inlineMarkdownToHtml(cell)}</th>`).join('')}</tr></thead><tbody>${body
    .map(
      (row) => `<tr>${row.map((cell) => `<td>${inlineMarkdownToHtml(cell)}</td>`).join('')}</tr>`
    )
    .join('')}</tbody></table>`;
}

function inlineMarkdownToHtml(value: string): string {
  return escapeHtml(value)
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/\*([^*]+)\*/g, '<em>$1</em>')
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, (_match, text: string, url: string) => {
      const href = url.trim();
      if (!/^(https?:|#|\.?\/)/i.test(href)) return text;
      return `<a href="${escapeHtml(href)}">${text}</a>`;
    });
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function buildSourceMarkdownWithAnswers(document: WorkDocument): string {
  const answeredById = new Set<string>();
  const lines: string[] = [];
  for (const line of document.sourceMarkdown?.split(/\r?\n/) ?? []) {
    lines.push(line);
    const question = document.questions.find(
      (candidate) =>
        candidate.answer?.trim() &&
        !answeredById.has(candidate.id) &&
        lineMatchesQuestion(line, candidate)
    );
    if (!question) continue;
    answeredById.add(question.id);
    lines.push('', `> Réponse Code Explorer générée pour ${question.label}`, '');
    lines.push(indentAnswerHeadings(normalizeWorkDocumentAnswer(question.answer!.trim())), '');
  }
  const missed = document.questions.filter(
    (question) => question.answer?.trim() && !answeredById.has(question.id)
  );
  if (missed.length > 0) {
    lines.push('', '### Réponses générées non repositionnées automatiquement', '');
    for (const question of missed) {
      lines.push(
        `#### ${question.label} - ${question.text}`,
        '',
        indentAnswerHeadings(normalizeWorkDocumentAnswer(question.answer!.trim())),
        ''
      );
    }
  }
  return lines.join('\n').trimEnd();
}

function lineMatchesQuestion(line: string, question: WorkDocumentQuestion): boolean {
  const normalizedLine = normalizeText(line);
  if (!normalizedLine) return false;
  const normalizedLabel = normalizeText(question.label);
  const normalizedText = normalizeText(question.text);
  return (
    (!!normalizedLabel && normalizedLine.includes(normalizedLabel)) ||
    (!!normalizedText && normalizedLine.includes(normalizedText))
  );
}

function normalizeText(value: string): string {
  return value
    .toLowerCase()
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[^a-z0-9]+/g, ' ')
    .trim();
}

function indentAnswerHeadings(markdown: string): string {
  return markdown.replace(/^(#{1,6})\s+/gm, (_match, marks: string) => {
    return `${'#'.repeat(Math.min(6, marks.length + 2))} `;
  });
}

function repairMermaidFences(markdown: string): string {
  const lines = markdown.replace(/\r\n?/g, '\n').split('\n');
  const output: string[] = [];
  let inFence = false;
  let codeLanguage = '';
  let code: string[] = [];

  for (const line of lines) {
    const fence = /^```([^\s`]*)?/.exec(line.trim());
    if (fence && !inFence) {
      inFence = true;
      codeLanguage = fence[1] ?? '';
      code = [];
      output.push(line);
      continue;
    }
    if (line.trim() === '```' && inFence) {
      const source = code.join('\n');
      output.push(
        ...(isMermaidLanguage(codeLanguage) ? repairMermaidSource(source).split('\n') : code)
      );
      output.push(line);
      inFence = false;
      codeLanguage = '';
      code = [];
      continue;
    }
    if (inFence) {
      code.push(line);
    } else {
      output.push(line);
    }
  }

  if (inFence) {
    output.push(...code);
  }
  return output.join('\n');
}

function hasRenderArtifacts(answer: string): boolean {
  return (
    answer.includes('-->||') ||
    hasEmptyInlineCodeSpan(answer) ||
    answer.includes('`valeur à vérifier`')
  );
}

function hasEmptyInlineCodeSpan(answer: string): boolean {
  return /(^|[^`])``(?!`)/.test(stripFencedCodeBlocks(answer));
}

function stripFencedCodeBlocks(markdown: string): string {
  const lines = markdown.replace(/\r\n?/g, '\n').split('\n');
  let inFence = false;
  return lines
    .filter((line) => {
      if (/^```/.test(line.trim())) {
        inFence = !inFence;
        return false;
      }
      return !inFence;
    })
    .join('\n');
}

function replaceEmptyInlineCodeSpans(markdown: string): string {
  const lines = markdown.replace(/\r\n?/g, '\n').split('\n');
  let inFence = false;
  return lines
    .map((line) => {
      if (/^```/.test(line.trim())) {
        inFence = !inFence;
        return line;
      }
      if (inFence) return line;
      return line.replace(/(^|[^`])``(?!`)/g, '$1`valeur à vérifier`');
    })
    .join('\n');
}

function repairOrphanHeadingMarkers(markdown: string): string {
  const lines = markdown.replace(/\r\n?/g, '\n').split('\n');
  let inFence = false;
  return lines
    .map((line) => {
      if (/^```/.test(line.trim())) {
        inFence = !inFence;
        return line;
      }
      if (inFence) return line;
      return line.replace(/^(#{1,6})\s+\.\s+/, '$1 ');
    })
    .join('\n');
}

function questionStatusLabel(question: WorkDocumentQuestion): string {
  if (question.status === 'answered' && question.answer?.trim()) return 'Répondue';
  if (question.status === 'error') return 'Erreur';
  if (question.status === 'answering') return 'En cours';
  return 'À traiter';
}

function escapeMarkdownTableCell(value: string): string {
  return value.replace(/\|/g, '\\|').replace(/\r?\n/g, '<br>');
}

function countSourceReferences(answer: string): number {
  return extractSourceReferences(answer).length;
}

function extractSourceReferences(answer: string): string[] {
  const matches = new Set<string>();
  const pathPattern =
    /(?:^|[\s([`])((?:[A-Za-z]:\\)?(?:[\w .-]+[\\/])+[\w .@-]+\.(?:cs|cshtml|razor|ts|tsx|js|jsx|rs|py|sql|json|xml|config|md|yaml|yml)|(?:[\w@.-]+\/)+[\w@.-]+\.(?:cs|cshtml|razor|ts|tsx|js|jsx|rs|py|sql|json|xml|config|md|yaml|yml))/g;
  let match: RegExpExecArray | null;
  while ((match = pathPattern.exec(answer)) !== null) {
    matches.add(
      match[1]
        .trim()
        .replace(/^[-*]\s+/, '')
        .replace(/\\/g, '/')
    );
  }
  return [...matches].sort((left, right) => left.localeCompare(right));
}

function countMermaidDiagrams(answer: string): number {
  return countFencedCodeBlocks(answer, (language) => isMermaidLanguage(language));
}

function countCodeBlocks(answer: string): number {
  return countFencedCodeBlocks(answer, (language) => !isMermaidLanguage(language));
}

function isShortAnswer(answer: string): boolean {
  const words = answer.trim().split(/\s+/).filter(Boolean).length;
  return words > 0 && words < 140;
}

function isMermaidLanguage(language: string): boolean {
  return /^(mermaid|mmd)$/i.test(language.trim());
}

function countFencedCodeBlocks(answer: string, predicate: (language: string) => boolean): number {
  let count = 0;
  let inFence = false;
  for (const line of answer.replace(/\r\n?/g, '\n').split('\n')) {
    const fence = /^```([^\s`]*)?/.exec(line.trim());
    if (!fence) continue;
    if (inFence) {
      inFence = false;
      continue;
    }
    inFence = true;
    if (predicate(fence[1] ?? '')) count += 1;
  }
  return count;
}
