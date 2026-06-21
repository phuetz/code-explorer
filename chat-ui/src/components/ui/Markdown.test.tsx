import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { Markdown } from './Markdown';
import { normalizeBareMermaid, normalizeCodeFenceLanguage } from '../../utils/markdown';
import { linkifySourceReferences } from '../../utils/source-references';

vi.mock('./MermaidBlock', () => ({
  MermaidBlock: ({ text }: { text: string }) => (
    <div data-testid="mermaid-block">{text}</div>
  ),
}));

describe('normalizeBareMermaid', () => {
  it('wraps a bare flowchart block so Mermaid can render it', () => {
    const normalized = normalizeBareMermaid(`Flux global
flowchart TD
    A[Controller] --> B[Service]
    B --> C[PDF]
Étapes détaillées
Le texte continue.`);

    expect(normalized).toContain('```mermaid\nflowchart TD');
    expect(normalized).toContain('    A[Controller] --> B[Service]');
    expect(normalized).toContain('```\nÉtapes détaillées');
  });

  it('does not double-wrap an already fenced Mermaid block', () => {
    const markdown = `\`\`\`mermaid
flowchart TD
    A --> B
\`\`\``;

    expect(normalizeBareMermaid(markdown)).toBe(markdown);
  });

  it('keeps unindented sequence diagram arrows inside the Mermaid block', () => {
    const normalized = normalizeBareMermaid(`Flux détaillé
sequenceDiagram
MVC->>BAL: CreerCourrierMasse
BAL-->>MVC: PDF fusionné
Étapes détaillées`);

    expect(normalized).toContain('```mermaid\nsequenceDiagram');
    expect(normalized).toContain('MVC->>BAL: CreerCourrierMasse');
    expect(normalized).toContain('BAL-->>MVC: PDF fusionné');
    expect(normalized).toContain('```\nÉtapes détaillées');
  });

  it('keeps class diagram members and closing braces inside the Mermaid block', () => {
    const normalized = normalizeBareMermaid(`Vue classes
classDiagram
class CourrierController {
+ImprimerListeCourrierMasse()
}
CourrierController --> CourriersService
Sources`);

    expect(normalized).toContain('```mermaid\nclassDiagram');
    expect(normalized).toContain('class CourrierController {');
    expect(normalized).toContain('+ImprimerListeCourrierMasse()');
    expect(normalized).toContain('}\nCourrierController --> CourriersService');
    expect(normalized).toContain('```\nSources');
  });
});

describe('normalizeCodeFenceLanguage', () => {
  it('maps common LLM language aliases to highlighter names', () => {
    expect(normalizeCodeFenceLanguage('cs')).toBe('csharp');
    expect(normalizeCodeFenceLanguage('c#')).toBe('csharp');
    expect(normalizeCodeFenceLanguage('ts')).toBe('typescript');
    expect(normalizeCodeFenceLanguage('js')).toBe('javascript');
    expect(normalizeCodeFenceLanguage('ps1')).toBe('powershell');
    expect(normalizeCodeFenceLanguage('shell')).toBe('bash');
  });

  it('keeps unknown language tags instead of dropping highlighting', () => {
    expect(normalizeCodeFenceLanguage('rust')).toBe('rust');
    expect(normalizeCodeFenceLanguage(undefined)).toBeUndefined();
  });
});

describe('Markdown Mermaid rendering', () => {
  it('routes bare Mermaid flowcharts to MermaidBlock', () => {
    render(
      <Markdown>{`Flux global
flowchart TD
    A[Controller] --> B[Service]
Étapes détaillées`}</Markdown>
    );

    expect(screen.getByTestId('mermaid-block').textContent).toContain('flowchart TD');
    expect(screen.getByText('Étapes détaillées')).toBeTruthy();
  });

  it('treats common Mermaid typo aliases as diagrams', () => {
    const markdown = [
      '```maimaid',
      'sequenceDiagram',
      '    MVC->>BAL: CreerCourrierMasse',
      '```',
    ].join('\n');

    render(<Markdown>{markdown}</Markdown>);

    expect(screen.getByTestId('mermaid-block').textContent).toContain('sequenceDiagram');
  });

  it('keeps code readable while the syntax highlighter chunk loads', () => {
    render(<Markdown>{'```ts\nconst value: number = 1;\n```'}</Markdown>);

    expect(screen.getByText(/const value: number = 1/)).toBeTruthy();
  });
});

describe('Markdown source references', () => {
  it('turns plain source paths into explorer actions', () => {
    const onOpen = vi.fn();

    render(
      <Markdown onOpenSourceReference={onOpen}>
        {'Voir Acme.Sample.ihm/Controllers/CourrierController.cs:42 pour le controleur.'}
      </Markdown>
    );

    fireEvent.click(screen.getByRole('button', { name: /CourrierController\.cs:42/ }));

    expect(onOpen).toHaveBeenCalledWith({
      path: 'Acme.Sample.ihm/Controllers/CourrierController.cs',
      startLine: 42,
      endLine: 42,
    });
  });

  it('keeps source paths inert while repository file validation is pending', () => {
    const onOpen = vi.fn();

    render(
      <Markdown onOpenSourceReference={onOpen} sourceValidationState="pending">
        {'Voir Acme.Sample.ihm/Controllers/CourrierController.cs:42 pour le controleur.'}
      </Markdown>
    );

    expect(screen.queryByRole('button', { name: /CourrierController\.cs:42/ })).toBeNull();
    expect(screen.getByText(/CourrierController\.cs:42/).tagName.toLowerCase()).toBe('code');
  });

  it('does not linkify source paths inside code fences', () => {
    const linked = linkifySourceReferences(
      '```text\nAcme.Sample.ihm/Controllers/CourrierController.cs:42\n```'
    );

    expect(linked).not.toContain('code-explorer-source:');
  });
});
