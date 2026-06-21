/**
 * Pure SVG donut chart — no external dependencies.
 */

interface Segment {
  label: string;
  value: number;
  color: string;
}

interface DonutChartProps {
  segments: Segment[];
  size?: number;
  thickness?: number;
  centerLabel?: string;
  centerValue?: string;
}

export function DonutChart({
  segments,
  size = 140,
  thickness = 18,
  centerLabel,
  centerValue,
}: DonutChartProps) {
  const total = segments.reduce((sum, s) => sum + s.value, 0);
  if (total === 0) return null;

  const radius = (size - thickness) / 2;
  const circumference = 2 * Math.PI * radius;
  const center = size / 2;

  // Pre-compute offsets for each segment
  const segmentData = segments.map((seg, i) => {
    const pct = seg.value / total;
    const dashLength = pct * circumference;
    const offset = segments.slice(0, i).reduce((sum, s) => sum + (s.value / total) * circumference, 0);
    return { ...seg, dashLength, dashGap: circumference - dashLength, offset };
  });

  return (
    <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 12 }}>
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`}>
        {/* Background ring */}
        <circle
          cx={center}
          cy={center}
          r={radius}
          fill="none"
          stroke="var(--bg-3)"
          strokeWidth={thickness}
        />

        {/* Segments */}
        {segmentData.map((seg) => (
          <circle
            key={seg.label}
            cx={center}
            cy={center}
            r={radius}
            fill="none"
            stroke={seg.color}
            strokeWidth={thickness}
            strokeDasharray={`${seg.dashLength} ${seg.dashGap}`}
            strokeDashoffset={-seg.offset}
            strokeLinecap="butt"
            style={{
              transform: "rotate(-90deg)",
              transformOrigin: "center",
              transition: "stroke-dasharray 0.6s ease, stroke-dashoffset 0.6s ease",
            }}
          />
        ))}

        {/* Center text */}
        {centerValue && (
          <>
            <text
              x={center}
              y={center - 4}
              textAnchor="middle"
              dominantBaseline="auto"
              style={{
                fontSize: 22,
                fontWeight: 700,
                fontFamily: "var(--font-display)",
                fill: "var(--text-0)",
              }}
            >
              {centerValue}
            </text>
            {centerLabel && (
              <text
                x={center}
                y={center + 14}
                textAnchor="middle"
                dominantBaseline="auto"
                style={{
                  fontSize: 10,
                  fill: "var(--text-3)",
                  fontFamily: "var(--font-body)",
                }}
              >
                {centerLabel}
              </text>
            )}
          </>
        )}
      </svg>

      {/* Legend */}
      <div style={{ display: "flex", flexWrap: "wrap", gap: "6px 14px", justifyContent: "center" }}>
        {segments.map((seg) => (
          <div key={seg.label} style={{ display: "flex", alignItems: "center", gap: 5, fontSize: 11, color: "var(--text-2)" }}>
            <span style={{ width: 8, height: 8, borderRadius: "50%", background: seg.color, flexShrink: 0 }} />
            {seg.label} <span style={{ color: "var(--text-3)" }}>{seg.value}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
