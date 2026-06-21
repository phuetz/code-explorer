import { File, Code2, ArrowLeft } from "lucide-react";
import { useFileContent } from "../../hooks/use-tauri-query";
import { useShikiTokens } from "../../hooks/use-shiki";
import { useI18n } from "../../hooks/use-i18n";

/**
 * FilePreview shows the contents of a selected file in a read-only code view
 * with syntax highlighting powered by Shiki.
 */
export function FilePreview({
  nodeId,
  fileName,
  onClose,
}: {
  nodeId: string;
  fileName: string | null;
  onClose: () => void;
}) {
  const { t } = useI18n();
  // Extract file path from "File:<path>" format
  const filePath = nodeId.startsWith("File:") ? nodeId.slice(5) : null;
  const { data: fileContent, isLoading, error } = useFileContent(filePath);

  // Tokenize for syntax highlighting (runs async, gracefully degrades)
  const { tokens, ready: shikiReady } = useShikiTokens(
    fileContent?.content,
    fileContent?.language
  );

  if (!filePath) {
    return (
      <div
        className="h-full flex items-center justify-center text-center"
        style={{ padding: 16, color: "var(--text-3)" }}
      >
        <p>{t("files.selectFileToPreview")}</p>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col" style={{ backgroundColor: "var(--bg-0)" }}>
      {/* Header */}
      <div
        className="flex items-center shrink-0"
        style={{
          paddingLeft: 12,
          paddingRight: 12,
          paddingTop: 10,
          paddingBottom: 10,
          gap: 8,
          backgroundColor: "var(--bg-1)",
          borderBottom: "1px solid var(--surface-border)",
        }}
      >
        <button
          onClick={onClose}
          className="rounded hover-surface"
          style={{ padding: 4, color: "var(--text-3)" }}
          title={t("files.closePreview")}
        >
          <ArrowLeft size={14} />
        </button>
        <File size={14} style={{ color: "var(--accent)" }} />
        <span
          className="text-xs font-medium truncate"
          style={{ color: "var(--text-1)" }}
        >
          {fileName || filePath}
        </span>
        {fileContent && (
          <span
            className="text-[10px] rounded"
            style={{
              marginLeft: "auto",
              paddingLeft: 8,
              paddingRight: 8,
              paddingTop: 2,
              paddingBottom: 2,
              backgroundColor: "var(--bg-2)",
              color: "var(--text-3)",
            }}
          >
            {fileContent.totalLines} {t("files.lines")}
            {fileContent.language && ` · ${fileContent.language}`}
          </span>
        )}
      </div>

      {/* Content */}
      <div className="flex-1 overflow-auto relative">
        {isLoading && (
          <div
            className="flex items-center justify-center"
            style={{ paddingTop: 48, paddingBottom: 48, color: "var(--text-3)" }}
          >
            <Code2 size={16} className="animate-pulse" style={{ marginRight: 8 }} />
            {t("files.loadingFile")}
          </div>
        )}

        {error && (
          <div
            className="flex items-center justify-center text-center"
            style={{ paddingTop: 48, paddingBottom: 48, paddingLeft: 16, paddingRight: 16, color: "var(--rose)" }}
          >
            {t("files.unableToRead")}
          </div>
        )}

        {fileContent && (
          <pre
            className="text-[12px] leading-[1.6]"
            style={{
              margin: 0,
              padding: 0,
              fontFamily: "var(--font-mono)",
              color: "var(--text-1)",
            }}
          >
            <code>
              {tokens
                ? /* Syntax-highlighted rendering */
                  tokens.map((lineTokens, i) => (
                    <div
                      key={i}
                      className="flex hover-line"
                      style={{ minHeight: "1.6em" }}
                    >
                      <span
                        className="select-none text-right shrink-0"
                        style={{
                          paddingLeft: 12,
                          paddingRight: 16,
                          color: "var(--text-3)",
                          width: "4em",
                          userSelect: "none",
                          opacity: 0.5,
                        }}
                      >
                        {i + 1}
                      </span>
                      <span className="flex-1 whitespace-pre" style={{ paddingRight: 16 }}>
                        {lineTokens.map((token, j) => (
                          <span key={j} style={{ color: token.color }}>{token.content}</span>
                        ))}
                      </span>
                    </div>
                  ))
                : /* Plain-text fallback (while shiki loads or on error) */
                  fileContent.content.split("\n").map((line, i) => (
                    <div
                      key={i}
                      className="flex hover-line"
                      style={{ minHeight: "1.6em" }}
                    >
                      <span
                        className="select-none text-right shrink-0"
                        style={{
                          paddingLeft: 12,
                          paddingRight: 16,
                          color: "var(--text-3)",
                          width: "4em",
                          userSelect: "none",
                          opacity: 0.5,
                        }}
                      >
                        {i + 1}
                      </span>
                      <span className="flex-1 whitespace-pre" style={{ paddingRight: 16 }}>{line}</span>
                    </div>
                  ))}
            </code>
          </pre>
        )}

        {/* Subtle loading indicator while shiki tokenizes */}
        {fileContent && !shikiReady && (
          <div
            style={{
              position: "absolute",
              top: 48,
              right: 12,
              fontSize: 10,
              color: "var(--text-4)",
              opacity: 0.6,
            }}
          >
            {t("files.highlighting")}
          </div>
        )}
      </div>
    </div>
  );
}
