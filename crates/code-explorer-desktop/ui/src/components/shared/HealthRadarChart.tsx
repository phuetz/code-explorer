import { useMemo } from "react";
import type { CodeHealth } from "../../lib/tauri-commands";

export function HealthRadarChart({ health }: { health: CodeHealth }) {
  const size = 260;
  const center = size / 2;
  const radius = size * 0.35;

  const metrics = useMemo(() => [
    { label: "Overall", value: health.overallScore / 100 },
    { label: "Cohesion", value: health.cohesionScore / 100 },
    { label: "Coverage", value: health.tracingCoverage / 100 },
    { label: "Ownership", value: health.ownershipScore / 100 },
    { label: "Maintainability", value: Math.max(0, 1 - health.avgComplexity / 50) },
    { label: "Stability", value: Math.max(0, 1 - health.hotspotScore / 100) },
  ], [health]);

  const angleStep = (Math.PI * 2) / metrics.length;

  const points = metrics.map((m, i) => {
    const angle = i * angleStep - Math.PI / 2;
    const val = Math.max(0.1, m.value); // min 10% for visibility
    return {
      x: center + Math.cos(angle) * radius * val,
      y: center + Math.sin(angle) * radius * val,
      labelX: center + Math.cos(angle) * (radius + 25),
      labelY: center + Math.sin(angle) * (radius + 25),
      label: m.label,
    };
  });

  const polygonPath = points.map(p => `${p.x},${p.y}`).join(" ");

  return (
    <div className="flex flex-col items-center">
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} className="overflow-visible">
        {/* Background webs */}
        {[0.2, 0.4, 0.6, 0.8, 1.0].map((tick) => (
          <polygon
            key={tick}
            points={metrics.map((_, i) => {
              const angle = i * angleStep - Math.PI / 2;
              return `${center + Math.cos(angle) * radius * tick},${center + Math.sin(angle) * radius * tick}`;
            }).join(" ")}
            fill="none"
            stroke="var(--surface-border)"
            strokeWidth="1"
            strokeDasharray={tick === 1 ? "" : "2,2"}
          />
        ))}

        {/* Axis lines */}
        {metrics.map((_, i) => {
          const angle = i * angleStep - Math.PI / 2;
          return (
            <line
              key={i}
              x1={center}
              y1={center}
              x2={center + Math.cos(angle) * radius}
              y2={center + Math.sin(angle) * radius}
              stroke="var(--surface-border)"
              strokeWidth="1"
            />
          );
        })}

        {/* Data polygon */}
        <polygon
          points={polygonPath}
          fill="color-mix(in srgb, var(--accent) 30%, transparent)"
          stroke="var(--accent)"
          strokeWidth="2"
          className="transition-all duration-700 ease-out"
        />

        {/* Data points */}
        {points.map((p, i) => (
          <circle key={i} cx={p.x} cy={p.y} r="3" fill="var(--accent)" />
        ))}

        {/* Labels */}
        {points.map((p, i) => (
          <text
            key={i}
            x={p.labelX}
            y={p.labelY}
            textAnchor="middle"
            dominantBaseline="middle"
            className="text-[10px] font-semibold uppercase tracking-tighter"
            fill="var(--text-3)"
          >
            {p.label}
          </text>
        ))}
      </svg>
    </div>
  );
}
