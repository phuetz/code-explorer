import { useEffect } from "react";
import type { AbstractGraph } from "graphology-types";
import type { GitHotspot } from "../lib/tauri-commands";
import { useAppStore } from "../stores/app-store";
import type { AppState } from "../stores/app-store";

interface UseGraphLensesProps {
  activeLens: AppState["activeLens"];
  hotspotsData?: GitHotspot[];
  graphRef: React.MutableRefObject<AbstractGraph | null>;
  refresh: () => void;
  showDeadCode: boolean;
}

export function useGraphLenses({
  activeLens,
  hotspotsData,
  graphRef,
  refresh,
  showDeadCode,
}: UseGraphLensesProps) {
  const riskThreshold = useAppStore((s) => s.riskThreshold);

  // ── Hotspots Overlay Effect ───────────────────────────────────────
  useEffect(() => {
    const g = graphRef.current;
    if (!g || g.order === 0) return;

    if (activeLens === "hotspots" && hotspotsData && hotspotsData.length > 0) {
      const scoreMap = new Map<string, number>();
      let maxScore = 0;
      for (const h of hotspotsData) {
        scoreMap.set(h.path.replace(/\\/g, '/'), h.score);
        if (h.score > maxScore) maxScore = h.score;
      }

      g.forEachNode((node, attrs) => {
        if (!attrs.filePath) return;
        
        let nodeScore = 0;
        const normalizedFilePath = attrs.filePath.replace(/\\/g, '/');
        for (const [path, score] of scoreMap.entries()) {
          if (normalizedFilePath.endsWith(path) || path.endsWith(normalizedFilePath)) {
            nodeScore = score;
            break;
          }
        }

        if (nodeScore > 0) {
          const intensity = maxScore > 0 ? Math.min(1, nodeScore / maxScore) : 0;
          const r = Math.round(234 + intensity * (239 - 234));
          const gCol = Math.round(179 + intensity * (68 - 179));
          const b = Math.round(8 + intensity * (68 - 8));
          
          g.setNodeAttribute(node, "color", `rgb(${r}, ${gCol}, ${b})`);
          g.setNodeAttribute(node, "size", (attrs.originalSize || attrs.size) * (1 + intensity * 0.5));
        } else {
          g.setNodeAttribute(node, "color", "var(--bg-3)");
          g.setNodeAttribute(node, "size", attrs.originalSize || attrs.size);
        }
      });
    } else if (activeLens !== "risk" && !showDeadCode) {
      // Risk and Dead Code have their own logic — don't reset if they are active.
      g.forEachNode((node, attrs) => {
        if (attrs.originalColor) g.setNodeAttribute(node, "color", attrs.originalColor);
        if (attrs.originalSize) g.setNodeAttribute(node, "size", attrs.originalSize);
      });
    }

    refresh();
  }, [activeLens, hotspotsData, graphRef, refresh, showDeadCode]);

  // ── Risk Composite Lens ───────────────────────────────────────────
  useEffect(() => {
    const g = graphRef.current;
    if (!g || g.order === 0) return;
    if (activeLens !== "risk") return;

    const hotspotScores = new Map<string, number>();
    let maxHotspot = 0;
    if (hotspotsData) {
      for (const h of hotspotsData) {
        const norm = h.path.replace(/\\/g, "/");
        hotspotScores.set(norm, h.score);
        if (h.score > maxHotspot) maxHotspot = h.score;
      }
    }

    g.forEachNode((node, attrs) => {
      let churn = 0;
      if (attrs.filePath && maxHotspot > 0) {
        const fp = attrs.filePath.replace(/\\/g, "/");
        for (const [path, score] of hotspotScores.entries()) {
          if (fp.endsWith(path) || path.endsWith(fp)) {
            churn = Math.min(1, score / maxHotspot);
            break;
          }
        }
      }
      const dead = attrs.isDeadCandidate ? 1 : 0;
      const untraced = attrs.isTraced === false ? 0.4 : 0;
      const llmRisk = typeof attrs.llmRiskScore === "number" ? attrs.llmRiskScore : 0;
// Composite ∈ [0, 1] — weighted sum capped at 1.
const risk = Math.min(
  1,
  churn * 0.4 + dead * 0.5 + untraced * 0.2 + llmRisk * 0.4,
);

if (risk >= riskThreshold) {
  if (risk > 0) {
    const r = Math.round(158 + risk * (247 - 158));
    const gCol = Math.round(206 + risk * (118 - 206));
    const b = Math.round(106 + risk * (142 - 106));
    g.setNodeAttribute(node, "color", `rgb(${r}, ${gCol}, ${b})`);
    const baseSize = attrs.originalSize || attrs.size;
    g.setNodeAttribute(node, "size", baseSize * (1 + risk * 0.6));
  } else {
    g.setNodeAttribute(node, "color", "rgba(120, 130, 145, 0.4)");
    if (attrs.originalSize) g.setNodeAttribute(node, "size", attrs.originalSize);
  }
} else {
  // Hide or extreme dimming for nodes below threshold
  g.setNodeAttribute(node, "color", "rgba(50, 50, 60, 0.05)");
  g.setNodeAttribute(node, "size", (attrs.originalSize || attrs.size) * 0.3);
  g.setNodeAttribute(node, "label", ""); // Hide label
}
});
refresh();
}, [activeLens, hotspotsData, graphRef, refresh, riskThreshold]);

  // ── Dead Code Effect ──────────────────────────────────────────────
  useEffect(() => {
    const g = graphRef.current;
    if (!g || g.order === 0) return;

    if (showDeadCode) {
      g.forEachNode((node, attrs) => {
        if (attrs.isDeadCandidate) {
          g.setNodeAttribute(node, "color", "var(--rose)");
          g.setNodeAttribute(node, "size", (attrs.originalSize || attrs.size) * 1.5);
        } else {
          g.setNodeAttribute(node, "color", "rgba(100, 116, 139, 0.2)");
        }
      });
    } else if (activeLens === "all") { // Only reset if no other lens is active
      g.forEachNode((node, attrs) => {
        if (attrs.originalColor) g.setNodeAttribute(node, "color", attrs.originalColor);
        if (attrs.originalSize) g.setNodeAttribute(node, "size", attrs.originalSize);
      });
    }
    refresh();
  }, [showDeadCode, activeLens, graphRef, refresh]);
}
