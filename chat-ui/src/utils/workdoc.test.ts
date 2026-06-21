import { describe, expect, it } from 'vitest';
import type { WorkDocument } from '../types/chat';
import {
  auditWorkDocumentQuality,
  buildWorkDocumentPrintableHtml,
  buildWorkDocumentMarkdown,
  buildWorkQuestionPrompt,
  collectWorkDocumentSourceReferences,
  normalizeWorkDocumentAnswer,
  workDocumentExportTitle,
  workDocumentExportFilename,
  workDocumentExportFilenameFor,
  workDocumentReadinessLabel,
} from './workdoc';

describe('workdoc utilities', () => {
  const document: WorkDocument = {
    id: 'doc1',
    filename: 'Questions Sample.docx',
    importedAt: 1774500000000,
    repo: 'repo_alise',
    repoName: 'sample-app',
    sessionId: 'session-atelier-word',
    sourceBytes: 1234,
    markdownChars: 5678,
    sourceMarkdown: [
      '# Document de travail',
      '',
      'Q1.1 — À quoi sert StackLogger ?',
      '',
      'Ancienne réponse courte.',
    ].join('\n'),
    questions: [
      {
        id: 'q-001',
        order: 1,
        label: 'Q1.1',
        text: 'À quoi sert StackLogger ?',
        context: 'Question issue du chapitre journalisation.',
        status: 'answered',
        answer: [
          'StackLogger sert à tracer les méthodes métier et à relier une exécution applicative à une méthode précise du code. Il est utile pour diagnostiquer un comportement instable, retrouver les appels importants et documenter les flux avec une preuve vérifiable. La réponse doit donc citer les endroits où le mécanisme est appelé et expliquer comment ces traces deviennent exploitables dans Code Explorer.',
          '',
          '```csharp',
          'using (StackLogger.BeginMethodScope())',
          '{',
          '    service.Executer();',
          '}',
          '```',
          '',
          '```mermaid',
          'flowchart TD',
          '  Code --> Trace',
          '  Trace --> Code Explorer',
          '```',
          '',
          '## Sources',
          '- Acme.Sample/Logging/StackLogger.cs',
        ].join('\n'),
        answeredAt: 1774500100000,
        messageIds: {
          assistant: 'assistant-message-1',
        },
      },
    ],
  };

  it('builds a project-aware structured prompt for a question', () => {
    const prompt = buildWorkQuestionPrompt({
      document,
      question: document.questions[0],
      repositoryName: 'sample-app',
    });

    expect(prompt).toContain('Atelier document Code Explorer : Questions Sample.docx');
    expect(prompt).toContain('Question à traiter : À quoi sert StackLogger ?');
    expect(prompt).toContain('dans le dépôt sample-app');
    expect(prompt).toContain('Question issue du chapitre journalisation.');
    expect(prompt).toContain('Cite les chemins exacts');
    expect(prompt).toContain('Si la question demande une "section à part"');
    expect(prompt).toContain('ajoute une cartographie par canal');
    expect(prompt).toContain('fournis au moins ce nombre d’exemples concrets');
    expect(prompt).toContain('produis une matrice de cas');
    expect(prompt).toContain('jamais de libellé vide (`A -->|| B`)');
    expect(prompt).toContain('écrire `true`, `false`, `null` en code inline');
  });

  it('exports answered questions as a final markdown deliverable', () => {
    const markdown = buildWorkDocumentMarkdown(document, 1774510000000);

    expect(markdown).toContain('# Livrable Code Explorer - Questions Sample.docx');
    expect(markdown).toContain('| Questions répondues | 1 |');
    expect(markdown).toContain('| Statut du livrable | Prêt pour relecture finale |');
    expect(markdown).toContain('## Document source enrichi');
    expect(markdown).toContain('> Réponse Code Explorer générée pour Q1.1');
    expect(markdown).toContain('## Plan de relecture final');
    expect(markdown).toContain(
      '| Q1.1 | Répondue | Enrichir la réponse avec preuves, impacts et limites. |'
    );
    expect(markdown).toContain('### Chapitre 1 - Q1.1');
    expect(markdown).toContain('#### Trace Code Explorer');
    expect(markdown).toContain('| Session chat | session-atelier-word |');
    expect(markdown).toContain('| Message assistant | assistant-message-1 |');
    expect(markdown).toContain('#### Sources');
    expect(markdown).toContain('StackLogger sert à tracer les méthodes métier');
    expect(markdown).toContain('## Index des sources citées');
    expect(markdown).toContain('| Acme.Sample/Logging/StackLogger.cs | Q1.1 | 1 |');
    expect(markdown).toContain('## Contrôle qualité documentaire');
  });

  it('adds review actions for incomplete or weak answers', () => {
    const reviewDocument: WorkDocument = {
      ...document,
      questions: [
        {
          id: 'q-pending',
          order: 1,
          label: 'Q1',
          text: 'Question à traiter ?',
          context: '',
          status: 'pending',
        },
        {
          id: 'q-error',
          order: 2,
          label: 'Q2',
          text: 'Question échouée ?',
          context: '',
          status: 'error',
          error: 'Backend indisponible',
        },
        {
          id: 'q-short',
          order: 3,
          label: 'Q3',
          text: 'Question trop courte ?',
          context: '',
          status: 'answered',
          answer: 'Réponse courte sans source.',
        },
        {
          id: 'q-render',
          order: 4,
          label: 'Q4',
          text: 'Question avec rendu cassé ?',
          context: '',
          status: 'answered',
          answer: [
            'Réponse documentée avec un artefact Markdown `` à corriger.',
            '',
            '## Sources',
            '- Acme.Sample/Rendu/Export.cs',
          ].join('\n'),
        },
      ],
    };

    const markdown = buildWorkDocumentMarkdown(reviewDocument, 1774510000000);

    expect(markdown).toContain('| Q1 | À traiter | Générer la réponse Code Explorer avant diffusion. |');
    expect(markdown).toContain(
      '| Q2 | Erreur | Relancer la question après correction: Backend indisponible |'
    );
    expect(markdown).toContain(
      '| Q3 | Répondue | Ajouter des sources exactes issues du code consulté. |'
    );
    expect(markdown).toContain(
      '| Q3 | Répondue | Enrichir la réponse avec preuves, impacts et limites. |'
    );
    expect(markdown).toContain(
      '| Q4 | Répondue | Corriger les artefacts de rendu ou valeurs techniques à vérifier avant diffusion. |'
    );
    expect(markdown).toContain('Réponse documentée avec un artefact Markdown `valeur à vérifier`');
    expect(markdown).toContain('- Le DOCX final a été ouvert une fois après génération.');
    expect(workDocumentReadinessLabel(reviewDocument)).toBe('Brouillon à relire');
    expect(workDocumentExportTitle(reviewDocument)).toBe(
      'Livrable Code Explorer (Brouillon à relire) - Questions Sample.docx'
    );
  });

  it('normalizes generated answers before rendering or exporting them', () => {
    const normalized = normalizeWorkDocumentAnswer(
      [
        'La valeur <true> active le traitement et <false> le désactive.',
        '## . Synthèse courte',
        '- `` : valeur absente générée par le modèle.',
        '',
        '```mermaid',
        'flowchart TD',
        '  A[Début] -->|| B[Fin]',
        '```',
      ].join('\n')
    );

    expect(normalized).toContain('La valeur `true` active le traitement');
    expect(normalized).toContain('## Synthèse courte');
    expect(normalized).not.toContain('## . Synthèse courte');
    expect(normalized).toContain('`false` le désactive');
    expect(normalized).toContain('- `valeur à vérifier` : valeur absente');
    expect(normalized).not.toContain('<true>');
    expect(normalized).not.toContain('`` :');
    expect(normalized).not.toContain('-->||');
    expect(normalized).toContain('A[Début] --> B[Fin]');
  });

  it('audits source references, diagrams and code blocks', () => {
    const report = auditWorkDocumentQuality(document);

    expect(report.summary.answered).toBe(1);
    expect(report.summary.sourceReferences).toBe(1);
    expect(report.summary.sourceFiles).toBe(1);
    expect(report.summary.diagrams).toBe(1);
    expect(report.summary.codeBlocks).toBe(1);
    expect(report.summary.renderArtifacts).toBe(0);
    expect(report.score).toBeGreaterThan(70);
  });

  it('collects a source index for the final review', () => {
    expect(collectWorkDocumentSourceReferences(document)).toEqual([
      {
        path: 'Acme.Sample/Logging/StackLogger.cs',
        count: 1,
        questionLabels: ['Q1.1'],
      },
    ]);
  });

  it('generates a safe markdown export filename', () => {
    expect(workDocumentExportFilename(document)).toMatch(
      /^code-explorer-document-travail-questions-alise-alise-v2-\d{8}-\d{6}\.md$/
    );
    expect(workDocumentExportFilenameFor(document, 'docx')).toMatch(/\.docx$/);
    expect(workDocumentExportFilenameFor(document, 'pdf')).toMatch(/\.pdf$/);
    expect(workDocumentExportFilenameFor(document, 'html')).toMatch(/\.html$/);
  });

  it('builds printable HTML for native PDF export', () => {
    const html = buildWorkDocumentPrintableHtml(document, 1774510000000);

    expect(html).toContain('<!doctype html>');
    expect(html).toContain('Code Explorer document de travail');
    expect(html).toContain('Livrable Code Explorer - Questions Sample.docx');
    expect(html).toContain('data-document-profile="technical-book"');
    expect(html).toContain('font-family: Georgia');
    expect(html).not.toContain('linear-gradient(135deg');
    expect(html).toContain('class="callout callout-note"');
    expect(html).toContain('print-diagram-mermaid');
    expect(html).toContain('StackLogger sert à tracer les méthodes métier');
  });
});
