import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { repairMermaidSource } from '../../utils/mermaid';
import { MermaidBlock } from './MermaidBlock';

const mermaidMock = vi.hoisted(() => ({
  initialize: vi.fn(),
  render: vi.fn(),
}));

vi.mock('mermaid', () => ({
  default: mermaidMock,
}));

describe('MermaidBlock', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mermaidMock.render.mockResolvedValue({
      svg: '<svg><script>alert(1)</script><g onclick="alert(2)"><text>OK</text></g></svg>',
    });
  });

  it('shows a visible loading state, then renders sanitized SVG', async () => {
    const { container } = render(<MermaidBlock text="flowchart TD\nA --> B" />);

    expect(screen.getByTestId('mermaid-loading')).toBeTruthy();

    await waitFor(() => {
      expect(container.querySelector('svg')).toBeTruthy();
    });

    expect(container.querySelector('script')).toBeNull();
    expect(container.querySelector('[onclick]')).toBeNull();
    expect(container.textContent).toContain('OK');
  });

  it('can reveal and copy the original diagram source', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
    });

    const source = 'sequenceDiagram\nMVC->>BAL: CreerCourrierMasse';
    const { container } = render(<MermaidBlock text={source} />);

    await waitFor(() => {
      expect(container.querySelector('svg')).toBeTruthy();
    });

    fireEvent.click(screen.getByRole('button', { name: /afficher la source mermaid/i }));
    expect(container.querySelector('pre')?.textContent).toBe(source);

    fireEvent.click(screen.getByRole('button', { name: /copier la source mermaid/i }));
    expect(writeText).toHaveBeenCalledWith(source);
  });

  it('can download the rendered diagram as an SVG file', async () => {
    const createObjectURL = vi.fn((blob: Blob) => {
      void blob;
      return 'blob:gitnexus-diagram';
    });
    const revokeObjectURL = vi.fn();
    const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});
    Object.defineProperty(URL, 'createObjectURL', {
      value: createObjectURL,
      configurable: true,
    });
    Object.defineProperty(URL, 'revokeObjectURL', {
      value: revokeObjectURL,
      configurable: true,
    });

    const { container } = render(<MermaidBlock text="flowchart TD\nA --> B" />);

    await waitFor(() => {
      expect(container.querySelector('svg')).toBeTruthy();
    });

    fireEvent.click(screen.getByRole('button', { name: /télécharger le diagramme mermaid/i }));

    expect(click).toHaveBeenCalled();
    expect(createObjectURL).toHaveBeenCalledTimes(1);
    expect(revokeObjectURL).toHaveBeenCalledWith('blob:gitnexus-diagram');

    const blob = createObjectURL.mock.calls[0][0];
    const svgText = await blob.text();
    expect(svgText).toContain('<?xml version="1.0" encoding="UTF-8"?>');
    expect(svgText).toContain('xmlns="http://www.w3.org/2000/svg"');
    expect(svgText).not.toContain('<script>');

    click.mockRestore();
  });

  it('can open rendered diagrams in a larger modal', async () => {
    render(<MermaidBlock text="flowchart TD\nA --> B" />);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /agrandir le diagramme mermaid/i })).toBeTruthy();
    });

    fireEvent.click(screen.getByRole('button', { name: /agrandir le diagramme mermaid/i }));

    expect(screen.getByRole('dialog', { name: /diagramme mermaid agrandi/i })).toBeTruthy();

    fireEvent.keyDown(document, { key: 'Escape' });

    await waitFor(() => {
      expect(screen.queryByRole('dialog', { name: /diagramme mermaid agrandi/i })).toBeNull();
    });
  });

  it('keeps the source visible when Mermaid cannot render the graph', async () => {
    mermaidMock.render.mockRejectedValue(new Error('Parse error'));

    const source = 'flowchart TD\nA -- B';
    const { container } = render(<MermaidBlock text={source} />);

    await waitFor(() => {
      expect(screen.getByText('Rendu Mermaid impossible')).toBeTruthy();
    });

    expect(screen.getByText('Parse error')).toBeTruthy();
    expect(container.querySelector('pre')?.textContent).toBe(source);
  });
});

describe('repairMermaidSource', () => {
  it('quotes flowchart labels that contain punctuation known to break Mermaid parsing', () => {
    const repaired = repairMermaidSource(
      [
        'flowchart TD',
        'G --> H[CreerCourrier(...,)]',
        'H --> I[Validation sans sauvegarde]',
      ].join('\n')
    );

    expect(repaired).toContain('G --> H["CreerCourrier(...,)"]');
    expect(repaired).toContain('H --> I[Validation sans sauvegarde]');
  });

  it('keeps existing flowchart shape syntax untouched', () => {
    const repaired = repairMermaidSource(
      ['flowchart TD', 'A[(Base de donnees)] --> B{{Decision}}', 'C[[Sous-processus]] --> D'].join(
        '\n'
      )
    );

    expect(repaired).toContain('A[(Base de donnees)] --> B{{Decision}}');
    expect(repaired).toContain('C[[Sous-processus]] --> D');
  });
});
