import { useEffect, useRef, useState } from 'react';
import { Check, Copy, FileCode2, FileDown, Printer } from 'lucide-react';
import type { LlmConfigState } from '../../hooks/use-llm-config';
import { useChatStore } from '../../stores/chat-store';
import {
  conversationToMarkdown,
  exportMarkdown,
  exportPdf,
  exportPrintableHtml,
} from '../../utils/chat-export';
import { copyTextToClipboard } from '../../utils/clipboard';

interface Props {
  llm: LlmConfigState;
}

export function ChatExports({ llm }: Props) {
  const session = useChatStore((s) => s.getCurrentSession());
  const selectedRepo = useChatStore((s) => s.selectedRepo);
  const selectedRepoName = useChatStore((s) => s.selectedRepoName);
  const hasContent = !!session?.messages.some((m) => m.content.trim());
  const [copied, setCopied] = useState(false);
  const [isPdfExporting, setIsPdfExporting] = useState(false);
  const [exportError, setExportError] = useState<string | null>(null);
  const copiedResetTimer = useRef<number | null>(null);
  const errorResetTimer = useRef<number | null>(null);

  useEffect(() => {
    return () => {
      if (copiedResetTimer.current !== null) {
        window.clearTimeout(copiedResetTimer.current);
      }
      if (errorResetTimer.current !== null) {
        window.clearTimeout(errorResetTimer.current);
      }
    };
  }, []);

  const exportMetadata = {
    repo: selectedRepoName ?? selectedRepo,
    llm: llm.config,
  };

  const handleCopyMarkdown = async () => {
    if (!session || !hasContent) return;
    const ok = await copyTextToClipboard(conversationToMarkdown(session, exportMetadata));
    if (ok) {
      clearExportError();
      setCopied(true);
      if (copiedResetTimer.current !== null) {
        window.clearTimeout(copiedResetTimer.current);
      }
      copiedResetTimer.current = window.setTimeout(() => setCopied(false), 1600);
    } else {
      showExportError('Copie impossible');
    }
  };

  const handleMarkdown = () => {
    if (!session || !hasContent) return;
    clearExportError();
    exportMarkdown(session, exportMetadata);
  };

  const handleHtml = () => {
    if (!session || !hasContent) return;
    clearExportError();
    exportPrintableHtml(
      session,
      exportMetadata,
      document.getElementById('gitnexus-chat-export-source')
    );
  };

  const handlePdf = async () => {
    if (!session || !hasContent) return;
    try {
      clearExportError();
      setIsPdfExporting(true);
      await exportPdf(session, exportMetadata, document.getElementById('gitnexus-chat-export-source'));
    } catch (e) {
      showExportError(e instanceof Error ? e.message : String(e));
    } finally {
      setIsPdfExporting(false);
    }
  };

  const clearExportError = () => {
    setExportError(null);
    if (errorResetTimer.current !== null) {
      window.clearTimeout(errorResetTimer.current);
      errorResetTimer.current = null;
    }
  };

  const showExportError = (message: string) => {
    setExportError(message);
    if (errorResetTimer.current !== null) {
      window.clearTimeout(errorResetTimer.current);
    }
    errorResetTimer.current = window.setTimeout(() => {
      setExportError(null);
      errorResetTimer.current = null;
    }, 3200);
  };

  return (
    <div className="relative flex items-center gap-1" aria-label="Exports de la conversation">
      <button
        type="button"
        onClick={() => void handleCopyMarkdown()}
        disabled={!hasContent}
        className="control-button rounded-md border p-1.5 transition disabled:cursor-not-allowed disabled:opacity-40"
        aria-label={copied ? 'Conversation Markdown copiée' : 'Copier la conversation en Markdown'}
        title={copied ? 'Copié' : 'Copier en Markdown'}
      >
        {copied ? (
          <Check className="h-3.5 w-3.5 text-emerald-400" aria-hidden />
        ) : (
          <Copy className="h-3.5 w-3.5" aria-hidden />
        )}
      </button>
      <button
        type="button"
        onClick={handleMarkdown}
        disabled={!hasContent}
        className="control-button rounded-md border p-1.5 transition disabled:cursor-not-allowed disabled:opacity-40"
        aria-label="Exporter la conversation en Markdown"
        title="Exporter en Markdown"
      >
        <FileDown className="h-3.5 w-3.5" aria-hidden />
      </button>
      <button
        type="button"
        onClick={handleHtml}
        disabled={!hasContent}
        className="control-button rounded-md border p-1.5 transition disabled:cursor-not-allowed disabled:opacity-40"
        aria-label="Exporter la conversation en HTML imprimable"
        title="Exporter en HTML imprimable"
      >
        <FileCode2 className="h-3.5 w-3.5" aria-hidden />
      </button>
      <button
        type="button"
        onClick={() => void handlePdf()}
        disabled={!hasContent || isPdfExporting}
        className="control-button rounded-md border p-1.5 transition disabled:cursor-not-allowed disabled:opacity-40"
        aria-label={isPdfExporting ? 'Préparation du PDF' : 'Exporter la conversation en PDF'}
        title={isPdfExporting ? 'Préparation du PDF' : 'Exporter en PDF'}
      >
        <Printer className="h-3.5 w-3.5" aria-hidden />
      </button>
      <span
        role="status"
        aria-live="polite"
        className={
          exportError
            ? 'absolute right-0 top-[calc(100%+0.35rem)] z-20 max-w-64 rounded-md border border-red-900/70 bg-red-950/95 px-2 py-1 text-xs text-red-100 shadow-lg'
            : isPdfExporting
              ? 'absolute right-0 top-[calc(100%+0.35rem)] z-20 max-w-64 rounded-md border border-sky-900/40 bg-sky-950/90 px-2 py-1 text-xs text-sky-100 shadow-lg'
            : 'sr-only'
        }
      >
        {exportError ?? (isPdfExporting ? 'Préparation du PDF…' : '')}
      </span>
    </div>
  );
}
