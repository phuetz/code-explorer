/**
 * Architectural Layers visualization in the DetailPanel.
 * Shows nodes grouped by layer_type (Controller → Service → Repository → Database).
 */

import { useMemo } from "react";
import { ArrowDown, Layers } from "lucide-react";
import { useGraphData } from "../../hooks/use-tauri-query";
import { useAppStore } from "../../stores/app-store";
import type { CytoNode } from "../../lib/tauri-commands";

const LAYER_ORDER = [
  "Controller",
  "Service",
  "Repository",
  "Database",
  "External",
];

const LAYER_COLORS: Record<string, string> = {
  Controller: "var(--accent)",
  Service: "var(--green)",
  Repository: "#bb9af7",
  Database: "var(--amber)",
  External: "var(--rose)",
};

const LAYER_DESCRIPTIONS: Record<string, string> = {
  Controller: "Entry points — handles HTTP requests and user actions",
  Service: "Business logic — orchestrates domain operations",
  Repository: "Data access — queries and persists entities",
  Database: "Data model — entities, tables, and schemas",
  External: "External services — APIs, WebServices, third-party calls",
};

export function LayersTab() {
  const setSelectedNodeId = useAppStore((s) => s.setSelectedNodeId);
  const setMode = useAppStore((s) => s.setMode);

  const { data } = useGraphData(
    { zoomLevel: "symbol" as const, maxNodes: 5000 },
    true
  );

  const layers = useMemo(() => {
    if (!data?.nodes) return [];

    const grouped = new Map<string, CytoNode[]>();

    for (const node of data.nodes) {
      const layer = node.layerType;
      if (layer) {
        if (!grouped.has(layer)) grouped.set(layer, []);
        grouped.get(layer)!.push(node);
      }
    }

    // Sort by LAYER_ORDER
    return LAYER_ORDER.filter((l) => grouped.has(l)).map((layer) => ({
      name: layer,
      nodes: grouped.get(layer)!,
      color: LAYER_COLORS[layer] || "#565f89",
      description: LAYER_DESCRIPTIONS[layer] || "",
    }));
  }, [data]);

  const totalLayered = layers.reduce((sum, l) => sum + l.nodes.length, 0);
  const totalNodes = data?.nodes.length || 0;

  if (layers.length === 0) {
    return (
      <div
        style={{
          padding: 20,
          textAlign: "center",
          color: "var(--text-3)",
          fontSize: 12,
        }}
      >
        <Layers
          size={32}
          style={{ margin: "0 auto 8px", opacity: 0.3 }}
        />
        <p>No architectural layers detected.</p>
        <p style={{ fontSize: 11, marginTop: 4 }}>
          Layer detection works best with ASP.NET MVC projects
          (Controller → Service → Repository pattern).
        </p>
      </div>
    );
  }

  function handleNodeClick(nodeId: string) {
    setSelectedNodeId(nodeId);
    setMode("explorer");
  }

  return (
    <div style={{ padding: "12px 0", overflow: "auto" }}>
      {/* Summary */}
      <div
        style={{
          padding: "0 14px 12px",
          fontSize: 11,
          color: "var(--text-2)",
          borderBottom: "1px solid var(--border)",
        }}
      >
        {totalLayered} of {totalNodes} symbols classified into{" "}
        {layers.length} layers
      </div>

      {/* Layer stack */}
      {layers.map((layer, i) => (
        <div key={layer.name}>
          {/* Layer header */}
          <div
            style={{
              padding: "10px 14px 6px",
              display: "flex",
              alignItems: "center",
              gap: 8,
            }}
          >
            <span
              style={{
                width: 10,
                height: 10,
                borderRadius: 3,
                background: layer.color,
                flexShrink: 0,
              }}
            />
            <span
              style={{
                fontSize: 12,
                fontWeight: 600,
                color: "var(--text-0)",
              }}
            >
              {layer.name}
            </span>
            <span
              style={{
                fontSize: 10,
                color: "var(--text-3)",
                marginLeft: "auto",
              }}
            >
              {layer.nodes.length}
            </span>
          </div>

          {/* Description */}
          <div
            style={{
              padding: "0 14px 6px",
              fontSize: 10,
              color: "var(--text-3)",
            }}
          >
            {layer.description}
          </div>

          {/* Nodes list */}
          <div style={{ padding: "0 14px 8px" }}>
            {layer.nodes.slice(0, 15).map((node) => (
              <button
                key={node.id}
                onClick={() => handleNodeClick(node.id)}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  padding: "4px 8px",
                  width: "100%",
                  background: "none",
                  border: "none",
                  borderRadius: 4,
                  color: "var(--text-1)",
                  fontSize: 11,
                  cursor: "pointer",
                  textAlign: "left",
                }}
                onMouseEnter={(e) => {
                  e.currentTarget.style.background = "var(--bg-2)";
                }}
                onMouseLeave={(e) => {
                  e.currentTarget.style.background = "none";
                }}
              >
                <span
                  style={{
                    width: 5,
                    height: 5,
                    borderRadius: "50%",
                    background: layer.color,
                    flexShrink: 0,
                    opacity: 0.7,
                  }}
                />
                <span
                  style={{
                    flex: 1,
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                >
                  {node.name}
                </span>
                <span
                  style={{ fontSize: 10, color: "var(--text-3)" }}
                >
                  {node.label}
                </span>
              </button>
            ))}
            {layer.nodes.length > 15 && (
              <div
                style={{
                  padding: "4px 8px",
                  fontSize: 10,
                  color: "var(--text-3)",
                }}
              >
                +{layer.nodes.length - 15} more
              </div>
            )}
          </div>

          {/* Arrow between layers */}
          {i < layers.length - 1 && (
            <div
              style={{
                display: "flex",
                justifyContent: "center",
                padding: "2px 0",
                color: "var(--text-4)",
              }}
            >
              <ArrowDown size={14} />
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
