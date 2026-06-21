import { lazy, Suspense, useCallback, useMemo, useRef, useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { toast } from "sonner";
import { Zap, Route as RouteIcon } from "lucide-react";
import { useGraphData } from "../../hooks/use-tauri-query";
import { useAppStore } from "../../stores/app-store";
import { useChatStore } from "../../stores/chat-store";
import { useSigma } from "../../hooks/use-sigma";
import { commands } from "../../lib/tauri-commands";
import { buildGraphologyGraph } from "../../lib/graph-adapter";
import { NodeHoverCard } from "./NodeHoverCard";
import { useI18n } from "../../hooks/use-i18n";
import { copyTextToClipboard } from "../../lib/clipboard";
import { LENS_EDGE_TYPES } from "../explorer/lens-constants";
import { useGraphState } from "./useGraphState";
import { useGraphEffects } from "./useGraphEffects";
import { useGraphLenses } from "../../hooks/use-graph-lenses";
import { useGraphCommunities } from "../../hooks/use-graph-communities";
import { GraphContextMenu } from "./GraphContextMenu";
import { GraphToolbarRow } from "./GraphToolbarRow";
import { GraphZoomControls } from "./GraphZoomControls";
import { GraphLoading, GraphEmpty, GraphError } from "./GraphEmptyStates";
import type { GraphFilter, SavedView, CameraState } from "../../lib/tauri-commands";
import type { GraphMode } from "./GraphToolbar";

const TreemapView = lazy(() =>
  import("./TreemapView").then((m) => ({ default: m.TreemapView })),
);
const CypherQueryFAB = lazy(() =>
  import("./CypherQueryFAB").then((m) => ({ default: m.CypherQueryFAB })),
);
const ProcessFlowModal = lazy(() =>
  import("./ProcessFlowModal").then((m) => ({ default: m.ProcessFlowModal })),
);
const GraphLegend = lazy(() =>
  import("./GraphLegend").then((m) => ({ default: m.GraphLegend })),
);
const CommunitiesPanel = lazy(() =>
  import("./CommunitiesPanel").then((m) => ({ default: m.CommunitiesPanel })),
);
const GraphMinimap = lazy(() =>
  import("./GraphMinimap").then((m) => ({ default: m.GraphMinimap })),
);
const GraphShortcutsOverlay = lazy(() =>
  import("./GraphShortcutsOverlay").then((m) => ({ default: m.GraphShortcutsOverlay })),
);

export function GraphExplorer() {
  const { t } = useI18n();

  // ── Store ────────────────────────────────────────────────────────
  const zoomLevel = useAppStore((s) => s.zoomLevel);
  const setSelectedNodeId = useAppStore((s) => s.setSelectedNodeId);
  const setMode = useAppStore((s) => s.setMode);
  const setSearchOpen = useAppStore((s) => s.setSearchOpen);
  const setSearchQuery = useAppStore((s) => s.setSearchQuery);
  const selectedNodeId = useAppStore((s) => s.selectedNodeId);
  const searchMatchIds = useAppStore((s) => s.searchMatchIds);
  const activeLens = useAppStore((s) => s.activeLens);
  const egoDepth = useAppStore((s) => s.egoDepth);
  const selectedFeatures = useAppStore((s) => s.selectedFeatures);
  const activeRepo = useAppStore((s) => s.activeRepo);
  const clusterByCommunity = useAppStore((s) => s.clusterByCommunity);

  // ── Local state ──────────────────────────────────────────────────
  const gs = useGraphState();

  // ── Theme C — Diff/Path mode state (kept local per plan: no new store fields).
  const [graphMode, setGraphMode] = useState<GraphMode>("normal");
  const [diffPicker, setDiffPicker] = useState<{ from: string; to: string }>({
    from: "",
    to: "live",
  });
  const [pathState, setPathState] = useState<{
    from: string | null;
    to: string | null;
    path: string[] | null;
  }>({ from: null, to: null, path: null });

  const {
    setContextMenu, setHoveredNode, setHoverPos, setHoverDegrees,
    setFocusNodeId, setImpactNodeIds, setImpactOverlay,
    setLayout, setHiddenEdgeTypes, highlightedNodeType, setHighlightedNodeType,
    complexityThreshold, setComplexityThreshold,
  } = gs;

  const effectiveHiddenEdgeTypes = useMemo(() => {
    const lensEdgeTypes = LENS_EDGE_TYPES[activeLens];
    if (!lensEdgeTypes) return gs.hiddenEdgeTypes;
    const allKnown = [
      "CALLS", "IMPORTS", "DEPENDS_ON", "HAS_METHOD", "HAS_PROPERTY",
      "CONTAINED_IN", "DEFINED_IN", "EXTENDS", "IMPLEMENTS", "INHERITS",
      "CONTAINS", "REFERENCES",
    ];
    const next = new Set(gs.hiddenEdgeTypes);
    for (const type of allKnown) {
      if (!lensEdgeTypes.includes(type)) next.add(type); else next.delete(type);
    }
    return next;
  }, [gs.hiddenEdgeTypes, activeLens]);

  // ── Data ──
  const { data, isLoading, error } = useGraphData({ zoomLevel, maxNodes: 200 } as GraphFilter, true);
  const { data: subgraphData } = useQuery({
    queryKey: ["subgraph", activeRepo, gs.focusNodeId],
    queryFn: () => commands.getSubgraph(gs.focusNodeId!, 2),
    enabled: !!gs.focusNodeId,
    staleTime: 30_000,
  });
  const activeData = gs.focusNodeId && subgraphData ? subgraphData : data;

  const highlightedNodeIdsFromType = useMemo(() => {
    if (!highlightedNodeType || !activeData) return new Set<string>();
    return new Set(activeData.nodes.filter(n => n.label === highlightedNodeType).map(n => n.id));
  }, [highlightedNodeType, activeData]);

  const combinedHighlightedNodeIds = useMemo(() => {
    const set = new Set(searchMatchIds);
    highlightedNodeIdsFromType.forEach(id => set.add(id));
    return set;
  }, [searchMatchIds, highlightedNodeIdsFromType]);

  // ── Handlers Ref (to break circular dependency with useSigma) ──
  const handlersRef = useRef<{
    onNodeClick: ((nodeId: string | null) => void) | null;
    onNodeHover: ((nodeId: string | null) => void) | null;
    onNodeRightClick: ((nodeId: string, x: number, y: number) => void) | null;
    onNodeDoubleClick: ((nodeId: string) => void) | null;
  }>({
    onNodeClick: null,
    onNodeHover: null,
    onNodeRightClick: null,
    onNodeDoubleClick: null,
  });

  // ── Sigma ────────────────────────────────────────────────────────
  const {
    containerRef, graphRef, isLayoutRunning,
    setGraph, runLayout, focusNode, fitView,
    zoomIn, zoomOut, exportPNG, refresh, sigmaRef,
  } = useSigma({
    selectedNodeId,
    hoveredNodeId: gs.hoveredNode?.id,
    highlightedNodeIds: combinedHighlightedNodeIds,
    impactNodeIds: gs.impactOverlay ? gs.impactNodeIds : undefined,
    egoNodeIds: gs.egoNodeIds,
    egoDepthMap: gs.egoDepthMap,
    onNodeClick: useCallback((nodeId: string | null) => handlersRef.current.onNodeClick?.(nodeId), []),
    onNodeHover: useCallback((nodeId: string | null) => handlersRef.current.onNodeHover?.(nodeId), []),
    onNodeRightClick: useCallback((nodeId: string, x: number, y: number) => handlersRef.current.onNodeRightClick?.(nodeId, x, y), []),
    onNodeDoubleClick: useCallback((nodeId: string) => handlersRef.current.onNodeDoubleClick?.(nodeId), []),
  });

  // Sync actual handler implementations into the ref after render
  // (avoids "Cannot access refs during render" lint error).
  // Theme C — `pathClickRef` is updated below where `handlePathClick` is
  // defined; we read it through the ref so this callback's identity stays
  // stable regardless of `pathState` changes.
  const pathClickRef = useRef<((nodeId: string | null) => void) | null>(null);
  const handleNodeClick = useCallback((nodeId: string | null) => {
    // In Path mode the click is consumed by path picking, not by the normal
    // selection flow. Selection still updates so the user has visual feedback.
    if (graphMode === "path") {
      pathClickRef.current?.(nodeId);
    }
    if (nodeId) {
      const g = graphRef.current;
      setSelectedNodeId(nodeId, g?.hasNode(nodeId) ? g.getNodeAttribute(nodeId, "label") : null);
    } else { setSelectedNodeId(null, null); setContextMenu(null); }
  }, [setSelectedNodeId, setContextMenu, graphRef, graphMode]);

  const handleNodeHover = useCallback((nodeId: string | null) => {
    if (!nodeId) { setHoveredNode(null); setHoverPos(null); return; }
    const g = graphRef.current; const sigma = sigmaRef.current;
    if (!g || !sigma || !g.hasNode(nodeId)) return;
    const a = g.getNodeAttributes(nodeId);
    const vp = sigma.graphToViewport({ x: a.x, y: a.y });
    setHoveredNode({ id: nodeId, name: a.label, label: a.nodeType, filePath: a.filePath, startLine: a.startLine, endLine: a.endLine, parameterCount: a.parameterCount, returnType: a.returnType, isTraced: a.isTraced, isDeadCandidate: a.isDeadCandidate, complexity: a.complexity });
    setHoverPos({ x: vp.x, y: vp.y });
    setHoverDegrees({ inDeg: g.inDegree(nodeId), outDeg: g.outDegree(nodeId) });
  }, [setHoveredNode, setHoverPos, setHoverDegrees, graphRef, sigmaRef]);

  const handleNodeRightClick = useCallback((nodeId: string, x: number, y: number) => {
    const g = graphRef.current; if (!g?.hasNode(nodeId)) return;
    const a = g.getNodeAttributes(nodeId);
    setContextMenu({ x, y, nodeId, name: a.label, filePath: a.filePath });
  }, [setContextMenu, graphRef]);

  const handleNodeDoubleClick = useCallback((nodeId: string) => {
    setFocusNodeId(nodeId);
  }, [setFocusNodeId]);

  useEffect(() => {
    handlersRef.current.onNodeClick = handleNodeClick;
    handlersRef.current.onNodeHover = handleNodeHover;
    handlersRef.current.onNodeRightClick = handleNodeRightClick;
    handlersRef.current.onNodeDoubleClick = handleNodeDoubleClick;
  }, [handleNodeClick, handleNodeHover, handleNodeRightClick, handleNodeDoubleClick]);


  const { data: hotspotsData } = useQuery({
    queryKey: ["git-hotspots", activeRepo, gs.hotspotDays],
    queryFn: () => commands.getHotspots(gs.hotspotDays),
    enabled: (activeLens === "hotspots" || activeLens === "risk") && !!activeRepo,
    staleTime: 60_000,
  });

  // ── Graph build effect ────────────────────────────────────────────
  const prevKeyRef = useRef("");
  useEffect(() => {
    if (!activeData || activeData.nodes.length === 0) return;
    const key = `${activeData.stats.nodeCount}-${activeData.stats.edgeCount}-${zoomLevel}-${gs.focusNodeId ?? ""}-${complexityThreshold}-${[...effectiveHiddenEdgeTypes].sort().join(",")}`;
    if (key === prevKeyRef.current) return;
    prevKeyRef.current = key;
    setGraph(buildGraphologyGraph(activeData.nodes, activeData.edges, effectiveHiddenEdgeTypes, complexityThreshold));
    runLayout();
  }, [activeData, zoomLevel, effectiveHiddenEdgeTypes, gs.focusNodeId, complexityThreshold, setGraph, runLayout]);

  // ── Lenses ──
  useGraphLenses({
    activeLens,
    hotspotsData,
    graphRef,
    refresh,
    showDeadCode: gs.showDeadCode,
  });

  useGraphCommunities(graphRef, clusterByCommunity, refresh);

  // ── All other effects ─────────────────────────────────────────────
  useGraphEffects({ gs, selectedNodeId, searchMatchIds, selectedFeatures, egoDepth, graphRef, focusNode, refresh, fitView, zoomIn, zoomOut, exportPNG, setSearchOpen, setSelectedNodeId });

  // ── Impact overlay ────────────────────────────────────────────────
  const toggleImpactOverlay = useCallback(async () => {
    if (gs.impactOverlay) { setImpactNodeIds(new Map()); setImpactOverlay(false); refresh(); return; }
    if (!selectedNodeId) return;
    try {
      const result = await commands.getImpactAnalysis(selectedNodeId, "both", 3);
      const map = new Map<string, number>([[selectedNodeId, 0]]);
      const mark = (items: Array<{ node: { id: string }; depth: number }>) => {
        for (const item of items) if (!map.has(item.node.id)) map.set(item.node.id, item.depth);
      };
      if (result.upstream) mark(result.upstream);
      if (result.downstream) mark(result.downstream);
      setImpactNodeIds(map); setImpactOverlay(true); refresh();
    } catch (e) { console.error("Impact analysis failed:", e); toast.error(t("graph.impactFailed")); }
  }, [selectedNodeId, gs.impactOverlay, refresh, setImpactNodeIds, setImpactOverlay, t]);

  // ── Theme C — Diff overlay fetch ───────────────────────────────
  // When entering Diff mode the user picks "from" + "to" snapshots. We fetch
  // the diff lazily (only when both ids are set) and project the result into
  // three Sets that downstream effects use for coloring.
  const diffQ = useQuery({
    queryKey: ["graph-diff", activeRepo, diffPicker.from, diffPicker.to],
    queryFn: () => commands.diffSnapshots(diffPicker.from, diffPicker.to),
    enabled:
      graphMode === "diff" &&
      !!activeRepo &&
      !!diffPicker.from &&
      !!diffPicker.to &&
      diffPicker.from !== diffPicker.to,
    staleTime: 30_000,
  });

  // Derived state — overlay sets are pure projections of the diff result.
  // Avoiding `useState` + setState-in-effect keeps the React 19 lint happy
  // and removes a render cycle.
  const diffOverlay = useMemo(() => {
    if (graphMode !== "diff" || !diffQ.data) return null;
    return {
      added: new Set(diffQ.data.addedNodes),
      removed: new Set(diffQ.data.removedNodes),
      modified: new Set(diffQ.data.modified.map((m) => m.nodeId)),
    };
  }, [graphMode, diffQ.data]);

  // Apply diff coloring directly on the Graphology graph attributes — Sigma
  // re-renders next frame via `refresh()`. We use the same hook surface as
  // useGraphLenses (mutate node attributes, then call refresh).
  useEffect(() => {
    const g = graphRef.current;
    if (!g) return;
    if (graphMode !== "diff" || !diffOverlay) {
      // Reset any leftover diff coloring.
      g.forEachNode((n) => {
        if (g.getNodeAttribute(n, "diffMark")) {
          g.removeNodeAttribute(n, "diffMark");
        }
      });
      refresh();
      return;
    }
    g.forEachNode((id) => {
      let mark: "added" | "removed" | "modified" | null = null;
      let color: string | null = null;
      if (diffOverlay.added.has(id)) {
        mark = "added";
        color = "#9ece6a"; // green
      } else if (diffOverlay.removed.has(id)) {
        mark = "removed";
        color = "#f7768e"; // rose (rendered ghosty via lower opacity)
      } else if (diffOverlay.modified.has(id)) {
        mark = "modified";
        color = "#e0af68"; // amber
      }
      if (mark && color) {
        g.setNodeAttribute(id, "diffMark", mark);
        g.setNodeAttribute(id, "color", color);
      } else {
        g.removeNodeAttribute(id, "diffMark");
      }
    });
    refresh();
  }, [graphMode, diffOverlay, graphRef, refresh, activeData]);

  // ── Theme C — Path mode click handling ──────────────────────────
  // When path mode is active, normal node-click selects A then B and fires
  // a BFS via the Rust `find_path` command. The path is highlighted; all
  // other nodes get dimmed via a "pathDim" attribute consumed by reducers.
  const handlePathClick = useCallback(
    async (nodeId: string | null) => {
      if (!nodeId) {
        setPathState({ from: null, to: null, path: null });
        return;
      }
      if (!pathState.from) {
        setPathState({ from: nodeId, to: null, path: null });
        toast.info("Path mode: pick a target node");
        return;
      }
      if (pathState.from === nodeId) {
        // Click same source again: clear.
        setPathState({ from: null, to: null, path: null });
        return;
      }
      try {
        const result = await commands.findPath(pathState.from, nodeId, ["CALLS", "IMPORTS"], 10);
        if (!result.found) {
          toast.error("No path found within depth 10");
          setPathState({ from: pathState.from, to: nodeId, path: null });
          return;
        }
        setPathState({ from: pathState.from, to: nodeId, path: result.path });
        toast.success(`Path of ${result.path.length - 1} hop(s) highlighted`);
      } catch (e) {
        toast.error(`find_path failed: ${(e as Error).message}`);
      }
    },
    [pathState.from],
  );
  // Keep the ref pointing at the latest closure so the stable `handleNodeClick`
  // can call into it without re-binding.
  useEffect(() => {
    pathClickRef.current = handlePathClick;
  }, [handlePathClick]);

  // Apply path highlighting / dim other nodes.
  useEffect(() => {
    const g = graphRef.current;
    if (!g) return;
    if (graphMode !== "path") {
      g.forEachNode((n) => {
        g.removeNodeAttribute(n, "pathDim");
        g.removeNodeAttribute(n, "pathOnPath");
      });
      refresh();
      return;
    }
    const pathSet = pathState.path ? new Set(pathState.path) : null;
    g.forEachNode((id) => {
      if (pathSet && pathSet.has(id)) {
        g.setNodeAttribute(id, "pathOnPath", true);
        g.removeNodeAttribute(id, "pathDim");
      } else if (id === pathState.from) {
        g.setNodeAttribute(id, "pathOnPath", true);
        g.removeNodeAttribute(id, "pathDim");
      } else if (pathSet) {
        g.setNodeAttribute(id, "pathDim", true);
        g.removeNodeAttribute(id, "pathOnPath");
      } else {
        g.removeNodeAttribute(id, "pathDim");
        g.removeNodeAttribute(id, "pathOnPath");
      }
    });
    refresh();
  }, [graphMode, pathState, graphRef, refresh, activeData]);

  // ── Theme C — Saved Views (capture + apply) ─────────────────────
  const collectViewState = useCallback(() => {
    const sigma = sigmaRef.current;
    let cameraState: CameraState | undefined;
    if (sigma) {
      const cam = sigma.getCamera().getState();
      cameraState = { x: cam.x, y: cam.y, ratio: cam.ratio, angle: cam.angle ?? 0 };
    }
    const filters = {
      zoomLevel,
      hiddenEdgeTypes: Array.from(gs.hiddenEdgeTypes),
      complexityThreshold,
      hotspotDays: gs.hotspotDays,
      showDeadCode: gs.showDeadCode,
      clusterByCommunity,
    };
    return {
      lens: activeLens,
      filters,
      cameraState,
      nodeSelection: selectedNodeId ? [selectedNodeId] : [],
    };
  }, [
    sigmaRef,
    zoomLevel,
    gs.hiddenEdgeTypes,
    complexityThreshold,
    gs.hotspotDays,
    gs.showDeadCode,
    clusterByCommunity,
    activeLens,
    selectedNodeId,
  ]);

  const setActiveLens = useAppStore((s) => s.setActiveLens);
  const setZoomLevel = useAppStore((s) => s.setZoomLevel);
  const setClusterByCommunity = useAppStore((s) => s.setClusterByCommunity);
  const handleApplyView = useCallback(
    (view: SavedView) => {
      // Lens / zoom — apply if present and known.
      if (view.lens) {
        // Cast through unknown so the LensType union check happens at runtime.
        setActiveLens(view.lens as Parameters<typeof setActiveLens>[0]);
      }
      const filters = (view.filters ?? {}) as {
        zoomLevel?: typeof zoomLevel;
        hiddenEdgeTypes?: string[];
        complexityThreshold?: number;
        hotspotDays?: number;
        showDeadCode?: boolean;
        clusterByCommunity?: boolean;
      };
      if (filters.zoomLevel) setZoomLevel(filters.zoomLevel);
      if (filters.hiddenEdgeTypes) {
        const nextSet = new Set(filters.hiddenEdgeTypes);
        gs.setHiddenEdgeTypes(() => nextSet);
      }
      if (typeof filters.complexityThreshold === "number") {
        setComplexityThreshold(filters.complexityThreshold);
      }
      if (typeof filters.hotspotDays === "number") gs.setHotspotDays(filters.hotspotDays);
      if (typeof filters.showDeadCode === "boolean") gs.setShowDeadCode(filters.showDeadCode);
      if (typeof filters.clusterByCommunity === "boolean") {
        setClusterByCommunity(filters.clusterByCommunity);
      }
      // Selection
      if (view.nodeSelection.length > 0) {
        setSelectedNodeId(view.nodeSelection[0], null);
      }
      // Camera — animate if Sigma is mounted.
      if (view.cameraState && sigmaRef.current) {
        sigmaRef.current
          .getCamera()
          .animate(
            { x: view.cameraState.x, y: view.cameraState.y, ratio: view.cameraState.ratio },
            { duration: 250 },
          );
      }
    },
    [
      gs,
      setActiveLens,
      setZoomLevel,
      setClusterByCommunity,
      setComplexityThreshold,
      setSelectedNodeId,
      sigmaRef,
    ],
  );

  // ── Toolbar ───────────────────────────────────────────────────────
  const handleFit = useCallback(() => fitView(), [fitView]);
  const handleExport = useCallback(() => exportPNG(), [exportPNG]);
  const handleLayoutChange = useCallback((l: string) => { setLayout(l); runLayout(); }, [setLayout, runLayout]);
  const handleToggleEdgeType = useCallback((type: string) => {
    setHiddenEdgeTypes((prev) => { const next = new Set(prev); if (next.has(type)) next.delete(type); else next.add(type); return next; });
  }, [setHiddenEdgeTypes]);

  const toolbarProps = {
    stats: data?.stats,
    layout: gs.layout,
    onLayoutChange: handleLayoutChange,
    onFit: handleFit,
    onExport: handleExport,
    hiddenEdgeTypes: gs.hiddenEdgeTypes,
    onToggleEdgeType: handleToggleEdgeType,
    depthFilter: gs.depthFilter,
    onDepthFilterChange: gs.setDepthFilter,
    complexityThreshold,
    onComplexityChange: setComplexityThreshold,
    hotspotDays: gs.hotspotDays,
    onHotspotDaysChange: gs.setHotspotDays,
    showDeadCode: gs.showDeadCode,
    onToggleDeadCode: () => gs.setShowDeadCode(!gs.showDeadCode),
    // Theme C
    graphMode,
    onGraphModeChange: setGraphMode,
    collectViewState,
    onApplyView: handleApplyView,
  };

  // ── Early returns ─────────────────────────────────────────────────
  if (isLoading) return <GraphLoading {...toolbarProps} />;
  if (error) return <GraphError {...toolbarProps} error={error} />;
  if (data && data.nodes.length === 0) return <GraphEmpty {...toolbarProps} />;

  // ── Main render ──────────────────────────────────────────────────
  return (
    <div className="h-full flex flex-col overflow-hidden">
      <GraphToolbarRow {...toolbarProps} onFlows={() => gs.setFlowsOpen(true)} viewMode={gs.viewMode} onViewModeChange={gs.setViewMode} />

      {gs.viewMode === "treemap" ? (
        <div className="flex-1 relative">
          <Suspense fallback={<GraphLoading {...toolbarProps} />}>
            <TreemapView data={data} isLoading={isLoading} />
          </Suspense>
        </div>
      ) : (
        <div className="flex flex-1 min-h-0 relative overflow-hidden">
          <div className="flex-1 relative bg-bg-0">
            <div ref={containerRef} className="absolute inset-0 cursor-grab focus:outline-none" role="application" aria-label="Interactive code dependency graph" tabIndex={0} />

            {isLayoutRunning && (
              <div className="absolute bottom-4 left-4 z-30 flex items-center gap-2 px-3 py-1.5 rounded-full border border-surface-border bg-bg-2 shadow-lg animate-pulse">
                <div className="w-2 h-2 rounded-full bg-accent" />
                <div className="text-text-2 text-[11px] font-medium">{t("graph.computingLayout")}</div>
              </div>
            )}

            {gs.focusNodeId && (
              <button onClick={() => gs.setFocusNodeId(null)} className="absolute top-16 left-4 z-20 rounded-lg text-xs font-medium px-3 py-1.5 bg-accent text-white border-none cursor-pointer">
                &larr; {t("graph.backToFull")}
              </button>
            )}

            {/* Theme C — Diff mode: snapshot pickers */}
            {graphMode === "diff" && (
              <DiffModeBanner
                from={diffPicker.from}
                to={diffPicker.to}
                onChange={setDiffPicker}
                isLoading={diffQ.isLoading}
                error={diffQ.error as Error | null}
                summary={
                  diffOverlay
                    ? {
                        added: diffOverlay.added.size,
                        removed: diffOverlay.removed.size,
                        modified: diffOverlay.modified.size,
                      }
                    : null
                }
              />
            )}

            {/* Theme C — Path mode: from/to indicator */}
            {graphMode === "path" && (
              <div
                className="absolute top-16 left-4 z-20 flex items-center gap-2 rounded-lg px-3 py-1.5 bg-bg-2 border border-surface-border shadow-sm"
                style={{ maxWidth: "min(70vw, 600px)" }}
              >
                <RouteIcon size={13} className="text-accent flex-shrink-0" />
                <span className="text-[11px] text-text-2 font-medium truncate">
                  {!pathState.from
                    ? "Click a source node…"
                    : !pathState.to
                      ? `From ${shortId(pathState.from)} — pick a target`
                      : pathState.path
                        ? `${shortId(pathState.from)} → ${shortId(pathState.to)} (${pathState.path.length - 1} hops)`
                        : `No path from ${shortId(pathState.from)} to ${shortId(pathState.to)}`}
                </span>
                {(pathState.from || pathState.to) && (
                  <button
                    onClick={() => setPathState({ from: null, to: null, path: null })}
                    className="ml-1 px-1.5 py-0.5 rounded text-[10px] bg-transparent text-text-3 border border-surface-border cursor-pointer hover:bg-surface-hover"
                  >
                    Clear
                  </button>
                )}
              </div>
            )}

            {data?.stats.truncated && !gs.focusNodeId && (
              <div
                className="absolute top-16 left-4 z-20 rounded-lg text-xs px-3 py-2 bg-bg-2 border border-surface-border text-text-2 text-center"
                style={{ right: "calc(260px + 2rem)" }}
              >
                {t("graph.showingTopNodes").replace("{0}", String(data.stats.nodeCount))}
              </div>
            )}

            <NodeHoverCard node={gs.hoveredNode} position={gs.hoverPos} inDegree={gs.hoverDegrees.inDeg} outDegree={gs.hoverDegrees.outDeg}
              onViewSource={() => { if (gs.hoveredNode) { setMode("explorer"); setSelectedNodeId("File:" + gs.hoveredNode.filePath, gs.hoveredNode.name); } }}
              onImpact={() => { if (gs.hoveredNode) { setSelectedNodeId(gs.hoveredNode.id, gs.hoveredNode.name); setMode("explorer"); } }}
            />

            <GraphContextMenu contextMenu={gs.contextMenu} onClose={() => gs.setContextMenu(null)}
              onGoToDefinition={(fp, name) => { setMode("explorer"); setSelectedNodeId("File:" + fp, name); }}
              onFindReferences={(name) => { setSearchQuery(name); setSearchOpen(true); }}
              onViewImpact={(nodeId, name) => { setSelectedNodeId(nodeId, name); setMode("explorer"); }}
              onExpandNeighbors={() => {}}
              onHideNode={(nodeId) => { const g = graphRef.current; if (g?.hasNode(nodeId)) { g.dropNode(nodeId); refresh(); } }}
              onCopyName={async (name) => {
                const ok = await copyTextToClipboard(name);
                if (ok) toast.success(t("graph.copiedToClipboard"));
                else toast.error(t("graph.copyFailed"));
              }}
              onCopyFilePath={async (fp) => {
                const ok = await copyTextToClipboard(fp);
                if (ok) toast.success(t("graph.copiedToClipboard"));
                else toast.error(t("graph.copyFailed"));
              }}
              onAiAction={(action, ctx) => {
                const dispatch = useChatStore.getState().dispatchQuestion;
                switch (action) {
                  case "explain":
                    dispatch("qa", `Explain the symbol \`${ctx.name}\` in \`${ctx.filePath}\`. What is its role, who calls it, and what does it depend on?`, true);
                    break;
                  case "feature_dev":
                    dispatch("feature_dev", `I want to extend or refactor \`${ctx.name}\` (in \`${ctx.filePath}\`). Design the changes.`, true);
                    break;
                  case "code_review":
                    dispatch("code_review", `review: ${ctx.name}`, true);
                    break;
                  case "dead_check":
                    dispatch("qa", `Is \`${ctx.name}\` (in \`${ctx.filePath}\`) actually used? Find all callers and assess whether this is dead code.`, true);
                    break;
                }
                setMode("chat");
              }}
            />

            {(gs.minimapVisible || gs.minimapOpacity !== 0.3) && (
              <Suspense fallback={null}>
                <GraphMinimap visible={gs.minimapVisible} opacity={gs.minimapOpacity} onOpacityChange={gs.setMinimapOpacity} onClose={() => gs.setMinimapVisible(false)} sigmaRef={sigmaRef} graphRef={graphRef} />
              </Suspense>
            )}

            {selectedNodeId && (
              <button 
                onClick={toggleImpactOverlay} 
                className={`absolute z-20 flex items-center gap-1.5 rounded-lg transition-all bottom-[70px] left-4 px-3.5 py-2 backdrop-blur-md text-[11px] font-semibold cursor-pointer border ${
                  gs.impactOverlay 
                    ? "bg-rose text-white border-rose shadow-[0_0_20px_color-mix(in srgb,var(--rose)_30%,transparent)]" 
                    : "bg-bg-2 text-text-2 border-surface-border shadow-sm"
                }`}
              >
                <Zap size={13} />{gs.impactOverlay ? t("graph.clearImpact") : t("graph.impactOverlay")}
              </button>
            )}

            <Suspense fallback={null}>
              <CypherQueryFAB />
            </Suspense>
            {gs.shortcutsOpen && (
              <Suspense fallback={null}>
                <GraphShortcutsOverlay visible={gs.shortcutsOpen} />
              </Suspense>
            )}
            <GraphZoomControls onZoomIn={zoomIn} onZoomOut={zoomOut} onFitView={fitView} legendExpanded={gs.legendExpanded} />
            <Suspense fallback={null}>
              <GraphLegend 
                nodes={data?.nodes ?? []} 
                expanded={gs.legendExpanded} 
                onExpand={() => gs.setLegendExpanded(true)} 
                onCollapse={() => gs.setLegendExpanded(false)} 
                highlightedNodeType={highlightedNodeType}
                onTypeClick={setHighlightedNodeType}
              />
            </Suspense>
            <Suspense fallback={null}>
              <CommunitiesPanel />
            </Suspense>
          </div>
        </div>
      )}

      {gs.flowsOpen && (
        <Suspense fallback={null}>
          <ProcessFlowModal open={gs.flowsOpen} onClose={() => gs.setFlowsOpen(false)} />
        </Suspense>
      )}
    </div>
  );
}

// ─── Theme C — Diff mode UI helper ───────────────────────────────────

function DiffModeBanner({
  from,
  to,
  onChange,
  isLoading,
  error,
  summary,
}: {
  from: string;
  to: string;
  onChange: (next: { from: string; to: string }) => void;
  isLoading: boolean;
  error: Error | null;
  summary: { added: number; removed: number; modified: number } | null;
}) {
  const { data: snapshots = [] } = useQuery({
    queryKey: ["snapshots-for-diff"],
    queryFn: () => commands.snapshotList(),
    staleTime: 30_000,
  });
  return (
    <div
      className="absolute top-16 left-4 z-20 flex items-center gap-2 rounded-lg px-3 py-2 bg-bg-2 border border-surface-border shadow-sm"
      style={{ maxWidth: "min(80vw, 720px)" }}
    >
      <span className="text-[10px] font-bold uppercase tracking-wide text-text-3">Diff</span>
      <select
        value={from}
        onChange={(e) => onChange({ from: e.target.value, to })}
        className="px-2 py-1 text-[11px] rounded border border-surface-border bg-surface text-text-1"
        aria-label="From snapshot"
      >
        <option value="">— from —</option>
        <option value="live">Current (live)</option>
        {snapshots.map((s) => (
          <option key={s.id} value={s.id}>
            {s.label} {s.commitSha ? `(${s.commitSha.slice(0, 7)})` : ""}
          </option>
        ))}
      </select>
      <span className="text-text-3 text-[11px]">→</span>
      <select
        value={to}
        onChange={(e) => onChange({ from, to: e.target.value })}
        className="px-2 py-1 text-[11px] rounded border border-surface-border bg-surface text-text-1"
        aria-label="To snapshot"
      >
        <option value="live">Current (live)</option>
        {snapshots.map((s) => (
          <option key={s.id} value={s.id}>
            {s.label} {s.commitSha ? `(${s.commitSha.slice(0, 7)})` : ""}
          </option>
        ))}
      </select>
      {isLoading && <span className="text-[10px] text-text-3">Computing…</span>}
      {error && <span className="text-[10px] text-rose">{error.message}</span>}
      {summary && (
        <div className="flex items-center gap-2 ml-1">
          <span className="text-[10px] text-green font-semibold">+{summary.added}</span>
          <span className="text-[10px] text-rose font-semibold">−{summary.removed}</span>
          <span className="text-[10px] text-amber font-semibold">~{summary.modified}</span>
        </div>
      )}
    </div>
  );
}

function shortId(id: string): string {
  // "Function:src/foo/bar.ts:doStuff" → "doStuff"
  const colonIdx = id.lastIndexOf(":");
  return colonIdx >= 0 ? id.slice(colonIdx + 1) : id;
}
