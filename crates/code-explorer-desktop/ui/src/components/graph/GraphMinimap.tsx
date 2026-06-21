import { useRef, useCallback, useEffect } from "react";
import { EyeOff } from "lucide-react";
import type Sigma from "sigma";
import type Graph from "graphology";
import type { SigmaNodeAttributes, SigmaEdgeAttributes } from "../../lib/graph-adapter";

interface GraphMinimapProps {
  visible: boolean;
  opacity: number;
  onOpacityChange: (v: number) => void;
  onClose: () => void;
  sigmaRef: React.RefObject<Sigma | null>;
  graphRef: React.RefObject<Graph<SigmaNodeAttributes, SigmaEdgeAttributes> | null>;
}

export function GraphMinimap({
  visible,
  opacity,
  onOpacityChange,
  onClose,
  sigmaRef,
  graphRef,
}: GraphMinimapProps) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    const sigma = sigmaRef.current;
    const graph = graphRef.current;
    if (!canvas || !sigma || !graph || graph.order === 0) return;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const canvasW = 160;
    const canvasH = 120;
    ctx.fillStyle = "#1a1b26";
    ctx.fillRect(0, 0, canvasW, canvasH);

    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    graph.forEachNode((_n, attrs) => {
      if (attrs.x < minX) minX = attrs.x;
      if (attrs.y < minY) minY = attrs.y;
      if (attrs.x > maxX) maxX = attrs.x;
      if (attrs.y > maxY) maxY = attrs.y;
    });

    const graphW = maxX - minX || 1;
    const graphH = maxY - minY || 1;
    const padding = 8;
    const innerW = canvasW - padding * 2;
    const innerH = canvasH - padding * 2;
    const scale = Math.min(innerW / graphW, innerH / graphH);

    graph.forEachNode((_n, attrs) => {
      const x = padding + (attrs.x - minX) * scale;
      const y = padding + (attrs.y - minY) * scale;
      ctx.fillStyle = attrs.color || "#565f89";
      ctx.beginPath();
      ctx.arc(x, y, 2, 0, Math.PI * 2);
      ctx.fill();
    });

    // Viewport rectangle.
    // Use Sigma's `viewportToGraph` to transform the four screen corners into
    // graph coordinates, then map them through the same scale used for nodes.
    // The previous math collapsed to `(graphW * 0.5 - graphW * 0.5) === 0`,
    // leaving the viewport box systematically misaligned.
    const dim = sigma.getDimensions();
    const tl = sigma.viewportToGraph({ x: 0, y: 0 });
    const br = sigma.viewportToGraph({ x: dim.width, y: dim.height });

    const graphToMiniX = (gx: number) => padding + (gx - minX) * scale;
    const graphToMiniY = (gy: number) => padding + (gy - minY) * scale;

    // Sigma's Y axis can be inverted relative to mini-canvas Y; normalize
    // so width/height are positive regardless of orientation.
    const vx1 = graphToMiniX(tl.x);
    const vx2 = graphToMiniX(br.x);
    const vy1 = graphToMiniY(tl.y);
    const vy2 = graphToMiniY(br.y);
    const vpX = Math.min(vx1, vx2);
    const vpY = Math.min(vy1, vy2);
    const vpW = Math.abs(vx2 - vx1);
    const vpH = Math.abs(vy2 - vy1);

    ctx.strokeStyle = "var(--accent)";
    ctx.lineWidth = 1.5;
    ctx.fillStyle = "rgba(122, 162, 247, 0.12)";
    ctx.fillRect(vpX, vpY, vpW, vpH);
    ctx.strokeRect(vpX, vpY, vpW, vpH);
  }, [sigmaRef, graphRef]);

  // Throttled draw via RAF
  const rafRef = useRef<number | null>(null);
  const drawThrottled = useCallback(() => {
    if (rafRef.current) return;
    rafRef.current = requestAnimationFrame(() => {
      draw();
      rafRef.current = null;
    });
  }, [draw]);

  // Attach to sigma afterRender — include `visible` so re-subscribes when minimap shown
  useEffect(() => {
    if (!visible) return;
    const sigma = sigmaRef.current;
    if (!sigma) return;
    sigma.on("afterRender", drawThrottled);
    drawThrottled(); // initial draw
    return () => {
      sigma.removeListener("afterRender", drawThrottled);
      if (rafRef.current) cancelAnimationFrame(rafRef.current);
    };
  }, [sigmaRef, drawThrottled, visible]);

  if (!visible) return null;

  return (
    <div
      className="absolute z-15 pointer-events-auto"
      style={{
        bottom: "16px",
        left: "16px",
        borderRadius: "var(--radius-md)",
        backgroundColor: "var(--bg-2)",
        border: "1px solid var(--surface-border)",
        opacity,
        transition: "opacity 0.2s ease",
      }}
      onMouseEnter={() => onOpacityChange(1.0)}
      onMouseLeave={() => onOpacityChange(0.8)}
    >
      <canvas
        ref={canvasRef}
        width={160}
        height={120}
        style={{
          display: "block",
          cursor: "pointer",
          borderRadius: "var(--radius-md)",
        }}
      />
      <button
        onClick={onClose}
        aria-label="Close minimap"
        className="absolute transition-colors hover:brightness-125"
        style={{
          top: "4px",
          right: "4px",
          padding: "4px",
          backgroundColor: "rgba(0, 0, 0, 0.5)",
          borderRadius: "4px",
          color: "var(--text-3)",
          border: "none",
          cursor: "pointer",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <EyeOff size={12} />
      </button>
    </div>
  );
}
