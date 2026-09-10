import { useEffect, useRef } from "react";
import {
  filterGraphByDepth,
  filterGraphByCommunities,
} from "../../lib/graph-adapter";
import type { GraphState } from "./useGraphState";
import type Graph from "graphology";
import type { SigmaNodeAttributes, SigmaEdgeAttributes } from "../../lib/graph-adapter";

interface GraphEffectsOptions {
  gs: GraphState;
  selectedNodeId: string | null | undefined;
  searchMatchIds: string[];
  /// Pass the Set directly (NOT a fresh array). A new array reference each
  /// render would re-fire the community-filter effect on every parent render
  /// and re-walk the entire graph. The Set in the store is stable until
  /// `setSelectedFeatures` actually replaces it.
  selectedFeatures: Set<string>;
  egoDepth: number;
  graphRef: React.RefObject<Graph<SigmaNodeAttributes, SigmaEdgeAttributes> | null>;
  focusNode: (id: string) => void;
  refresh: () => void;
  fitView: () => void;
  zoomIn: () => void;
  zoomOut: () => void;
  exportPNG: () => void;
  setSearchOpen: (v: boolean) => void;
  setSelectedNodeId: (id: string | null, name: string | null) => void;
}

export function useGraphEffects({
  gs,
  selectedNodeId,
  searchMatchIds,
  selectedFeatures,
  egoDepth,
  graphRef,
  focusNode,
  refresh,
  fitView,
  zoomIn,
  zoomOut,
  exportPNG,
  setSearchOpen,
  setSelectedNodeId,
}: GraphEffectsOptions) {
  // Destructure stable setters for exhaustive-deps compliance
  const {
    setEgoNodeIds, setEgoDepthMap, depthFilter,
    setContextMenu, setShortcutsOpen, layout, setLayout,
  } = gs;

  // Ego-network BFS
  useEffect(() => {
    const graph = graphRef.current;
    if (!selectedNodeId || !graph || !graph.hasNode(selectedNodeId)) {
      setEgoNodeIds(undefined);
      setEgoDepthMap(undefined);
      return;
    }
    const nodeIds = new Set<string>();
    const depthMap = new Map<string, number>();
    const queue: [string, number][] = [[selectedNodeId, 0]];
    nodeIds.add(selectedNodeId);
    depthMap.set(selectedNodeId, 0);
    while (queue.length > 0) {
      const [cur, d] = queue.shift()!;
      if (d >= egoDepth) continue;
      graph.neighbors(cur).forEach((nb) => {
        if (!nodeIds.has(nb)) {
          nodeIds.add(nb);
          depthMap.set(nb, d + 1);
          queue.push([nb, d + 1]);
        }
      });
    }
    setEgoNodeIds(nodeIds);
    setEgoDepthMap(depthMap);
    refresh();
  }, [selectedNodeId, egoDepth, setEgoNodeIds, setEgoDepthMap, graphRef, refresh]);

  // Depth filter
  useEffect(() => {
    const g = graphRef.current;
    if (!g || g.order === 0) return;
    filterGraphByDepth(g, selectedNodeId ?? null, depthFilter);
    refresh();
  }, [depthFilter, selectedNodeId, refresh, graphRef]);

  // Camera focus on selection
  useEffect(() => {
    if (selectedNodeId) focusNode(selectedNodeId);
    refresh();
  }, [selectedNodeId, focusNode, refresh]);

  // Refresh on search highlights
  useEffect(() => {
    refresh();
  }, [searchMatchIds, refresh]);

  // Community filter
  useEffect(() => {
    const g = graphRef.current;
    if (!g) return;
    filterGraphByCommunities(g, selectedFeatures);
    refresh();
  }, [selectedFeatures, refresh, graphRef]);

  // Keyboard shortcuts
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      const isInput =
        target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.isContentEditable;

      if ((e.ctrlKey || e.metaKey) && e.key === "g") { e.preventDefault(); setSearchOpen(true); }
      if ((e.ctrlKey || e.metaKey) && e.key === "e") { e.preventDefault(); exportPNG(); }
      if ((e.ctrlKey || e.metaKey) && (e.key === "=" || e.key === "+")) { e.preventDefault(); zoomIn(); }
      if ((e.ctrlKey || e.metaKey) && e.key === "-") { e.preventDefault(); zoomOut(); }
      if ((e.ctrlKey || e.metaKey) && e.key === "0") { e.preventDefault(); fitView(); }
      if (e.key === "Escape" && !e.ctrlKey && !e.metaKey && !isInput) {
        setSelectedNodeId(null, null);
        setContextMenu(null);
        setShortcutsOpen(false);
      }
      if (e.key === "?" && !e.ctrlKey && !e.metaKey && !isInput) setShortcutsOpen((p) => !p);
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [exportPNG, zoomIn, zoomOut, fitView, setSearchOpen, setSelectedNodeId, setContextMenu, setShortcutsOpen]);

  // Custom window events.
  // Read `layout` through a ref so this effect registers its listeners exactly
  // once. Re-subscribing on every layout change opens a window between cleanup
  // and re-attach where rapid `code-explorer:cycle-layout` events would be missed.
  const layoutRef = useRef(layout);
  useEffect(() => {
    layoutRef.current = layout;
  }, [layout]);

  useEffect(() => {
    const onFit = () => fitView();
    const onCycle = () => {
      const ls = ["forceatlas2", "grid", "circle", "random"];
      setLayout(ls[(ls.indexOf(layoutRef.current) + 1) % ls.length]);
    };
    window.addEventListener("code-explorer:fit-graph", onFit);
    window.addEventListener("code-explorer:cycle-layout", onCycle);
    return () => {
      window.removeEventListener("code-explorer:fit-graph", onFit);
      window.removeEventListener("code-explorer:cycle-layout", onCycle);
    };
  }, [fitView, setLayout]);
}
