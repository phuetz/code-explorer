const STRUCTURED_PROMPT_MARKERS = [
  'Question a traiter :',
  'Question à traiter :',
  'Reponse attendue :',
  'Réponse attendue :',
  'Contraintes de fiabilite :',
  'Contraintes de fiabilité :',
];

function isAlreadyStructuredPrompt(value: string) {
  return STRUCTURED_PROMPT_MARKERS.some((marker) =>
    value.toLocaleLowerCase('fr-FR').includes(marker.toLocaleLowerCase('fr-FR'))
  );
}

export function reformulateChatPrompt(input: string, repositoryName?: string | null) {
  const question = input.trim();
  if (!question) return '';
  if (isAlreadyStructuredPrompt(question)) return question;

  const repoScope = repositoryName
    ? `dans le dépôt ${repositoryName}`
    : 'dans le dépôt actuellement sélectionné';

  return [
    `Question à traiter : ${question}`,
    '',
    `Contexte : réponds en français ${repoScope}. Utilise les outils Code Explorer et lis les fichiers nécessaires avant de conclure.`,
    '',
    'Réponse attendue :',
    '- Commence par une synthèse courte.',
    '- Identifie les fichiers, classes, méthodes ou symboles réellement concernés.',
    '- Explique le raisonnement étape par étape avec les extraits de code utiles.',
    '- Ajoute un diagramme Mermaid si la question porte sur un flux, des dépendances ou une architecture.',
    '- Pour Mermaid, utilise des liens simples (`A --> B`) ou des libellés explicites (`A -->|Oui| B`) ; n’utilise jamais de libellé vide comme `A -->|| B`.',
    '- Pour les booléens, écris toujours les valeurs en code inline (`true`, `false`, `null`) et jamais sous forme HTML ou entre chevrons.',
    '- Termine par une section Sources avec les chemins exacts des fichiers cités.',
    '',
    "Contraintes de fiabilité : n'invente aucun fichier, symbole, ligne ou comportement. Si une information n'est pas prouvée par les outils ou les fichiers lus, dis-le clairement.",
  ].join('\n');
}
