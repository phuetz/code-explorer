import { memo } from "react";
import { useAppStore } from "../../stores/app-store";

const DEPTHS = [1, 2, 3] as const;

export const EgoDepthSlider = memo(function EgoDepthSlider() {
  const egoDepth = useAppStore((s) => s.egoDepth);
  const setEgoDepth = useAppStore((s) => s.setEgoDepth);

  return (
    <div className="flex items-center gap-1">
      <span
        className="text-xs"
        style={{ color: "var(--text-3)", fontFamily: "var(--font-body)" }}
      >
        Depth
      </span>
      <div
        className="flex rounded-md overflow-hidden"
        role="group"
        aria-label="Ego network depth"
        style={{ border: "1px solid var(--surface-border)" }}
      >
        {DEPTHS.map((d) => (
          <button
            key={d}
            onClick={() => setEgoDepth(d)}
            aria-label={`Depth ${d}`}
            aria-pressed={egoDepth === d}
            className="px-2 py-0.5 text-xs font-medium transition-colors"
            style={{
              background: egoDepth === d ? "var(--accent-subtle)" : "var(--bg-3)",
              color: egoDepth === d ? "var(--accent)" : "var(--text-3)",
              borderRight: d < 3 ? "1px solid var(--surface-border)" : undefined,
              fontFamily: "var(--font-mono)",
            }}
          >
            {d}
          </button>
        ))}
      </div>
    </div>
  );
});
