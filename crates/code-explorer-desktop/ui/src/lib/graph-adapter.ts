/**
 * Converts Code Explorer graph data (CytoNode/CytoEdge) to a Graphology graph
 * for Sigma.js rendering.
 */
import Graph from "graphology";
import type { CytoNode, CytoEdge } from "./tauri-commands";

// ─── Types ──────────────────────────────────────────────────────────

export interface SigmaNodeAttributes {
  x: number;
  y: number;
  size: number;
  color: string;
  originalSize: number;
  originalColor: string;
  label: string;
  nodeType: string;
  filePath: string;
  startLine?: number;
  endLine?: number;
  parameterCount?: number;
  returnType?: string;
  isTraced?: boolean;
  isDeadCandidate?: boolean;
  complexity?: number;
  hidden?: boolean;
  zIndex?: number;
  mass?: number;
  community?: string;
  // Theme C — graph diff / path modes. Attribute-only markers; actual
  // visual styling lives in the Sigma node reducer when wired up.
  diffMark?: "added" | "removed" | "modified" | null;
  pathDim?: boolean;
  pathOnPath?: boolean;
}

export interface SigmaEdgeAttributes {
  size: number;
  color: string;
  relationType: string;
  type?: string;
  curvature?: number;
  hidden?: boolean;
  zIndex?: number;
}

// ─── Constants ──────────────────────────────────────────────────────

export const NODE_COLORS: Record<string, string> = {
  Project: "#a855f7",
  Package: "#8b5cf6",
  Module: "#7c3aed",
  Folder: "#6366f1",
  File: "#3b82f6",
  Class: "#f59e0b",
  Function: "#10b981",
  Method: "#14b8a6",
  Interface: "#ec4899",
  Struct: "#f97316",
  Enum: "#ef4444",
  Variable: "#64748b",
  Import: "#475569",
  Controller: "#a855f7",
  Service: "#06b6d4",
  Community: "#22c55e",
  Process: "#eab308",
  Constructor: "#14b8a6",
  Property: "#06b6d4",
  Trait: "#22c55e",
  Namespace: "#6366f1",
  Route: "#f97316",
  Tool: "#eab308",
  Type: "#a78bfa",
};

export const NODE_BASE_SIZES: Record<string, number> = {
  Project: 20, Package: 16, Module: 13, Folder: 10,
  File: 6, Class: 8, Interface: 7, Function: 4,
  Method: 3, Variable: 2, Enum: 5, Import: 1.5,
  Controller: 10, Service: 8, Community: 12, Process: 10,
  Constructor: 3, Property: 2, Trait: 7, Type: 3,
  Namespace: 8, Route: 4, Tool: 4, Struct: 6,
};

const NODE_MASS: Record<string, number> = {
  Project: 50, Package: 30, Module: 20, Folder: 15,
  File: 3, Class: 5, Interface: 4, Function: 2,
  Method: 2, Variable: 1, Controller: 8, Service: 6,
};

export const EDGE_STYLES: Record<string, { color: string; size: number }> = {
  CALLS: { color: "#7c3aed", size: 0.8 },
  CALLS_ACTION: { color: "#8b5cf6", size: 0.7 },
  CALLS_SERVICE: { color: "#a78bfa", size: 0.7 },
  IMPORTS: { color: "#1d4ed8", size: 0.6 },
  EXTENDS: { color: "#c2410c", size: 1.0 },
  INHERITS: { color: "#ea580c", size: 1.0 },
  IMPLEMENTS: { color: "#be185d", size: 0.9 },
  CONTAINS: { color: "#2d5a3d", size: 0.4 },
  DEFINES: { color: "#0e7490", size: 0.5 },
  HAS_METHOD: { color: "#065f46", size: 0.3 },
  HAS_PROPERTY: { color: "#064e3b", size: 0.3 },
  HAS_ACTION: { color: "#047857", size: 0.3 },
  DEPENDS_ON: { color: "#4338ca", size: 0.6 },
  RENDERS_VIEW: { color: "#0891b2", size: 0.5 },
};

export const COMMUNITY_COLORS = [
  "#ef4444", "#f97316", "#eab308", "#22c55e", "#06b6d4",
  "#3b82f6", "#8b5cf6", "#d946ef", "#ec4899", "#f43f5e",
  "#14b8a6", "#84cc16",
];

// ─── Helpers ────────────────────────────────────────────────────────

function hashString(s: string): number {
  let hash = 0;
  for (let i = 0; i < s.length; i++) {
    hash = ((hash << 5) - hash + s.charCodeAt(i)) | 0;
  }
  return Math.abs(hash);
}

function getScaledNodeSize(baseSize: number, nodeCount: number): number {
  if (nodeCount > 500) return Math.max(3, baseSize * 0.5);
  if (nodeCount > 200) return Math.max(4, baseSize * 0.65);
  if (nodeCount > 50) return Math.max(4, baseSize * 0.8);
  return Math.max(5, baseSize);
}

export function getCommunityColor(community: string): string {
  return COMMUNITY_COLORS[hashString(community) % COMMUNITY_COLORS.length];
}

// ─── Main Conversion ────────────────────────────────────────────────

export function buildGraphologyGraph(
  nodes: CytoNode[],
  edges: CytoEdge[],
  hiddenEdgeTypes: Set<string>,
  complexityThreshold: number = 0,
): Graph<SigmaNodeAttributes, SigmaEdgeAttributes> {
  const graph = new Graph<SigmaNodeAttributes, SigmaEdgeAttributes>();
  
  // Filter nodes by complexity if applicable
  const filteredNodes = nodes.filter(node => {
    if (complexityThreshold > 0 && node.complexity !== undefined) {
      return node.complexity >= complexityThreshold;
    }
    return true;
  });
  
  const nodeCount = filteredNodes.length;

  // Golden angle distribution for initial positioning
  const goldenAngle = Math.PI * (3 - Math.sqrt(5));
  const spread = Math.sqrt(nodeCount) * 40;

  filteredNodes.forEach((node, i) => {
    const baseSize = NODE_BASE_SIZES[node.label] || 4;
    const size = getScaledNodeSize(baseSize, nodeCount);
    const mass = NODE_MASS[node.label] || 2;

    // Use community color for symbol nodes, type color for structural
    const isSymbol = ["Function", "Method", "Class", "Interface", "Struct", "Enum", "Variable", "Constructor", "Property"].includes(node.label);
    const color = (isSymbol && node.community)
      ? getCommunityColor(node.community)
      : NODE_COLORS[node.label] || "#64748b";

    // Golden angle spiral positioning
    const angle = i * goldenAngle;
    const radius = spread * Math.sqrt(i + 1) / Math.sqrt(nodeCount);

    graph.addNode(node.id, {
      x: Math.cos(angle) * radius,
      y: Math.sin(angle) * radius,
      size,
      color,
      originalSize: size,
      originalColor: color,
      label: node.name,
      nodeType: node.label,
      filePath: node.filePath,
      startLine: node.startLine,
      endLine: node.endLine,
      parameterCount: node.parameterCount,
      returnType: node.returnType,
      isTraced: node.isTraced,
      isDeadCandidate: node.isDeadCandidate,
      complexity: node.complexity,
      mass,
      community: node.community,
    });
  });

  // Add edges (filtered by hidden types)
  edges.forEach((edge) => {
    if (hiddenEdgeTypes.has(edge.relType)) return;
    if (!graph.hasNode(edge.source) || !graph.hasNode(edge.target)) return;
    // Avoid duplicate edges
    const edgeKey = `${edge.source}->${edge.target}:${edge.relType}`;
    if (graph.hasEdge(edgeKey)) return;

    const style = EDGE_STYLES[edge.relType] || { color: "#565f89", size: 0.5 };

    // Derive curvature deterministically from the edge key so the same data
    // always produces the same visual layout. Math.random() here meant every
    // graph rebuild (e.g. lens toggle, panel resize) jiggled every edge.
    const h = hashString(edgeKey);
    graph.addEdgeWithKey(edgeKey, edge.source, edge.target, {
      size: style.size,
      color: style.color,
      relationType: edge.relType,
      type: "curved",
      curvature: 0.12 + ((h % 1000) / 1000) * 0.08,
    });
  });

  return graph;
}

// ─── Depth Filter ───────────────────────────────────────────────────

export function filterGraphByDepth(
  graph: Graph<SigmaNodeAttributes, SigmaEdgeAttributes>,
  selectedNodeId: string | null,
  maxHops: number | null,
): void {
  // Reset all visibility
  graph.forEachNode((nodeId) => {
    graph.setNodeAttribute(nodeId, "hidden", false);
  });
  graph.forEachEdge((edgeId) => {
    graph.setEdgeAttribute(edgeId, "hidden", false);
  });

  if (!maxHops || !selectedNodeId || !graph.hasNode(selectedNodeId)) return;

  // BFS to find nodes within N hops
  const visited = new Set<string>([selectedNodeId]);
  const queue: { id: string; depth: number }[] = [{ id: selectedNodeId, depth: 0 }];

  while (queue.length > 0) {
    const { id, depth } = queue.shift()!;
    if (depth >= maxHops) continue;

    graph.forEachNeighbor(id, (neighborId) => {
      if (!visited.has(neighborId)) {
        visited.add(neighborId);
        queue.push({ id: neighborId, depth: depth + 1 });
      }
    });
  }

  // Hide nodes/edges not in range
  graph.forEachNode((nodeId) => {
    if (!visited.has(nodeId)) {
      graph.setNodeAttribute(nodeId, "hidden", true);
    }
  });

  graph.forEachEdge((edgeId, _attrs, source, target) => {
    if (!visited.has(source) || !visited.has(target)) {
      graph.setEdgeAttribute(edgeId, "hidden", true);
    }
  });
}

// ─── Color Utilities ────────────────────────────────────────────────

// ─── Community Filter ──────────────────────────────────────────────

export function filterGraphByCommunities(
  graph: Graph<SigmaNodeAttributes, SigmaEdgeAttributes>,
  selectedCommunities: Set<string>,
): void {
  if (selectedCommunities.size === 0) {
    // Show all
    graph.forEachNode((nodeId) => {
      graph.setNodeAttribute(nodeId, "hidden", false);
    });
    graph.forEachEdge((edgeId) => {
      graph.setEdgeAttribute(edgeId, "hidden", false);
    });
    return;
  }

  // Hide nodes not in selected communities
  const visibleNodes = new Set<string>();
  graph.forEachNode((nodeId, attrs) => {
    const inCommunity = attrs.community && selectedCommunities.has(attrs.community);
    graph.setNodeAttribute(nodeId, "hidden", !inCommunity);
    if (inCommunity) visibleNodes.add(nodeId);
  });

  // Hide edges where either endpoint is hidden
  graph.forEachEdge((edgeId, _attrs, source, target) => {
    graph.setEdgeAttribute(edgeId, "hidden", !visibleNodes.has(source) || !visibleNodes.has(target));
  });
}

// ─── Color Utilities ────────────────────────────────────────────────

export function dimColor(hex: string, amount: number): string {
  if (!hex || !hex.startsWith("#") || hex.length < 7) return hex || "#565f89";
  const r = parseInt(hex.slice(1, 3), 16);
  const g = parseInt(hex.slice(3, 5), 16);
  const b = parseInt(hex.slice(5, 7), 16);
  if (isNaN(r) || isNaN(g) || isNaN(b)) return hex;
  const bg = { r: 6, g: 6, b: 10 }; // #06060a
  const nr = Math.round(bg.r + (r - bg.r) * amount);
  const ng = Math.round(bg.g + (g - bg.g) * amount);
  const nb = Math.round(bg.b + (b - bg.b) * amount);
  return `#${nr.toString(16).padStart(2, "0")}${ng.toString(16).padStart(2, "0")}${nb.toString(16).padStart(2, "0")}`;
}
