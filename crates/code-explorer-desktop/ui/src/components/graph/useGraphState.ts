import { useState } from "react";
import type { ViewMode } from "./ViewModeToggle";

export interface GraphState {
  // View state
  viewMode: ViewMode;
  setViewMode: (mode: ViewMode) => void;
  legendExpanded: boolean;
  setLegendExpanded: (v: boolean) => void;
  flowsOpen: boolean;
  setFlowsOpen: (v: boolean) => void;
  minimapVisible: boolean;
  setMinimapVisible: (v: boolean) => void;
  minimapOpacity: number;
  setMinimapOpacity: (v: number) => void;
  shortcutsOpen: boolean;
  setShortcutsOpen: (v: boolean | ((prev: boolean) => boolean)) => void;
  layout: string;
  setLayout: (v: string) => void;

  // Impact overlay
  impactOverlay: boolean;
  setImpactOverlay: (v: boolean) => void;
  impactNodeIds: Map<string, number>;
  setImpactNodeIds: (v: Map<string, number>) => void;

  // Focus/filter
  focusNodeId: string | null;
  setFocusNodeId: (v: string | null) => void;
  highlightedNodeType: string | null;
  setHighlightedNodeType: (v: string | null) => void;
  complexityThreshold: number;
  setComplexityThreshold: (v: number) => void;
  hotspotDays: number;
  setHotspotDays: (v: number) => void;
  showDeadCode: boolean;
  setShowDeadCode: (v: boolean) => void;
  hiddenEdgeTypes: Set<string>;
  setHiddenEdgeTypes: (updater: (prev: Set<string>) => Set<string>) => void;
  depthFilter: number | null;
  setDepthFilter: (v: number | null) => void;

  // Ego-network
  egoNodeIds: Set<string> | undefined;
  setEgoNodeIds: (v: Set<string> | undefined) => void;
  egoDepthMap: Map<string, number> | undefined;
  setEgoDepthMap: (v: Map<string, number> | undefined) => void;

  // Hover card
  hoveredNode: {
    id: string;
    name: string;
    label: string;
    filePath: string;
    startLine?: number;
    endLine?: number;
    parameterCount?: number;
    returnType?: string;
    isTraced?: boolean;
    isDeadCandidate?: boolean;
    complexity?: number;
  } | null;
  setHoveredNode: (v: GraphState["hoveredNode"]) => void;
  hoverPos: { x: number; y: number } | null;
  setHoverPos: (v: { x: number; y: number } | null) => void;
  hoverDegrees: { inDeg: number; outDeg: number };
  setHoverDegrees: (v: { inDeg: number; outDeg: number }) => void;

  // Context menu
  contextMenu: {
    x: number;
    y: number;
    nodeId: string;
    name: string;
    filePath: string;
  } | null;
  setContextMenu: (
    v: GraphState["contextMenu"],
  ) => void;
}

export function useGraphState(): GraphState {
  // View state
  const [viewMode, setViewMode] = useState<ViewMode>("graph");
  const [legendExpanded, setLegendExpanded] = useState(false);
  const [flowsOpen, setFlowsOpen] = useState(false);
  const [minimapVisible, setMinimapVisible] = useState(true);
  const [minimapOpacity, setMinimapOpacity] = useState(0.8);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const [layout, setLayout] = useState("forceatlas2");

  // Impact overlay
  const [impactOverlay, setImpactOverlay] = useState(false);
  const [impactNodeIds, setImpactNodeIds] = useState<Map<string, number>>(
    new Map(),
  );

  // Focus/filter
  const [focusNodeId, setFocusNodeId] = useState<string | null>(null);
  const [highlightedNodeType, setHighlightedNodeType] = useState<string | null>(null);
  const [complexityThreshold, setComplexityThreshold] = useState<number>(0);
  const [hotspotDays, setHotspotDays] = useState<number>(90);
  const [showDeadCode, setShowDeadCode] = useState<boolean>(false);
  const [hiddenEdgeTypes, setHiddenEdgeTypes] = useState<Set<string>>(
    new Set(["IMPORTS", "HAS_METHOD", "HAS_PROPERTY", "CONTAINS"]),
  );
  const [depthFilter, setDepthFilter] = useState<number | null>(null);

  // Ego-network
  const [egoNodeIds, setEgoNodeIds] = useState<Set<string> | undefined>();
  const [egoDepthMap, setEgoDepthMap] = useState<
    Map<string, number> | undefined
  >();

  // Hover card
  const [hoveredNode, setHoveredNode] = useState<GraphState["hoveredNode"]>(null);
  const [hoverPos, setHoverPos] = useState<{ x: number; y: number } | null>(
    null,
  );
  const [hoverDegrees, setHoverDegrees] = useState<{
    inDeg: number;
    outDeg: number;
  }>({ inDeg: 0, outDeg: 0 });

  // Context menu
  const [contextMenu, setContextMenu] =
    useState<GraphState["contextMenu"]>(null);

  return {
    viewMode,
    setViewMode,
    legendExpanded,
    setLegendExpanded,
    flowsOpen,
    setFlowsOpen,
    minimapVisible,
    setMinimapVisible,
    minimapOpacity,
    setMinimapOpacity,
    shortcutsOpen,
    setShortcutsOpen,
    layout,
    setLayout,
    impactOverlay,
    setImpactOverlay,
    impactNodeIds,
    setImpactNodeIds,
    focusNodeId,
    setFocusNodeId,
    highlightedNodeType,
    setHighlightedNodeType,
    complexityThreshold,
    setComplexityThreshold,
    hotspotDays,
    setHotspotDays,
    showDeadCode,
    setShowDeadCode,
    hiddenEdgeTypes,
    setHiddenEdgeTypes,
    depthFilter,
    setDepthFilter,
    egoNodeIds,
    setEgoNodeIds,
    egoDepthMap,
    setEgoDepthMap,
    hoveredNode,
    setHoveredNode,
    hoverPos,
    setHoverPos,
    hoverDegrees,
    setHoverDegrees,
    contextMenu,
    setContextMenu,
  };
}
