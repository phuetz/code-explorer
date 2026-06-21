import { describe, expect, it } from 'vitest';
import { reformulateChatPrompt } from './prompt-rewrite';

describe('reformulateChatPrompt', () => {
  it('keeps an empty prompt empty', () => {
    expect(reformulateChatPrompt('   ', 'sample-app')).toBe('');
  });

  it('turns a draft into a structured, source-focused prompt', () => {
    const rewritten = reformulateChatPrompt('explique StackLogger', 'sample-app');

    expect(rewritten).toContain('Question à traiter : explique StackLogger');
    expect(rewritten).toContain('dans le dépôt sample-app');
    expect(rewritten).toContain('fichiers, classes, méthodes ou symboles');
    expect(rewritten).toContain('diagramme Mermaid');
    expect(rewritten).toContain('A -->|Oui| B');
    expect(rewritten).toContain('`true`, `false`, `null`');
    expect(rewritten).toContain('section Sources');
    expect(rewritten).toContain("n'invente aucun fichier");
  });

  it('does not wrap a prompt that is already structured', () => {
    const prompt = [
      'Question à traiter : explique StackLogger',
      '',
      'Réponse attendue :',
      '- Cite les sources.',
    ].join('\n');

    expect(reformulateChatPrompt(prompt, 'sample-app')).toBe(prompt);
  });
});
