import { useEffect } from "react";
import type { AbstractGraph } from "graphology-types";
import louvain from "graphology-communities-louvain";

export function useGraphCommunities(
  graphRef: React.MutableRefObject<AbstractGraph | null>,
  enabled: boolean,
  refresh: () => void,
) {
  useEffect(() => {
    const graph = graphRef.current;
    if (!graph || graph.order === 0) return;

    if (enabled) {
      // Run Louvain community detection
      louvain.assign(graph);

      // Color nodes by community
      const colors = [
        "#4ade80", "#fbbf24", "#fb7185", "#818cf8", "#c084fc",
        "#2dd4bf", "#f472b6", "#fb923c", "#94a3b8", "#60a5fa"
      ];

      graph.forEachNode((node, attrs) => {
        const community = attrs.community as number;
        const color = colors[community % colors.length];
        graph.setNodeAttribute(node, "color", color);
      });
    } else {
      // Reset colors to original
      graph.forEachNode((node, attrs) => {
        if (attrs.originalColor) {
          graph.setNodeAttribute(node, "color", attrs.originalColor);
        }
      });
    }

    refresh();
  }, [graphRef, enabled, refresh]);
}
