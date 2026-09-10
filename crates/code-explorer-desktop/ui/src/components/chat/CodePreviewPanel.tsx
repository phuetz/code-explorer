import { useState, useEffect, useMemo } from "react";
import { X, Copy, Check, ExternalLink } from "lucide-react";
import { toast } from "sonner";
import { useAppStore } from "../../stores/app-store";
import { commands } from "../../lib/tauri-commands";
import { copyTextToClipboard } from "../../lib/clipboard";
import {
  ensureUiLanguageLoaded,
  getUiHighlighter,
  resolveUiLanguage,
} from "../../lib/shiki-runtime";

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

export function CodePreviewPanel({
  filePath,
  onClose,
  startLine,
  endLine,
}: {
  filePath: string;
  onClose: () => void;
  startLine?: number;
  endLine?: number;
}) {
  const [code, setCode] = useState<string>("");
  const [html, setHtml] = useState<string>("");
  const [isLoading, setIsLoading] = useState(true);
  const [copied, setCheckCopied] = useState(false);

  const extension = useMemo(() => {
    const parts = filePath.split(".");
    return parts.length > 1 ? parts[parts.length - 1] : "txt";
  }, [filePath]);

  useEffect(() => {
    let cancelled = false;
    setIsLoading(true);

    void (async () => {
      try {
        const result = await commands.readFileContent(filePath, startLine, endLine);
        if (cancelled) return;
        
        setCode(result.content);

        setHtml(`<pre><code>${escapeHtml(result.content)}</code></pre>`);

        try {
          const highlighter = await getUiHighlighter();
          const lang = resolveUiLanguage(extension);
          const langOk = lang ? await ensureUiLanguageLoaded(highlighter, lang) : false;

          const generatedHtml = highlighter.codeToHtml(result.content, {
            lang: langOk && lang ? lang : ("text" as never),
            theme: "tokyo-night" as never,
          });
          
          if (!cancelled) {
            setHtml(generatedHtml);
          }
        } catch (shikiErr) {
          console.warn("Shiki highlighter failed, using fallback:", shikiErr);
        } finally {
          if (!cancelled) setIsLoading(false);
        }
      } catch (e) {
        console.error("Failed to load code preview:", e);
        if (!cancelled) setIsLoading(false);
      }
    })();

    return () => { cancelled = true; };
  }, [filePath, extension, startLine, endLine]);

  const handleCopy = async () => {
    const ok = await copyTextToClipboard(code);
    if (ok) {
      setCheckCopied(true);
      setTimeout(() => setCheckCopied(false), 2000);
      toast.success("Copied to clipboard");
    } else {
      toast.error("Failed to copy code");
    }
  };

  return (
    <div className="flex flex-col h-full bg-[#1a1b26] border-l border-surface-border text-[#a9b1d6] font-mono text-[13px] shadow-2xl relative">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-2 border-b border-white/5 bg-[#16161e]">
        <div className="flex items-center gap-2 overflow-hidden">
          <span className="text-[11px] opacity-50 truncate">{filePath}</span>
          {startLine && (
            <span className="text-[10px] bg-white/10 px-1.5 py-0.5 rounded text-white/70">
              L{startLine}:{endLine}
            </span>
          )}
        </div>
        <div className="flex items-center gap-1">
          <button
            onClick={handleCopy}
            className="p-1.5 hover:bg-white/10 rounded transition-colors"
            title="Copy code"
          >
            {copied ? <Check size={14} className="text-green" /> : <Copy size={14} />}
          </button>
          <button
            onClick={() => {
              useAppStore.getState().setSelectedNodeId("File:" + filePath);
              useAppStore.getState().setMode("explorer");
            }}
            className="p-1.5 hover:bg-white/10 rounded transition-colors"
            title="Open in Explorer"
          >
            <ExternalLink size={14} />
          </button>
          <button
            onClick={onClose}
            className="p-1.5 hover:bg-white/10 rounded transition-colors ml-1"
          >
            <X size={14} />
          </button>
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-auto p-4 custom-scrollbar">
        {isLoading ? (
          <div className="flex items-center justify-center h-full opacity-30 animate-pulse">
            Loading preview...
          </div>
        ) : (
          <div 
            dangerouslySetInnerHTML={{ __html: html }} 
            className="shiki-container"
          />
        )}
      </div>

      <style dangerouslySetInnerHTML={{ __html: `
        .shiki-container pre { background: transparent !important; margin: 0; }
        .shiki-container code { font-family: 'JetBrains Mono', monospace !important; }
        .custom-scrollbar::-webkit-scrollbar { width: 8px; height: 8px; }
        .custom-scrollbar::-webkit-scrollbar-track { background: transparent; }
        .custom-scrollbar::-webkit-scrollbar-thumb { background: rgba(255,255,255,0.1); border-radius: 4px; }
        .custom-scrollbar::-webkit-scrollbar-thumb:hover { background: rgba(255,255,255,0.2); }
      `}} />
    </div>
  );
}
