import { useMemo, useState, useRef, useCallback } from "react";
import { hierarchy, treemap, treemapSquarify, type HierarchyRectangularNode } from "d3-hierarchy";
import { SkeletonBlock } from "../shared/motion";
import { useAppStore } from "../../stores/app-store";
import { useI18n } from "../../hooks/use-i18n";
import type { GraphPayload, CytoNode } from "../../lib/tauri-commands";

// ─── Color mapping (matches GraphExplorer LABEL_COLORS) ──────────────

const LABEL_COLORS: Record<string, string> = {
  Function: "var(--accent)",
  Class: "#bb9af7",
  Method: "#7dcfff",
  Interface: "var(--amber)",
  Struct: "#ff9e64",
  Trait: "var(--green)",
  Enum: "var(--rose)",
  File: "#565f89",
  Folder: "#414868",
  Module: "#565f89",
  Package: "#414868",
  Variable: "#73daca",
  Type: "#c0caf5",
  Import: "#414868",
  Community: "var(--green)",
  Process: "var(--amber)",
  Constructor: "#7dcfff",
  Property: "#73daca",
  Route: "#ff9e64",
  Tool: "var(--amber)",
  Namespace: "#414868",
};

// ─── Types ───────────────────────────────────────────────────────────

interface TreeNode {
  name: string;
  path: string;
  children?: TreeNode[];
  value?: number;
  dominantLabel?: string;
  nodeCount?: number;
}

interface TreemapViewProps {
  data: GraphPayload | undefined;
  isLoading: boolean;
}

// ─── Build hierarchy from flat node list ─────────────────────────────

function buildTree(nodes: CytoNode[]): TreeNode {
  const root: TreeNode = { name: "root", path: "", children: [] };

  // Group nodes by filePath
  const fileMap = new Map<string, CytoNode[]>();
  for (const node of nodes) {
    const fp = node.filePath || "(unknown)";
    let list = fileMap.get(fp);
    if (!list) {
      list = [];
      fileMap.set(fp, list);
    }
    list.push(node);
  }

  // Build a nested folder tree
  const dirMap = new Map<string, TreeNode>();
  dirMap.set("", root);

  const ensureDir = (dirPath: string): TreeNode => {
    if (dirMap.has(dirPath)) return dirMap.get(dirPath)!;
    const parts = dirPath.split("/");
    const parentPath = parts.slice(0, -1).join("/");
    const parent = ensureDir(parentPath);
    const dirNode: TreeNode = {
      name: parts[parts.length - 1],
      path: dirPath,
      children: [],
    };
    if (!parent.children) parent.children = [];
    parent.children.push(dirNode);
    dirMap.set(dirPath, dirNode);
    return dirNode;
  };

  for (const [filePath, fileNodes] of fileMap) {
    // Normalize separators
    const normalized = filePath.replace(/\\/g, "/");
    const parts = normalized.split("/");
    const fileName = parts[parts.length - 1];
    const dirPath = parts.slice(0, -1).join("/");

    const parent = ensureDir(dirPath);

    // Determine dominant label (most common nodeLabel in this file)
    const labelCounts = new Map<string, number>();
    for (const n of fileNodes) {
      const lbl = n.label || "File";
      labelCounts.set(lbl, (labelCounts.get(lbl) || 0) + 1);
    }
    let dominantLabel = "File";
    let maxCount = 0;
    for (const [lbl, cnt] of labelCounts) {
      if (cnt > maxCount) {
        maxCount = cnt;
        dominantLabel = lbl;
      }
    }

    const leaf: TreeNode = {
      name: fileName,
      path: normalized,
      value: fileNodes.length,
      dominantLabel,
      nodeCount: fileNodes.length,
    };
    if (!parent.children) parent.children = [];
    parent.children.push(leaf);
  }

  return root;
}

// ─── Component ───────────────────────────────────────────────────────

export function TreemapView({ data, isLoading }: TreemapViewProps) {
  const { t } = useI18n();
  const containerRef = useRef<HTMLDivElement>(null);
  const setSelectedNodeId = useAppStore((s) => s.setSelectedNodeId);
  const setMode = useAppStore((s) => s.setMode);

  const [tooltip, setTooltip] = useState<{
    x: number;
    y: number;
    name: string;
    nodeCount: number;
    dominantLabel: string;
  } | null>(null);

  const [hoveredPath, setHoveredPath] = useState<string | null>(null);

  // Container size tracking
  const [dimensions, setDimensions] = useState({ width: 800, height: 600 });
  const resizeRef = useCallback(
    (el: HTMLDivElement | null) => {
      if (!el) return;
      (containerRef as React.MutableRefObject<HTMLDivElement | null>).current = el;
      const ro = new ResizeObserver((entries) => {
        const entry = entries[0];
        if (entry) {
          setDimensions({
            width: entry.contentRect.width,
            height: entry.contentRect.height,
          });
        }
      });
      ro.observe(el);
      return () => ro.disconnect();
    },
    [],
  );

  // Compute treemap layout
  const leaves = useMemo(() => {
    if (!data || data.nodes.length === 0) return [];

    const tree = buildTree(data.nodes);
    const root = hierarchy(tree)
      .sum((d) => d.value ?? 0)
      .sort((a, b) => (b.value ?? 0) - (a.value ?? 0));

    const tm = treemap<TreeNode>()
      .size([dimensions.width, dimensions.height])
      .tile(treemapSquarify)
      .padding(2)
      .paddingOuter(4)
      .round(true);

    tm(root);
    return root.leaves() as HierarchyRectangularNode<TreeNode>[];
  }, [data, dimensions.width, dimensions.height]);

  // ─── Loading ─────────────────────────────────────────────────────

  if (isLoading) {
    return (
      <div
        style={{
          width: "100%",
          height: "100%",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          background: "var(--bg-1)",
        }}
      >
        <div style={{ display: "flex", flexDirection: "column", gap: 12, alignItems: "center" }}>
          <SkeletonBlock width="320px" height="200px" />
          <SkeletonBlock width="200px" height="14px" rounded="6px" />
        </div>
      </div>
    );
  }

  // ─── Empty ───────────────────────────────────────────────────────

  if (!data || data.nodes.length === 0) {
    return (
      <div
        style={{
          width: "100%",
          height: "100%",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          background: "var(--bg-1)",
          color: "var(--text-3)",
          fontSize: 14,
        }}
      >
{t("graph.noTreemapData")}
      </div>
    );
  }

  // ─── Render ──────────────────────────────────────────────────────

  return (
    <div
      ref={resizeRef}
      style={{
        position: "relative",
        width: "100%",
        height: "100%",
        overflow: "hidden",
        background: "var(--bg-1)",
      }}
    >
      {leaves.map((leaf) => {
        const x0 = leaf.x0 ?? 0;
        const y0 = leaf.y0 ?? 0;
        const x1 = leaf.x1 ?? 0;
        const y1 = leaf.y1 ?? 0;
        const w = x1 - x0;
        const h = y1 - y0;
        if (w < 1 || h < 1) return null;

        const d = leaf.data;
        const color = LABEL_COLORS[d.dominantLabel ?? "File"] || "#565f89";
        const isHovered = hoveredPath === d.path;
        const showLabel = w > 60 && h > 20;

        return (
          <div
            key={d.path}
            style={{
              position: "absolute",
              left: x0,
              top: y0,
              width: w,
              height: h,
              backgroundColor: color,
              opacity: isHovered ? 1 : 0.75,
              borderRadius: 3,
              cursor: "pointer",
              overflow: "hidden",
              transition: "opacity 0.12s ease, box-shadow 0.12s ease",
              boxShadow: isHovered
                ? `0 0 0 2px var(--text-0), inset 0 0 0 1px var(--surface-border)`
                : "inset 0 0 0 1px var(--surface-border)",
              display: "flex",
              alignItems: "flex-end",
              padding: showLabel ? 4 : 0,
            }}
            role="button"
            tabIndex={0}
            aria-label={`${d.name} — ${d.dominantLabel}, ${d.nodeCount} nodes`}
            onClick={() => {
              setMode("explorer");
              setSelectedNodeId("File:" + d.path, d.name);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                setMode("explorer");
                setSelectedNodeId("File:" + d.path, d.name);
              }
            }}
            onMouseEnter={(e) => {
              setHoveredPath(d.path);
              const rect = containerRef.current?.getBoundingClientRect();
              if (rect) {
                setTooltip({
                  x: e.clientX - rect.left,
                  y: e.clientY - rect.top,
                  name: d.path,
                  nodeCount: d.nodeCount ?? 0,
                  dominantLabel: d.dominantLabel ?? "File",
                });
              }
            }}
            onMouseMove={(e) => {
              const rect = containerRef.current?.getBoundingClientRect();
              if (rect) {
                setTooltip((prev) =>
                  prev
                    ? {
                        ...prev,
                        x: e.clientX - rect.left,
                        y: e.clientY - rect.top,
                      }
                    : null,
                );
              }
            }}
            onMouseLeave={() => {
              setHoveredPath(null);
              setTooltip(null);
            }}
          >
            {showLabel && (
              <span
                style={{
                  fontSize: Math.min(11, Math.max(9, w / 12)),
                  lineHeight: 1.2,
                  color: "#fff",
                  fontWeight: 500,
                  textShadow: "0 1px 3px rgba(0,0,0,0.5)",
                  maxWidth: "100%",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                  pointerEvents: "none",
                }}
              >
                {d.name}
              </span>
            )}
          </div>
        );
      })}

      {/* Tooltip */}
      {tooltip && (
        <div
          style={{
            position: "absolute",
            left: tooltip.x + 12,
            top: tooltip.y - 8,
            transform: "translateY(-100%)",
            pointerEvents: "none",
            zIndex: 50,
            backgroundColor: "var(--surface)",
            border: "1px solid var(--surface-border)",
            borderRadius: "var(--radius-md)",
            backdropFilter: "blur(8px)",
            boxShadow: "var(--shadow-lg)",
            padding: "8px 12px",
            maxWidth: 280,
          }}
        >
          <div
            style={{
              fontSize: 12,
              fontWeight: 600,
              color: "var(--text-1)",
              marginBottom: 4,
              wordBreak: "break-all",
            }}
          >
            {tooltip.name}
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <span
              style={{
                display: "inline-block",
                width: 8,
                height: 8,
                borderRadius: "50%",
                backgroundColor:
                  LABEL_COLORS[tooltip.dominantLabel] || "#565f89",
                flexShrink: 0,
              }}
            />
            <span style={{ fontSize: 11, color: "var(--text-3)" }}>
              {tooltip.dominantLabel}
            </span>
            <span style={{ fontSize: 11, color: "var(--text-4)", marginLeft: 4 }}>
              {tooltip.nodeCount} node{tooltip.nodeCount !== 1 ? "s" : ""}
            </span>
          </div>
        </div>
      )}
    </div>
  );
}
