import { Maximize2, ChevronDown, GitBranch, Download, HelpCircle, Skull, Users, ArrowRightLeft, Route, Layers } from "lucide-react";
import { useAppStore } from "../../stores/app-store";
import { useI18n } from "../../hooks/use-i18n";
import { Tooltip } from "../shared/Tooltip";
import { AnimatedCounter } from "../shared/motion";
import type { GraphStats, ZoomLevel, SavedView, CameraState } from "../../lib/tauri-commands";
import { useState } from "react";
import { toast } from "sonner";
import { SavedViewsMenu } from "./SavedViewsMenu";

/** Theme C — top-level graph view mode. Local to GraphExplorer (not in store). */
export type GraphMode = "normal" | "diff" | "path";

const ZOOM_LEVELS: { id: ZoomLevel; i18nKey: string }[] = [
  { id: "package", i18nKey: "graph.packages" },
  { id: "module", i18nKey: "graph.modules" },
  { id: "symbol", i18nKey: "graph.symbols" },
];

const LAYOUTS = [
  { id: "forceatlas2", label: "Force" },
  { id: "grid", label: "Grid" },
  { id: "circle", label: "Circle" },
  { id: "random", label: "Random" },
];

export function GraphToolbar({
  stats,
  layout,
  onLayoutChange,
  onFit,
  onFlows,
  onExport,
  hiddenEdgeTypes,
  onToggleEdgeType,
  complexityThreshold,
  onComplexityChange,
  hotspotDays,
  onHotspotDaysChange,
  showDeadCode,
  onToggleDeadCode,
  // Theme C
  viewMode,
  onViewModeChange,
  collectViewState,
  onApplyView,
}: {
  stats?: GraphStats;
  layout: string;
  onLayoutChange: (layout: string) => void;
  onFit: () => void;
  onFlows?: () => void;
  onExport?: () => void;
  hiddenEdgeTypes?: Set<string>;
  onToggleEdgeType?: (type: string) => void;
  complexityThreshold?: number;
  onComplexityChange?: (v: number) => void;
  hotspotDays?: number;
  onHotspotDaysChange?: (v: number) => void;
  showDeadCode?: boolean;
  onToggleDeadCode?: () => void;
  /** Theme C — current top-level graph mode (Normal/Diff/Path). */
  viewMode?: GraphMode;
  onViewModeChange?: (m: GraphMode) => void;
  /** Theme C — capture current state when user clicks "Save view". */
  collectViewState?: () => {
    name?: string;
    lens?: string;
    filters?: unknown;
    cameraState?: CameraState;
    nodeSelection?: string[];
  };
  /** Theme C — apply a previously-saved view. */
  onApplyView?: (view: SavedView) => void;
}) {
  const { t, tt } = useI18n();
  const zoomLevel = useAppStore((s) => s.zoomLevel);
  const setZoomLevel = useAppStore((s) => s.setZoomLevel);
  const activeLens = useAppStore((s) => s.activeLens);
  const clusterByCommunity = useAppStore((s) => s.clusterByCommunity);
  const setClusterByCommunity = useAppStore((s) => s.setClusterByCommunity);
  const riskThreshold = useAppStore((s) => s.riskThreshold);
  const setRiskThreshold = useAppStore((s) => s.setRiskThreshold);
  const [showLayoutMenu, setShowLayoutMenu] = useState(false);

  const layoutLabel = LAYOUTS.find((l) => l.id === layout)?.label || "Layout";

  return (
    <div
      role="toolbar"
      aria-label={t("graph.edgeFilters")}
      className="flex items-center gap-4 px-4 py-2.5 border-b border-surface-border flex-wrap bg-bg-2"
    >
      {/* Zoom Level Pill Toggle Group */}
      <div
        className="flex rounded-full p-1 gap-1 bg-surface border border-surface-border"
        title={t("graph.granularity")}
      >
        {ZOOM_LEVELS.map(({ id, i18nKey }) => {
          const { label, tip } = tt(i18nKey);
          const active = zoomLevel === id;
          return (
            <Tooltip key={id} content={tip}>
              <button
                onClick={() => setZoomLevel(id)}
                className={`relative px-3 py-1.5 text-xs transition-all hover:bg-surface-hover rounded-md cursor-pointer ${
                  active 
                    ? "text-accent bg-bg-2 font-semibold shadow-[0_1px_3px_rgba(0,0,0,0.2),_inset_0_-2px_0_var(--accent)]" 
                    : "text-text-3 bg-transparent font-medium"
                }`}
              >
                {label}
              </button>
            </Tooltip>
          );
        })}
      </div>

      {/* Layout Dropdown Button */}
      <div className="relative">
        <Tooltip content={tt("graph.layout").tip}>
          <button
            onClick={() => setShowLayoutMenu(!showLayoutMenu)}
            aria-label={tt("graph.layout").label}
            className="flex items-center gap-2 px-3 py-1.5 text-xs font-medium rounded-md transition-all hover:border-surface-border-hover bg-surface text-text-2 border border-surface-border cursor-pointer"
          >
            {layoutLabel}
            <ChevronDown
              size={14}
              className={`transition-transform duration-200 ${showLayoutMenu ? "rotate-180" : "rotate-0"}`}
            />
          </button>
        </Tooltip>

        {/* Dropdown Menu */}
        {showLayoutMenu && (
          <div
            className="absolute left-0 top-full mt-1 rounded-md overflow-hidden shadow-lg z-50 bg-surface border border-surface-border min-w-[120px]"
            onMouseLeave={() => setShowLayoutMenu(false)}
          >
            {LAYOUTS.map(({ id, label }) => {
              const active = layout === id;
              return (
                <button
                  key={id}
                  onClick={() => {
                    onLayoutChange(id);
                    setShowLayoutMenu(false);
                  }}
                  className={`w-full text-left px-3 py-2 text-xs transition-colors hover:bg-surface-hover cursor-pointer ${
                    active ? "text-accent bg-bg-2" : "text-text-2 bg-transparent"
                  }`}
                >
                  {label}
                </button>
              );
            })}
          </div>
        )}
      </div>

      {/* Fit Button */}
      <Tooltip content={tt("graph.fitView").tip}>
        <button
          onClick={onFit}
          aria-label={tt("graph.fitView").label}
          className="p-2 rounded-md transition-all hover:bg-surface-hover hover:text-text-2 text-text-3 cursor-pointer"
        >
          <Maximize2 size={16} />
        </button>
      </Tooltip>

      {/* Theme C — View mode radio (Normal / Diff / Path) */}
      {onViewModeChange && (
        <div className="flex rounded-md p-0.5 gap-0.5 bg-surface border border-surface-border">
          {([
            { id: "normal" as const, label: "Normal", Icon: Layers },
            { id: "diff" as const, label: "Diff", Icon: ArrowRightLeft },
            { id: "path" as const, label: "Path", Icon: Route },
          ]).map(({ id, label, Icon }) => {
            const active = (viewMode ?? "normal") === id;
            return (
              <button
                key={id}
                onClick={() => onViewModeChange(id)}
                title={`${label} mode`}
                aria-pressed={active}
                className={`flex items-center gap-1 px-2 py-1 text-[11px] rounded transition-all cursor-pointer ${
                  active
                    ? "bg-accent text-white font-semibold"
                    : "bg-transparent text-text-3 hover:text-text-2 hover:bg-surface-hover"
                }`}
              >
                <Icon size={11} />
                {label}
              </button>
            );
          })}
        </div>
      )}

      {/* Theme C — Saved Views menu */}
      {collectViewState && onApplyView && (
        <SavedViewsMenu
          collectCurrentState={collectViewState}
          onApplyView={onApplyView}
        />
      )}

      {/* Export PNG Button */}
      {onExport && (
        <Tooltip content={t("graph.exportPng")}>
          <button
            onClick={onExport}
            aria-label={t("graph.exportPng")}
            className="p-2 rounded-md transition-all hover:bg-surface-hover hover:text-text-2 text-text-3 cursor-pointer"
          >
            <Download size={16} />
          </button>
        </Tooltip>
      )}

      {/* Flows Button */}
      {onFlows && (
        <Tooltip content={t("graph.processFlows")}>
          <button
            onClick={onFlows}
            aria-label={t("graph.processFlows")}
            className="p-2 rounded-md transition-all hover:bg-bg-3 hover:text-text-0 text-text-3 cursor-pointer"
          >
            <GitBranch size={16} />
          </button>
        </Tooltip>
      )}

      {/* Help Button */}
      <Tooltip content={t("graph.help") || "Aide"}>
        <button
          onClick={() => {
            toast.info(t("graph.helpTitle") || "Comment lire ce graphe ?", {
              description: t("graph.helpDescription") || "Les gros nœuds sont des fichiers/classes. Les petits sont des méthodes. Cliquez sur un nœud pour voir ses voisins, ou sur la légende pour filtrer par type.",
              duration: 10000,
            });
          }}
          aria-label={t("graph.help") || "Aide"}
          className="p-2 rounded-md transition-all hover:bg-bg-3 hover:text-text-0 text-text-3 cursor-pointer"
        >
          <HelpCircle size={16} />
        </button>
      </Tooltip>

      {/* Dead Code Toggle */}
      {onToggleDeadCode !== undefined && (
        <Tooltip content={t("graph.deadCode") || "Code Mort"}>
          <button
            onClick={onToggleDeadCode}
            aria-label={t("graph.deadCode") || "Code Mort"}
            className={`p-2 rounded-md transition-all hover:bg-bg-3 cursor-pointer border ${
              showDeadCode ? "bg-[rgba(247,118,142,0.2)] text-rose border-rose" : "bg-transparent text-text-3 border-transparent"
            }`}
          >
            <Skull size={16} />
          </button>
        </Tooltip>
      )}

      {/* Community Clustering Toggle */}
      <Tooltip content={t("graph.clusterByCommunity") || "Clustering par communauté"}>
        <button
          onClick={() => setClusterByCommunity(!clusterByCommunity)}
          aria-label={t("graph.clusterByCommunity") || "Clustering par communauté"}
          className={`p-2 rounded-md transition-all hover:bg-bg-3 cursor-pointer border ${
            clusterByCommunity ? "bg-accent-subtle text-accent border-accent" : "bg-transparent text-text-3 border-transparent"
          }`}
        >
          <Users size={16} />
        </button>
      </Tooltip>

      {/* Risk Score Filter (Phase 5) */}
      {activeLens === "risk" && (
        <div className="flex items-center gap-2 px-3 py-1 rounded-md border border-surface-border bg-surface">
          <span className="text-[10px] font-medium text-text-3">{t("toolbar.riskThreshold") || "Risk"} &gt; {(riskThreshold * 100).toFixed(0)}%</span>
          <input
            type="range"
            min="0"
            max="1"
            step="0.05"
            value={riskThreshold}
            onChange={(e) => setRiskThreshold(parseFloat(e.target.value))}
            className="w-24 h-1 bg-bg-3 rounded-lg appearance-none cursor-pointer accent-rose"
          />
        </div>
      )}

      {/* Complexity Filter */}
      {onComplexityChange !== undefined && (
        <div className="flex items-center gap-2 px-3 py-1 rounded-md border border-surface-border bg-surface">
          <span className="text-[10px] font-medium text-text-3">{t("toolbar.complexity")} &gt; {complexityThreshold}</span>
          <input
            type="range"
            min="0"
            max="100"
            value={complexityThreshold}
            onChange={(e) => onComplexityChange(parseInt(e.target.value))}
            className="w-20 h-1 bg-bg-3 rounded-lg appearance-none cursor-pointer accent-accent"
          />
        </div>
      )}

      {/* Hotspot Time Range Filter */}
      {onHotspotDaysChange !== undefined && (
        <div className="flex items-center gap-1 px-2 py-1 rounded-md border border-surface-border bg-surface">
          <span className="text-[10px] font-medium text-text-3 mr-1">{t("toolbar.gitRange")}:</span>
          {[30, 90, 365].map(d => {
            const active = hotspotDays === d;
            const label = t("toolbar.hotspotDays").replace("{0}", String(d));
            return (
              <button
                key={d}
                onClick={() => onHotspotDaysChange(d)}
                title={label}
                aria-label={label}
                aria-pressed={active}
                className={`px-1.5 py-0.5 rounded text-[9px] font-bold transition-all cursor-pointer ${
                  active ? "bg-accent text-white border-none" : "bg-transparent text-text-3 border border-surface-border"
                }`}
              >
                {d}d
              </button>
            );
          })}
        </div>
      )}

      {/* Stats Badges */}
      {stats && (
        <div className="flex gap-2 ml-auto">
          <div className="px-2.5 py-1 rounded-full text-xs font-medium bg-accent-subtle text-accent">
            <AnimatedCounter value={stats.nodeCount} /> {t("graph.nodesCount")}
          </div>
          <div className="px-2.5 py-1 rounded-full text-xs font-medium bg-accent-subtle text-accent">
            <AnimatedCounter value={stats.edgeCount} /> {t("graph.edgesCount")}
          </div>
          {stats.truncated && (
            <div className="px-2.5 py-1 rounded-full text-xs font-medium bg-amber text-bg-0">
              {t("graph.truncated")}
            </div>
          )}
        </div>
      )}

      {/* Edge Type Filters */}
      {hiddenEdgeTypes && onToggleEdgeType && (
        <div className="flex items-center gap-1 ml-2 pl-2 border-l border-surface-border">
          <span className="text-[10px] font-medium text-text-3">{t("graph.edges")}:</span>
          {["CALLS", "IMPORTS", "HAS_METHOD", "EXTENDS", "CALLS_ACTION", "CONTAINS"].map((type) => {
            const active = !hiddenEdgeTypes.has(type);
            return (
              <button
                key={type}
                onClick={() => onToggleEdgeType(type)}
                className={`px-1.5 py-0.5 rounded text-[9px] font-medium transition-colors cursor-pointer border ${
                  active ? "bg-accent-subtle text-accent border-accent opacity-100" : "bg-bg-3 text-text-4 border-surface-border opacity-50"
                }`}
                aria-pressed={active}
                aria-label={`Toggle ${type.replace("_", " ")} edges`}
              >
                {type.replace("_", " ")}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
