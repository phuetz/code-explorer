import { useCallback, Suspense, lazy } from "react";
import { Copy, Download, Zap, Sparkles, Microscope, Pin } from "lucide-react";
import { toast } from "sonner";
import { useI18n } from "../../hooks/use-i18n";
import type { Message } from "../../stores/chat-session-store";
import { useChatSessionStore } from "../../stores/chat-session-store";
import type { QueryComplexity } from "../../lib/tauri-commands";
import { ArtifactPanel } from "./ArtifactPanel";
import { CodeReviewPanel } from "./CodeReviewPanel";
import { SimplifyPanel } from "./SimplifyPanel";
import { copyTextToClipboard } from "../../lib/clipboard";

const ResearchPlanViewer = lazy(() =>
  import("./ResearchPlanViewer").then((m) => ({ default: m.ResearchPlanViewer })),
);
const SourceReferences = lazy(() =>
  import("./SourceReferences").then((m) => ({ default: m.SourceReferences })),
);
const ChatMarkdown = lazy(() =>
  import("./ChatMarkdown").then((m) => ({ default: m.ChatMarkdown })),
);

function MarkdownFallback({ content }: { content: string }) {
  return <div className="whitespace-pre-wrap">{content}</div>;
}

function formatMessageTime(timestamp: number | undefined): string {
  if (!timestamp) return "";
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) return "";
  return new Intl.DateTimeFormat(undefined, {
    day: "2-digit",
    month: "2-digit",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

export function ChatMessage({
  message,
  sessionId,
  onNavigateToNode,
  onFilePreview,
}: {
  message: Message;
  /** Parent session id — required for fork/pin/retry buttons. */
  sessionId?: string;
  onNavigateToNode?: (nodeId: string) => void;
  onFilePreview?: (file: { path: string; startLine?: number; endLine?: number }) => void;
}) {
  const { t } = useI18n();
  const pinMessage = useChatSessionStore((s) => s.pinMessage);
  const timestamp = formatMessageTime(message.timestamp);
  const handleCopyMessage = useCallback(async () => {
    const ok = await copyTextToClipboard(message.content);
    if (ok) {
      toast.success(t("chat.copiedToClipboard"));
    } else {
      toast.error(t("chat.copyFailed"));
    }
  }, [message.content, t]);

  const handleExportMessage = useCallback(() => {
    try {
      const date = new Date().toISOString().split("T")[0];
      const filename = `code-explorer-response-${date}.md`;
      const blob = new Blob([message.content], { type: "text/markdown" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = filename;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      setTimeout(() => URL.revokeObjectURL(url), 1000);
      toast.success(t("chat.responseExported"));
    } catch (e) {
      toast.error(t("chat.exportFailed").replace("{0}", String(e)));
    }
  }, [message.content, t]);

  if (message.role === "user") {
    return (
      <div className="group relative fade-in">
        {/* Role label */}
        <div className="flex items-center gap-1.5 mb-1">
          <span
            className="w-2 h-2 rounded-full flex-shrink-0"
            style={{ background: "var(--accent)" }}
          />
          <span className="text-[11px] font-medium" style={{ color: "var(--text-3)" }}>
            {t("chat.you")}
          </span>
          {timestamp && (
            <time
              dateTime={new Date(message.timestamp).toISOString()}
              className="text-[10px]"
              style={{ color: "var(--text-3)" }}
              title={timestamp}
            >
              {timestamp}
            </time>
          )}
          {message.pinned && (
            <Pin
              size={10}
              style={{ color: "var(--accent)" }}
              aria-label="Pinned"
            />
          )}
        </div>
        {/* Message content */}
        <div
          className="px-4 py-3 rounded-lg text-[13px] leading-relaxed"
          style={{ background: "var(--bg-2)", color: "var(--text-1)" }}
        >
          {message.content}
        </div>
        {/* Hover actions */}
        <div
          className="absolute top-0 right-0 flex items-center gap-0.5 opacity-0 group-hover:opacity-100 focus-within:opacity-100 transition-opacity"
          style={{ marginTop: -2 }}
        >
          {sessionId && (
            <button
              onClick={() => pinMessage(sessionId, message.id)}
              className="p-1 rounded transition-colors"
              style={{
                background: message.pinned ? "var(--accent-subtle)" : "var(--bg-3)",
                color: message.pinned ? "var(--accent)" : "var(--text-3)",
              }}
              aria-label={message.pinned ? "Unpin" : "Pin"}
              title={message.pinned ? "Unpin" : "Pin"}
            >
              <Pin size={12} />
            </button>
          )}
          <button
            onClick={handleCopyMessage}
            className="p-1 rounded transition-colors"
            style={{ background: "var(--bg-3)", color: "var(--text-3)" }}
            aria-label="Copy message"
          >
            <Copy size={12} />
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="group relative fade-in">
      {/* Role label */}
      <div className="flex items-center gap-1.5 mb-1">
        <span
          className="w-2 h-2 rounded-full flex-shrink-0"
          style={{ background: "var(--purple)" }}
        />
        <span className="text-[11px] font-medium" style={{ color: "var(--text-3)" }}>
          Code Explorer
        </span>
        {timestamp && (
          <time
            dateTime={new Date(message.timestamp).toISOString()}
            className="text-[10px]"
            style={{ color: "var(--text-3)" }}
            title={timestamp}
          >
            {timestamp}
          </time>
        )}
        {/* Complexity badge inline */}
        {message.complexity && <ComplexityIndicator complexity={message.complexity} />}
        {message.pinned && (
          <Pin
            size={10}
            style={{ color: "var(--accent)" }}
            aria-label="Pinned"
          />
        )}
      </div>

      {/* Hover actions */}
      <div
        className="absolute top-0 right-0 flex items-center gap-0.5 opacity-0 group-hover:opacity-100 focus-within:opacity-100 transition-opacity"
        style={{ marginTop: -2 }}
      >
        {sessionId && (
          <button
            onClick={() => pinMessage(sessionId, message.id)}
            className="p-1 rounded transition-colors"
            style={{
              background: message.pinned ? "var(--accent-subtle)" : "var(--bg-3)",
              color: message.pinned ? "var(--accent)" : "var(--text-3)",
            }}
            aria-label={message.pinned ? "Unpin" : "Pin"}
            title={message.pinned ? "Unpin this message" : "Pin this message"}
          >
            <Pin size={12} />
          </button>
        )}
        <button
          onClick={handleExportMessage}
          className="p-1 rounded transition-colors"
          style={{ background: "var(--bg-3)", color: "var(--text-3)" }}
          title={t("chat.exportResponseMarkdown")}
          aria-label="Export response"
        >
          <Download size={12} />
        </button>
        <button
          onClick={handleCopyMessage}
          className="p-1 rounded transition-colors"
          style={{ background: "var(--bg-3)", color: "var(--text-3)" }}
          aria-label="Copy message"
        >
          <Copy size={12} />
        </button>
      </div>

      {/* Research plan (if present) */}
      {message.plan && (
        <div className="mb-3">
          <Suspense fallback={null}>
            <ResearchPlanViewer plan={message.plan} />
          </Suspense>
        </div>
      )}

      {/* Artifact panels (mutually exclusive with plain content) */}
      {message.artifact ? (
        <ArtifactPanel artifact={message.artifact} />
      ) : message.reviewArtifact ? (
        <CodeReviewPanel artifact={message.reviewArtifact} />
      ) : message.simplifyArtifact ? (
        <SimplifyPanel artifact={message.simplifyArtifact} />
      ) : (
        /* Response content */
        <div
          className="prose-sm text-[13px] leading-relaxed"
          style={{ color: "var(--text-1)" }}
        >
          <Suspense fallback={<MarkdownFallback content={message.content} />}>
            <ChatMarkdown
              content={message.content}
              onNavigateToNode={onNavigateToNode}
              onFilePreview={onFilePreview}
              message={message}
              sessionId={sessionId}
            />
          </Suspense>
        </div>
      )}

      {/* Enhanced source references */}
      {message.sources && message.sources.length > 0 && (
        <Suspense fallback={null}>
          <SourceReferences
            sources={message.sources}
            onNavigateToNode={onNavigateToNode}
          />
        </Suspense>
      )}

      {/* Model indicator */}
      {message.model && (
        <div className="mt-2 text-[11px]" style={{ color: "var(--text-3)" }}>
          Answered by {message.model}
        </div>
      )}
    </div>
  );
}

function ComplexityIndicator({ complexity }: { complexity: QueryComplexity }) {
  const { t } = useI18n();
  const configs: Record<string, { label: string; color: string; icon: typeof Zap }> = {
    simple: { label: t("chat.quickAnswer"), color: "var(--green)", icon: Zap },
    medium: { label: t("chat.multiSource"), color: "var(--orange)", icon: Sparkles },
    complex: { label: t("chat.deepResearch"), color: "var(--purple)", icon: Microscope },
  };
  const config = configs[complexity] ?? configs.simple;

  const Icon = config.icon;

  return (
    <span
      className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium"
      style={{
        background: `color-mix(in srgb, ${config.color} 10%, transparent)`,
        color: config.color,
      }}
    >
      <Icon size={9} />
      {config.label}
    </span>
  );
}
