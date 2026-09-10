import { AlertCircle, Network } from "lucide-react";
import { GraphToolbar } from "./GraphToolbar";
import { LoadingOrbs } from "../shared/LoadingOrbs";
import { useI18n } from "../../hooks/use-i18n";
import type { GraphStats } from "../../lib/tauri-commands";

interface ToolbarProps {
  stats: GraphStats | undefined;
  layout: string;
  onLayoutChange: (l: string) => void;
  onFit: () => void;
  onExport: () => void;
  hiddenEdgeTypes: Set<string>;
  onToggleEdgeType: (t: string) => void;
  depthFilter: number | null;
  onDepthFilterChange: (d: number | null) => void;
}

type GraphLoadingProps = ToolbarProps;
type GraphEmptyProps = ToolbarProps;
type GraphErrorProps = ToolbarProps & { error: unknown };

export function GraphLoading(props: GraphLoadingProps) {
  const { t } = useI18n();
  return (
    <div className="h-full flex flex-col">
      <GraphToolbar {...props} />
      <div className="flex-1">
        <LoadingOrbs label={t("graph.loadingGraph")} />
      </div>
    </div>
  );
}

export function GraphEmpty(props: GraphEmptyProps) {
  const { t } = useI18n();
  return (
    <div className="h-full flex flex-col">
      <GraphToolbar {...props} />
      <div
        className="flex-1 relative flex flex-col items-center justify-center gap-4 overflow-hidden"
        style={{ backgroundColor: "var(--bg-1)", color: "var(--text-3)" }}
      >
        <div
          className="flex items-center justify-center"
          style={{
            width: 96,
            height: 96,
            borderRadius: "var(--radius-md)",
            backgroundColor: "var(--bg-3)",
            border: "2px dashed var(--surface-border)",
          }}
        >
          <Network size={64} style={{ color: "var(--text-4)" }} />
        </div>
        <p className="text-lg font-medium">{t("graph.noData")}</p>
        <p className="text-sm">{t("graph.analyzeFirst")}</p>
      </div>
    </div>
  );
}

export function GraphError({ error, ...props }: GraphErrorProps) {
  const { t } = useI18n();
  return (
    <div className="h-full flex flex-col">
      <GraphToolbar {...props} />
      <div
        className="flex-1 relative flex items-center justify-center"
        style={{ backgroundColor: "var(--bg-1)" }}
      >
        <div className="flex flex-col items-center gap-3">
          <div
            className="p-3 rounded-lg"
            style={{ backgroundColor: "var(--rose)", opacity: 0.15 }}
          >
            <AlertCircle size={24} style={{ color: "var(--rose)" }} />
          </div>
          <div className="text-center">
            <p style={{ color: "var(--text-2)", fontSize: 14, fontWeight: 500, marginBottom: 4 }}>
              {t("graph.failedToLoad")}
            </p>
            <p style={{ color: "var(--text-3)", fontSize: 12 }}>{String(error)}</p>
          </div>
        </div>
      </div>
    </div>
  );
}
