import { MousePointerClick } from "lucide-react";
import { useAppStore } from "../../stores/app-store";
import { DetailPanel } from "../layout/DetailPanel";
import { CodeInspectorPanel } from "../layout/CodeInspectorPanel";
import { EmptyState } from "../shared/EmptyState";
import { useI18n } from "../../hooks/use-i18n";

export function ExplorerRightPanel() {
  const { t } = useI18n();
  const selectedNodeId = useAppStore((s) => s.selectedNodeId);

  if (!selectedNodeId) {
    return (
      <div
        className="h-full"
        style={{
          background: "var(--glass-bg)",
          backdropFilter: "blur(var(--glass-blur))",
          borderLeft: "1px solid var(--glass-border)",
        }}
      >
        <EmptyState
          icon={MousePointerClick}
          title={t("detail.noSelection")}
          description={t("detail.noSelectionHint")}
        />
      </div>
    );
  }

  return (
    <div
      className="flex flex-col h-full overflow-hidden"
      aria-label="Symbol inspector"
      style={{
        background: "var(--glass-bg)",
        backdropFilter: "blur(var(--glass-blur))",
        borderLeft: "1px solid var(--glass-border)",
      }}
    >
      {/* Top: Code preview */}
      <div
        className="shrink-0"
        style={{
          maxHeight: "40%",
          borderBottom: "1px solid var(--surface-border)",
          overflow: "auto",
        }}
      >
        <CodeInspectorPanel />
      </div>

      {/* Bottom: Detail tabs */}
      <div className="flex-1 min-h-0 overflow-auto">
        <DetailPanel />
      </div>
    </div>
  );
}
