/**
 * useSigma — wraps all Sigma.js/graphology logic for the graph explorer.
 */
import { useRef, useEffect, useCallback, useState } from "react";
import type Sigma from "sigma";
import type Graph from "graphology";
import type {
  SigmaNodeAttributes,
  SigmaEdgeAttributes,
} from "../lib/graph-adapter";
import { dimColor } from "../lib/graph-adapter";
import { useSigmaLayout } from "./use-sigma-layout";

type SigmaRuntime = {
  Sigma: typeof import("sigma").default;
  Graph: typeof import("graphology").default;
  EdgeCurveProgram: typeof import("@sigma/edge-curve").default;
};

let sigmaRuntimePromise: Promise<SigmaRuntime> | null = null;

function loadSigmaRuntime(): Promise<SigmaRuntime> {
  if (!sigmaRuntimePromise) {
    sigmaRuntimePromise = Promise.all([
      import("sigma"),
      import("graphology"),
      import("@sigma/edge-curve"),
    ]).then(([sigma, graphology, edgeCurve]) => ({
      Sigma: sigma.default,
      Graph: graphology.default,
      EdgeCurveProgram: edgeCurve.default,
    }));
  }
  return sigmaRuntimePromise;
}

export interface UseSigmaOptions {
  onNodeClick?: (nodeId: string | null) => void;
  onNodeHover?: (nodeId: string | null) => void;
  onNodeRightClick?: (nodeId: string, x: number, y: number) => void;
  onNodeDoubleClick?: (nodeId: string) => void;
  selectedNodeId?: string | null;
  hoveredNodeId?: string | null;
  highlightedNodeIds?: Set<string>;
  impactNodeIds?: Map<string, number>;
  egoNodeIds?: Set<string>;
  egoDepthMap?: Map<string, number>;
}

export function useSigma(options: UseSigmaOptions) {
  const containerRef = useRef<HTMLDivElement>(null);
  const sigmaRef = useRef<Sigma | null>(null);
  const graphRef = useRef<Graph<SigmaNodeAttributes, SigmaEdgeAttributes> | null>(null);
  const [isLayoutRunning, setIsLayoutRunning] = useState(false);

  // ── Layout Hook ──
  const { runLayout, stopLayout } = useSigmaLayout(graphRef, sigmaRef, setIsLayoutRunning);

  const selectedRef = useRef(options.selectedNodeId);
  const hoveredRef = useRef(options.hoveredNodeId);
  const highlightedRef = useRef(options.highlightedNodeIds);
  const impactRef = useRef(options.impactNodeIds);
  const egoNodeIdsRef = useRef<Set<string> | undefined>(undefined);
  const egoDepthMapRef = useRef<Map<string, number> | undefined>(undefined);

  useEffect(() => {
    selectedRef.current = options.selectedNodeId;
    sigmaRef.current?.refresh();
  }, [options.selectedNodeId]);
  useEffect(() => {
    hoveredRef.current = options.hoveredNodeId;
    sigmaRef.current?.refresh();
  }, [options.hoveredNodeId]);
  useEffect(() => {
    highlightedRef.current = options.highlightedNodeIds;
    sigmaRef.current?.refresh();
  }, [options.highlightedNodeIds]);
  useEffect(() => {
    impactRef.current = options.impactNodeIds;
    sigmaRef.current?.refresh();
  }, [options.impactNodeIds]);
  useEffect(() => {
    egoNodeIdsRef.current = options.egoNodeIds;
    sigmaRef.current?.refresh();
  }, [options.egoNodeIds]);
  useEffect(() => {
    egoDepthMapRef.current = options.egoDepthMap;
    sigmaRef.current?.refresh();
  }, [options.egoDepthMap]);

  const onNodeClickRef = useRef(options.onNodeClick);
  const onNodeHoverRef = useRef(options.onNodeHover);
  const onNodeRightClickRef = useRef(options.onNodeRightClick);
  const onNodeDoubleClickRef = useRef(options.onNodeDoubleClick);
  useEffect(() => { onNodeClickRef.current = options.onNodeClick; }, [options.onNodeClick]);
  useEffect(() => { onNodeHoverRef.current = options.onNodeHover; }, [options.onNodeHover]);
  useEffect(() => { onNodeRightClickRef.current = options.onNodeRightClick; }, [options.onNodeRightClick]);
  useEffect(() => { onNodeDoubleClickRef.current = options.onNodeDoubleClick; }, [options.onNodeDoubleClick]);

  useEffect(() => {
    let cancelled = false;
    let themeObserver: MutationObserver | null = null;
    let clickDebounceTimer: ReturnType<typeof setTimeout> | null = null;

    void (async () => {
      if (!containerRef.current) return;
      const runtime = await loadSigmaRuntime();
      if (cancelled || !containerRef.current) return;

      const graph = graphRef.current ?? new runtime.Graph<SigmaNodeAttributes, SigmaEdgeAttributes>();
      graphRef.current = graph;

      const cssVars = getComputedStyle(document.documentElement);
      const labelColor = cssVars.getPropertyValue("--text-1").trim() || "#e4e4ed";
      const defaultNodeColor = cssVars.getPropertyValue("--text-4").trim() || "#64748b";
      const defaultEdgeColor = cssVars.getPropertyValue("--surface-border").trim() || "#2a2a3a";

      const sigma = new runtime.Sigma(graph, containerRef.current, {
        // Without this, Sigma throws "Container has no width" during the
        // brief frame where the right/left resizable panels animate from
        // 0 → target size, leaving the center panel at width=0. With the
        // flag on, Sigma stays dormant until the container gets real
        // dimensions back on the next frame.
        allowInvalidContainer: true,
        renderLabels: true,
        labelFont: "JetBrains Mono, Fira Code, monospace",
        labelSize: 11,
        labelWeight: "500",
        labelColor: { color: labelColor },
        labelRenderedSizeThreshold: 6,
        labelDensity: 0.15,
        labelGridCellSize: 60,
        defaultNodeColor,
        defaultEdgeColor,
        defaultEdgeType: "curved",
        hideEdgesOnMove: true,
        zIndex: true,
        minCameraRatio: 0.01,
        maxCameraRatio: 20,
        edgeProgramClasses: { curved: runtime.EdgeCurveProgram },

        defaultDrawNodeHover: (context, data) => {
          const label = data.label;
          if (!label) return;
          const fontSize = 11;
          const font = `500 ${fontSize}px JetBrains Mono, monospace`;
          context.save();
          try {
            context.font = font;
            const textWidth = context.measureText(label).width;
            const nodeSize = data.size;
            const x = data.x;
            const y = data.y - nodeSize - 12;
            const padX = 8;
            const padY = 5;
            const w = textWidth + padX * 2;
            const h = fontSize + padY * 2;
            context.fillStyle = "#0d1520";
            context.beginPath();
            context.roundRect(x - w / 2, y - h / 2, w, h, 4);
            context.fill();
            context.strokeStyle = data.color || "#06b6d4";
            context.lineWidth = 1.5;
            context.stroke();
            context.fillStyle = "#e4e4ed";
            context.textAlign = "center";
            context.textBaseline = "middle";
            context.fillText(label, x, y);
            context.beginPath();
            context.arc(data.x, data.y, nodeSize + 3, 0, Math.PI * 2);
            context.strokeStyle = data.color || "#06b6d4";
            context.lineWidth = 1.5;
            context.globalAlpha = 0.4;
            context.stroke();
          } finally {
            context.restore();
          }
        },

        nodeReducer: (node, data) => {
          const res = { ...data };
          const selected = selectedRef.current;
          const hovered = hoveredRef.current;
          const highlighted = highlightedRef.current;
          const impact = impactRef.current;
          const isStructural = ["Project", "Package", "Module", "Folder", "File", "Class", "Interface", "Struct"].includes(data.nodeType);
          if (isStructural) res.label = data.label; 
          const egoNodes = egoNodeIdsRef.current;
          const egoDepths = egoDepthMapRef.current;
          if (egoNodes && egoNodes.size > 0) {
            if (egoNodes.has(node)) {
              const d = egoDepths?.get(node) ?? 0;
              if (d <= 1) res.zIndex = 10 - d;
              else if (d === 2) { res.color = dimColor(data.color || "#64748b", 0.8); res.zIndex = 3; }
              else { res.color = dimColor(data.color || "#64748b", 0.6); res.zIndex = 2; }
            } else {
              res.color = dimColor(data.color || "#64748b", 0.10);
              res.size = (data.size || 6) * 0.4;
              res.label = "";
              res.zIndex = 0;
            }
            return res;
          }
          if (impact && impact.size > 0) {
            const depth = impact.get(node);
            if (depth !== undefined) {
              if (depth === 0) { res.color = "#ef4444"; res.size = (data.size || 6) * 1.8; res.zIndex = 10; }
              else if (depth === 1) { res.color = "#ff9e64"; res.size = (data.size || 6) * 1.4; res.zIndex = 5; }
              else if (depth === 2) { res.color = "#e0af68"; res.size = (data.size || 6) * 1.2; res.zIndex = 3; }
              else { res.color = "#9ece6a"; res.size = (data.size || 6) * 1.1; res.zIndex = 2; }
            } else {
              res.color = dimColor(data.color || "#64748b", 0.15);
              res.size = (data.size || 6) * 0.5;
            }
            return res;
          }
          if (highlighted && highlighted.size > 0 && !selected) {
            if (highlighted.has(node)) {
              res.color = "#06b6d4";
              res.size = (data.size || 6) * 1.5;
              res.zIndex = 5;
              res.label = data.label;
            } else {
              res.color = dimColor(data.color || "#64748b", 0.2);
              res.size = (data.size || 6) * 0.6;
            }
            return res;
          }
          if (selected) {
            const g = graphRef.current;
            if (node === selected) { res.size = (data.size || 6) * 1.8; res.zIndex = 10; res.label = data.label; }
            else if (g && g.hasNode(selected) && g.areNeighbors(node, selected)) { res.size = (data.size || 6) * 1.3; res.zIndex = 5; res.label = data.label; }
            else { res.color = dimColor(data.color || "#64748b", 0.4); res.size = (data.size || 6) * 0.5; }
          } else if (hovered) {
            const g = graphRef.current;
            if (node === hovered) { res.size = (data.size || 6) * 1.5; res.zIndex = 10; res.label = data.label; }
            else if (g && g.hasNode(hovered) && g.areNeighbors(node, hovered)) { res.size = (data.size || 6) * 1.2; res.zIndex = 5; res.label = data.label; }
            else { res.color = dimColor(data.color || "#64748b", 0.5); res.size = (data.size || 6) * 0.8; }
          }
          return res;
        },

        edgeReducer: (edge, data) => {
          const res = { ...data };
          const selected = selectedRef.current;
          const hovered = hoveredRef.current;
          const g = graphRef.current;
          const egoNodes2 = egoNodeIdsRef.current;
          if (egoNodes2 && egoNodes2.size > 0 && g) {
            const [source, target] = g.extremities(edge);
            const srcIn = egoNodes2.has(source);
            const tgtIn = egoNodes2.has(target);
            if (srcIn && tgtIn) { res.size = Math.max(1, (data.size || 0.5) * 2); res.zIndex = 5; }
            else if (srcIn || tgtIn) { res.color = dimColor(data.color || "#2a2a3a", 0.5); res.size = (data.size || 0.5) * 0.8; }
            else { res.color = dimColor(data.color || "#2a2a3a", 0.2); res.size = 0.1; }
            return res;
          }
          if (selected && g) {
            const [source, target] = g.extremities(edge);
            if (source === selected || target === selected) { res.size = Math.max(1.5, (data.size || 0.5) * 3); res.zIndex = 5; }
            else if (g.hasNode(selected) && (g.areNeighbors(source, selected) || g.areNeighbors(target, selected))) { res.size = (data.size || 0.5) * 1.5; res.color = dimColor(data.color || "#2a2a3a", 0.6); }
            else { res.color = dimColor(data.color || "#2a2a3a", 0.2); res.size = 0.2; }
          } else if (hovered && g) {
            const [source, target] = g.extremities(edge);
            if (source === hovered || target === hovered) { res.size = Math.max(1, (data.size || 0.5) * 2); res.zIndex = 5; }
            else { res.color = dimColor(data.color || "#2a2a3a", 0.4); res.size = (data.size || 0.5) * 0.5; }
          }
          return res;
        },
      });

      sigmaRef.current = sigma;
      themeObserver = new MutationObserver(() => {
        const s = getComputedStyle(document.documentElement);
        sigma.setSetting("labelColor", { color: s.getPropertyValue("--text-1").trim() || "#e4e4ed" });
        sigma.setSetting("defaultNodeColor", s.getPropertyValue("--text-4").trim() || "#64748b");
        sigma.setSetting("defaultEdgeColor", s.getPropertyValue("--surface-border").trim() || "#2a2a3a");
        sigma.refresh();
      });
      themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });

      sigma.on("clickNode", ({ node }) => {
        if (clickDebounceTimer) clearTimeout(clickDebounceTimer);
        clickDebounceTimer = setTimeout(() => { onNodeClickRef.current?.(node); }, 150);
      });
      sigma.on("clickStage", () => { onNodeClickRef.current?.(null); });
      sigma.on("doubleClickNode", ({ node }) => {
        if (clickDebounceTimer) { clearTimeout(clickDebounceTimer); clickDebounceTimer = null; }
        onNodeDoubleClickRef.current?.(node);
      });
      sigma.on("enterNode", ({ node }) => { onNodeHoverRef.current?.(node); if (containerRef.current) containerRef.current.style.cursor = "pointer"; });
      sigma.on("leaveNode", () => { onNodeHoverRef.current?.(null); if (containerRef.current) containerRef.current.style.cursor = "grab"; });
      sigma.on("rightClickNode", ({ node, event }) => { event.original.preventDefault(); onNodeRightClickRef.current?.(node, event.x, event.y); });
      sigma.refresh();
    })();

    return () => {
      cancelled = true;
      themeObserver?.disconnect();
      if (clickDebounceTimer) clearTimeout(clickDebounceTimer);
      stopLayout();
      sigmaRef.current?.kill();
      sigmaRef.current = null;
    };
  }, [stopLayout]);

  const setGraph = useCallback((newGraph: Graph<SigmaNodeAttributes, SigmaEdgeAttributes>) => {
    graphRef.current = newGraph;
    if (sigmaRef.current) { sigmaRef.current.setGraph(newGraph); sigmaRef.current.refresh(); }
  }, []);

  const focusNode = useCallback((nodeId: string) => {
    const sigma = sigmaRef.current;
    const graph = graphRef.current;
    if (!sigma || !graph || !graph.hasNode(nodeId)) return;
    const attrs = graph.getNodeAttributes(nodeId);
    sigma.getCamera().animate({ x: attrs.x, y: attrs.y, ratio: 0.15 }, { duration: 200 });
  }, []);

  const fitView = useCallback(() => { sigmaRef.current?.getCamera().animatedReset({ duration: 300 }); }, []);
  const zoomIn = useCallback(() => { const cam = sigmaRef.current?.getCamera(); if (cam) cam.animate({ ratio: cam.getState().ratio / 1.5 }, { duration: 200 }); }, []);
  const zoomOut = useCallback(() => { const cam = sigmaRef.current?.getCamera(); if (cam) cam.animate({ ratio: cam.getState().ratio * 1.5 }, { duration: 200 }); }, []);

  const exportPNG = useCallback(() => {
    const sigma = sigmaRef.current;
    if (!sigma) return;
    const canvases = sigma.getCanvases();
    const layers = Object.values(canvases);
    if (layers.length === 0) return;
    const w = layers[0].width;
    const h = layers[0].height;
    const merged = document.createElement("canvas");
    merged.width = w;
    merged.height = h;
    const ctx = merged.getContext("2d");
    if (!ctx) return;
    ctx.fillStyle = "#06060a";
    ctx.fillRect(0, 0, w, h);
    for (const layer of layers) ctx.drawImage(layer, 0, 0);
    merged.toBlob((blob) => {
      if (!blob) return;
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = "code-explorer-graph.png";
      a.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    });
  }, []);

  const refresh = useCallback(() => { sigmaRef.current?.refresh(); }, []);

  return { containerRef, sigmaRef, graphRef, isLayoutRunning, setGraph, runLayout, stopLayout, focusNode, fitView, zoomIn, zoomOut, exportPNG, refresh };
}
