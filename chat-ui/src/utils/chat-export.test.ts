import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  conversationToMarkdown,
  exportFilename,
  exportPdf,
  exportPrintableHtml,
} from './chat-export';
import { mcpClient } from '../api/mcp-client';
import type { Session } from '../types/chat';

const session: Session = {
  id: 's1',
  title: 'Trace courrier',
  createdAt: 1774507049000,
  updatedAt: 1774507049000,
  messages: [
    {
      id: 'm1',
      role: 'user',
      content: 'Trace le flux',
      createdAt: 1774507049000,
    },
    {
      id: 'm2',
      role: 'assistant',
      content: '```mermaid\nflowchart TD\nA --> B\n```',
      createdAt: 1774507059000,
      toolCalls: [
        { id: 't1', name: 'search_code', args: { query: 'courrier' }, status: 'done' },
        { id: 't2', name: 'trace_files', args: { symbol: 'Courrier' }, status: 'error' },
      ],
    },
  ],
};

describe('conversationToMarkdown', () => {
  it('includes repo, llm metadata, timestamps, and message contents', () => {
    const markdown = conversationToMarkdown(session, {
      repo: 'sample-app',
      llm: {
        configured: true,
        provider: 'chatgpt',
        model: 'gpt-5.5',
        reasoningEffort: 'high',
        maxTokens: 8192,
      },
    });

    expect(markdown).toContain('# Trace courrier');
    expect(markdown).toContain('- Projet: sample-app');
    expect(markdown).toContain('- LLM: chatgpt / gpt-5.5, raisonnement high, max 8192 tokens');
    expect(markdown).toContain('- Conversation créée: ');
    expect(markdown).toContain('- Dernière activité: ');
    expect(markdown).toContain('- Messages exportés: 2');
    expect(markdown).toContain('## Vous - ');
    expect(markdown).toContain('_Outils: search_code (done), trace_files (error)_');
    expect(markdown).toContain('Trace le flux');
    expect(markdown).toContain('```mermaid');
  });

  it('does not count empty streaming placeholders in exported metadata', () => {
    const markdown = conversationToMarkdown(
      {
        ...session,
        messages: [
          ...session.messages,
          {
            id: 'm3',
            role: 'assistant',
            content: '   ',
            createdAt: 1774507069000,
          },
        ],
      },
      { repo: 'sample-app', llm: null }
    );

    expect(markdown).toContain('- Messages exportés: 2');
    expect(markdown.match(/^## /gm)).toHaveLength(2);
  });

  it('adds a compact source appendix when assistant messages cite files', () => {
    const markdown = conversationToMarkdown(
      {
        ...session,
        messages: [
          ...session.messages,
          {
            id: 'm3',
            role: 'assistant',
            content: 'Sources: Controllers/CourrierController.cs:42 et BAL/CourriersService.cs:5-8',
            createdAt: 1774507069000,
          },
        ],
      },
      { repo: 'sample-app', llm: null }
    );

    expect(markdown).toContain('### Fichiers sources cités');
    expect(markdown).toContain('- Controllers/CourrierController.cs:42');
    expect(markdown).toContain('- BAL/CourriersService.cs:5-8');
  });
});

describe('exportPdf', () => {
  beforeEach(() => {
    vi.spyOn(mcpClient, 'exportPdf').mockRejectedValue(new Error('native unavailable'));
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('downloads a native PDF when the backend renderer is available', async () => {
    const pdfBlob = new Blob(['%PDF-1.7 native'], { type: 'application/pdf' });
    const exportSpy = vi.spyOn(mcpClient, 'exportPdf').mockResolvedValue(pdfBlob);
    const createObjectUrlDescriptor = Object.getOwnPropertyDescriptor(URL, 'createObjectURL');
    const revokeObjectUrlDescriptor = Object.getOwnPropertyDescriptor(URL, 'revokeObjectURL');
    const createObjectURL = vi.fn<(object: Blob | MediaSource) => string>(
      () => 'blob:code-explorer-native-pdf'
    );
    const revokeObjectURL = vi.fn();
    Object.defineProperty(URL, 'createObjectURL', {
      configurable: true,
      value: createObjectURL,
    });
    Object.defineProperty(URL, 'revokeObjectURL', {
      configurable: true,
      value: revokeObjectURL,
    });
    const click = vi
      .spyOn(HTMLAnchorElement.prototype, 'click')
      .mockImplementation(() => undefined);
    const openSpy = vi.spyOn(window, 'open');

    try {
      const mode = await exportPdf(session, { repo: 'sample-app', llm: null }, null);

      expect(mode).toBe('native');
      expect(exportSpy).toHaveBeenCalledTimes(1);
      expect(exportSpy.mock.calls[0][0].filename).toMatch(
        /^code-explorer-alise-v2-trace-courrier-\d{8}-\d{6}\.pdf$/
      );
      expect(exportSpy.mock.calls[0][0].html).toContain('<!doctype html>');
      expect(createObjectURL).toHaveBeenCalledWith(pdfBlob);
      expect(click).toHaveBeenCalledTimes(1);
      expect(revokeObjectURL).toHaveBeenCalledWith('blob:code-explorer-native-pdf');
      expect(openSpy).not.toHaveBeenCalled();
    } finally {
      restoreUrlMethod('createObjectURL', createObjectUrlDescriptor);
      restoreUrlMethod('revokeObjectURL', revokeObjectUrlDescriptor);
    }
  });

  it('keeps tool summaries in the fallback print transcript', async () => {
    const written: string[] = [];
    const popup = {
      document: {
        open: vi.fn(),
        write: vi.fn((html: string) => written.push(html)),
        close: vi.fn(),
      },
      focus: vi.fn(),
      print: vi.fn(),
      setTimeout: vi.fn((callback: () => void) => {
        callback();
        return 0;
      }),
    };
    vi.spyOn(window, 'open').mockReturnValue(popup as unknown as Window);

    await exportPdf(session, { repo: 'sample-app', llm: null }, null);

    const html = written.join('');
    expect(html).toContain('Code Explorer Chat');
    expect(html).toContain('Outils: search_code (done), trace_files (error)');
    expect(html).toContain('print-diagram-mermaid');
    expect(html).toContain('Diagramme Mermaid (source)');
    expect(html).toContain('flowchart TD');
    expect(popup.print).toHaveBeenCalled();
  });

  it('sanitizes invisible PDF-hostile characters before printing', async () => {
    const written: string[] = [];
    const popup = {
      document: {
        open: vi.fn(),
        write: vi.fn((html: string) => written.push(html)),
        close: vi.fn(),
      },
      focus: vi.fn(),
      print: vi.fn(),
      setTimeout: vi.fn((callback: () => void) => {
        callback();
        return 0;
      }),
    };
    vi.spyOn(window, 'open').mockReturnValue(popup as unknown as Window);

    await exportPdf(
      {
        ...session,
        messages: [
          ...session.messages,
          {
            id: 'm3',
            role: 'assistant',
            content: 'Texte avec zero\u200B width et variation\uFE0F selector.',
            createdAt: 1774507069000,
          },
        ],
      },
      { repo: 'sample-app', llm: null },
      null
    );

    const html = written.join('');
    expect(html).not.toContain('\u200B');
    expect(html).not.toContain('\uFE0F');
    expect(html).toContain('Texte avec zero width et variation selector.');
  });

  it('prepares rendered Mermaid fallbacks and source references for print', async () => {
    const written: string[] = [];
    const popup = {
      document: {
        open: vi.fn(),
        write: vi.fn((html: string) => written.push(html)),
        close: vi.fn(),
      },
      focus: vi.fn(),
      print: vi.fn(),
      setTimeout: vi.fn((callback: () => void) => {
        callback();
        return 0;
      }),
    };
    const transcript = document.createElement('div');
    transcript.innerHTML = `
      <section>
        <p><button>Acme.Sample.BAL/Courrier/CourriersService.cs:42</button></p>
        <button aria-label="Copier"><svg></svg></button>
        <div data-testid="mermaid-block">
          <div data-testid="mermaid-loading">Rendu...</div>
          <pre data-print-mermaid-source style="display:none"><code>flowchart TD
A --> B</code></pre>
        </div>
      </section>
    `;
    vi.spyOn(window, 'open').mockReturnValue(popup as unknown as Window);

    await exportPdf(session, { repo: 'sample-app', llm: null }, transcript);

    const html = written.join('');
    expect(html).toContain('class="print-source-ref"');
    expect(html).toContain('Acme.Sample.BAL/Courrier/CourriersService.cs:42');
    expect(html).toContain('data-print-visible="true"');
    expect(html).not.toContain('aria-label="Copier"');
  });

  it('adds a compact table of contents for assistant headings', async () => {
    const written: string[] = [];
    const popup = {
      document: {
        open: vi.fn(),
        write: vi.fn((html: string) => written.push(html)),
        close: vi.fn(),
      },
      focus: vi.fn(),
      print: vi.fn(),
      setTimeout: vi.fn((callback: () => void) => {
        callback();
        return 0;
      }),
    };
    vi.spyOn(window, 'open').mockReturnValue(popup as unknown as Window);

    await exportPdf(
      {
        ...session,
        messages: [
          ...session.messages,
          {
            id: 'm3',
            role: 'assistant',
            content: '## Vue d’ensemble\n\n### Génération `PDF`\n\nTexte',
            createdAt: 1774507069000,
          },
        ],
      },
      { repo: 'sample-app', llm: null },
      null
    );

    const html = written.join('');
    expect(html).toContain('class="print-toc"');
    expect(html).toContain('Vue d’ensemble');
    expect(html).toContain('Génération PDF');
  });

  it('adds cited source files as a printable appendix', async () => {
    const written: string[] = [];
    const popup = {
      document: {
        open: vi.fn(),
        write: vi.fn((html: string) => written.push(html)),
        close: vi.fn(),
      },
      focus: vi.fn(),
      print: vi.fn(),
      setTimeout: vi.fn((callback: () => void) => {
        callback();
        return 0;
      }),
    };
    vi.spyOn(window, 'open').mockReturnValue(popup as unknown as Window);

    await exportPdf(
      {
        ...session,
        messages: [
          ...session.messages,
          {
            id: 'm3',
            role: 'assistant',
            content: 'Voir Controllers/CourrierController.cs:42.',
            createdAt: 1774507069000,
          },
        ],
      },
      { repo: 'sample-app', llm: null },
      null
    );

    const html = written.join('');
    expect(html).toContain('class="print-related-sources"');
    expect(html).toContain('Controllers/CourrierController.cs:42');
  });

  it('chunks long fallback code blocks for printable layout', async () => {
    const written: string[] = [];
    const popup = {
      document: {
        open: vi.fn(),
        write: vi.fn((html: string) => written.push(html)),
        close: vi.fn(),
      },
      focus: vi.fn(),
      print: vi.fn(),
      setTimeout: vi.fn((callback: () => void) => {
        callback();
        return 0;
      }),
    };
    vi.spyOn(window, 'open').mockReturnValue(popup as unknown as Window);

    await exportPdf(
      {
        ...session,
        messages: [
          {
            id: 'm-long',
            role: 'assistant',
            content: `\`\`\`csharp\n${Array.from({ length: 50 }, (_, index) => `public class Line${index + 1} {}`).join('\n')}\n\`\`\``,
            createdAt: 1774507069000,
          },
        ],
      },
      { repo: 'sample-app', llm: null },
      null
    );

    const html = written.join('');
    expect(html).toContain('Code csharp · lignes 1-42');
    expect(html).toContain('Code csharp · lignes 43-50');
    expect(html).toContain('class="print-code-line-number">43</span>');
    expect(html).toContain('<span class="print-code-keyword">public</span>');
  });
});

describe('exportPrintableHtml', () => {
  const createObjectUrlDescriptor = Object.getOwnPropertyDescriptor(URL, 'createObjectURL');
  const revokeObjectUrlDescriptor = Object.getOwnPropertyDescriptor(URL, 'revokeObjectURL');

  afterEach(() => {
    vi.restoreAllMocks();
    restoreUrlMethod('createObjectURL', createObjectUrlDescriptor);
    restoreUrlMethod('revokeObjectURL', revokeObjectUrlDescriptor);
  });

  it('downloads the same standalone printable HTML used by PDF export', async () => {
    const createObjectURL = vi.fn<(object: Blob | MediaSource) => string>(
      () => 'blob:code-explorer-printable-html'
    );
    const revokeObjectURL = vi.fn();
    Object.defineProperty(URL, 'createObjectURL', {
      configurable: true,
      value: createObjectURL,
    });
    Object.defineProperty(URL, 'revokeObjectURL', {
      configurable: true,
      value: revokeObjectURL,
    });
    const click = vi
      .spyOn(HTMLAnchorElement.prototype, 'click')
      .mockImplementation(() => undefined);

    exportPrintableHtml(session, { repo: 'sample-app', llm: null }, null);

    expect(createObjectURL).toHaveBeenCalledTimes(1);
    const blob = createObjectURL.mock.calls[0][0] as Blob;
    const html = await blob.text();
    expect(blob.type).toBe('text/html;charset=utf-8');
    expect(html).toContain('<!doctype html>');
    expect(html).toContain('Code Explorer Chat');
    expect(html).toContain('class="print-cover"');
    expect(html).toContain('Outils: search_code (done), trace_files (error)');
    expect(html).toContain('Diagramme Mermaid (source)');
    expect(click).toHaveBeenCalledTimes(1);
    expect(revokeObjectURL).toHaveBeenCalledWith('blob:code-explorer-printable-html');
  });
});

describe('exportFilename', () => {
  it('normalizes repo and session names', () => {
    const filename = exportFilename(session, 'Sample v2', 'md');
    expect(filename).toMatch(/^code-explorer-alise-v2-trace-courrier-\d{8}-\d{6}\.md$/);
  });

  it('supports standalone HTML exports', () => {
    const filename = exportFilename(session, 'Sample v2', 'html');
    expect(filename).toMatch(/^code-explorer-alise-v2-trace-courrier-\d{8}-\d{6}\.html$/);
  });
});

function restoreUrlMethod(
  name: 'createObjectURL' | 'revokeObjectURL',
  descriptor: PropertyDescriptor | undefined
) {
  if (descriptor) {
    Object.defineProperty(URL, name, descriptor);
  } else {
    Reflect.deleteProperty(URL, name);
  }
}
