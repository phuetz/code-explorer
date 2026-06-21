import { Tooltip } from "../shared/Tooltip";
import { useI18n } from "../../hooks/use-i18n";

interface GraphZoomControlsProps {
  onZoomIn: () => void;
  onZoomOut: () => void;
  onFitView: () => void;
  legendExpanded: boolean;
}

export function GraphZoomControls({
  onZoomIn,
  onZoomOut,
  onFitView,
  legendExpanded,
}: GraphZoomControlsProps) {
  const { t } = useI18n();
  const btnStyle = {
    background: "var(--bg-2)",
    border: "1px solid var(--surface-border)",
    color: "var(--text-2)",
    cursor: "pointer",
  } as const;

  return (
    <div
      className="absolute z-20 flex flex-col gap-1"
      style={{ bottom: legendExpanded ? 200 : 80, right: 16 }}
    >
      <Tooltip content={t("zoom.in")}>
        <button
          onClick={onZoomIn}
          className="w-11 h-11 rounded-lg flex items-center justify-center text-sm font-bold transition-colors hover:brightness-125 focus-visible:ring-2 focus-visible:ring-[var(--accent)] focus-visible:outline-none"
          style={btnStyle}
          aria-label={t("zoom.inLabel")}
        >
          +
        </button>
      </Tooltip>
      <Tooltip content={t("zoom.out")}>
        <button
          onClick={onZoomOut}
          className="w-11 h-11 rounded-lg flex items-center justify-center text-sm font-bold transition-colors hover:brightness-125 focus-visible:ring-2 focus-visible:ring-[var(--accent)] focus-visible:outline-none"
          style={btnStyle}
          aria-label={t("zoom.outLabel")}
        >
          {"\u2212"}
        </button>
      </Tooltip>
      <Tooltip content={t("zoom.fit")}>
        <button
          onClick={onFitView}
          className="w-11 h-11 rounded-lg flex items-center justify-center text-xs font-bold transition-colors hover:brightness-125 focus-visible:ring-2 focus-visible:ring-[var(--accent)] focus-visible:outline-none"
          style={btnStyle}
          aria-label={t("zoom.fitLabel")}
        >
          {"\u229E"}
        </button>
      </Tooltip>
    </div>
  );
}
