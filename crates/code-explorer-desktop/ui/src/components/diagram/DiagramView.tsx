import { useState, useRef, useEffect } from "react";
import { useQuery } from "@tanstack/react-query";
import { toast } from "sonner";
import { Workflow, Copy } from "lucide-react";
import { commands, type SearchResult } from "../../lib/tauri-commands";
import { copyTextToClipboard } from "../../lib/clipboard";
import { useI18n } from "../../hooks/use-i18n";
import { useAppStore } from "../../stores/app-store";
import { SymbolAutocomplete } from "../shared/SymbolAutocomplete";

export function DiagramView() {
  const { t } = useI18n();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const [target, setTarget] = useState("");
  const [searchTarget, setSearchTarget] = useState("");
  const [renderError, setRenderError] = useState(false);
  const mermaidRef = useRef<HTMLDivElement>(null);
  const mermaidInitialized = useRef(false);

  // Scope by `activeRepo` so switching repos doesn't resurrect a diagram from
  // the previous repo when the same `searchTarget` name happens to be used.
  const { data: diagram, isLoading, error: queryError } = useQuery({
    queryKey: ["diagram", activeRepo, searchTarget],
    queryFn: () => commands.getDiagram(searchTarget),
    enabled: searchTarget.length > 0,
    staleTime: 60_000,
    retry: 0,
  });

  // Render Mermaid diagram as SVG
  useEffect(() => {
    if (!diagram?.mermaid || !mermaidRef.current) return;

    let cancelled = false;
    setRenderError(false);

    (async () => {
      try {
        const mermaid = await import("mermaid");
        if (!mermaidInitialized.current) {
          mermaid.default.initialize({
            startOnLoad: false,
            theme: "dark",
            themeVariables: {
              primaryColor: "var(--accent)",
              primaryTextColor: "#c0caf5",
              lineColor: "#565f89",
              secondaryColor: "#bb9af7",
            },
          });
          mermaidInitialized.current = true;
        }

        const id = `mermaid-diagram-${Date.now()}`;
        const { svg } = await mermaid.default.render(id, diagram.mermaid);
        if (!cancelled && mermaidRef.current) {
          mermaidRef.current.innerHTML = svg;
        }
      } catch {
        if (!cancelled) {
          setRenderError(true);
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [diagram]);

  const handleSearch = () => {
    if (target.trim()) {
      setRenderError(false);
      setSearchTarget(target.trim());
    }
  };

  const handleCopy = async () => {
    if (diagram?.mermaid) {
      const ok = await copyTextToClipboard("```mermaid\n" + diagram.mermaid + "\n```");
      if (ok) {
        toast.success(t("diagram.copied"));
      } else {
        toast.error(t("diagram.copyFailed"));
      }
    }
  };

  return (
    <div className="h-full overflow-auto" style={{ padding: 24 }}>
      <h2 className="text-lg font-semibold" style={{ color: "var(--text-0)", marginBottom: 20 }}>
        <Workflow size={20} style={{ display: "inline", marginRight: 8, verticalAlign: "text-bottom" }} />
        {t("sidebar.diagram")}
      </h2>

      {/* Search bar with autocomplete */}
      <div style={{ display: "flex", gap: 8, marginBottom: 24 }}>
        <SymbolAutocomplete
          value={target}
          onChange={setTarget}
          onSelect={(result: SearchResult) => {
            setTarget(result.name);
            setRenderError(false);
            setSearchTarget(result.name);
          }}
          placeholder={t("diagram.placeholder")}
        />
        <button
          onClick={handleSearch}
          style={{
            padding: "8px 16px",
            borderRadius: "var(--radius-md)",
            background: "var(--accent)",
            color: "white",
            fontSize: 12,
            fontWeight: 600,
            border: "none",
            cursor: "pointer",
            flexShrink: 0,
          }}
        >
          {t("diagram.generate")}
        </button>
      </div>

      {isLoading && (
        <div style={{ color: "var(--text-3)", padding: 20, textAlign: "center" }}>
          {t("diagram.generating")}
        </div>
      )}

      {diagram && (
        <div>
          {/* Header with copy button */}
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              marginBottom: 12,
            }}
          >
            <span style={{ fontSize: 12, color: "var(--text-2)" }}>
              {diagram.targetLabel}: <strong style={{ color: "var(--text-0)" }}>{diagram.targetName}</strong>
            </span>
            <button
              onClick={handleCopy}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 4,
                padding: "4px 10px",
                borderRadius: "var(--radius-sm)",
                border: "1px solid var(--surface-border)",
                background: "var(--bg-2)",
                color: "var(--text-2)",
                fontSize: 11,
                cursor: "pointer",
              }}
            >
              <Copy size={12} />
              {t("diagram.copyMermaid")}
            </button>
          </div>

          {/* Rendered Mermaid diagram */}
          <div
            ref={mermaidRef}
            style={{
              padding: 16,
              borderRadius: "var(--radius-lg)",
              border: "1px solid var(--surface-border)",
              background: "var(--bg-2)",
              overflow: "auto",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              minHeight: 200,
            }}
          />

          {renderError && (
            <pre
              style={{
                marginTop: 12,
                padding: 16,
                borderRadius: "var(--radius-lg)",
                border: "1px solid var(--surface-border)",
                background: "var(--bg-2)",
                color: "var(--text-2)",
                fontSize: 12,
                fontFamily: "var(--font-mono)",
                overflow: "auto",
                whiteSpace: "pre-wrap",
                lineHeight: 1.6,
              }}
            >
              {diagram.mermaid}
            </pre>
          )}
        </div>
      )}

      {queryError && (
        <div style={{ color: "var(--rose)", padding: 20, textAlign: "center", fontSize: 13 }}>
          {String(queryError)}
        </div>
      )}

      {!diagram && !isLoading && !queryError && searchTarget && (
        <div style={{ color: "var(--text-3)", padding: 20, textAlign: "center" }}>
          {t("diagram.noDiagram")}
        </div>
      )}
    </div>
  );
}
