import { motion } from "framer-motion";
import { Network, SquareStack } from "lucide-react";

export type ViewMode = "graph" | "treemap";

interface ViewModeToggleProps {
  mode: ViewMode;
  onChange: (mode: ViewMode) => void;
}

const options: { id: ViewMode; label: string; icon: typeof Network }[] = [
  { id: "graph", label: "Graph", icon: Network },
  { id: "treemap", label: "Treemap", icon: SquareStack },
];

export function ViewModeToggle({ mode, onChange }: ViewModeToggleProps) {
  return (
    <div
      role="radiogroup"
      aria-label="View mode"
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 0,
        height: 32,
        borderRadius: "var(--radius-md)",
        background: "var(--bg-2)",
        border: "1px solid var(--surface-border)",
        padding: 2,
      }}
    >
      {options.map(({ id, label, icon: Icon }) => {
        const isActive = mode === id;
        return (
          <button
            key={id}
            onClick={() => onChange(id)}
            aria-pressed={isActive}
            aria-label={label}
            style={{
              position: "relative",
              display: "flex",
              alignItems: "center",
              gap: 5,
              height: 26,
              padding: "0 10px",
              border: "none",
              borderRadius: "calc(var(--radius-md) - 2px)",
              background: "transparent",
              cursor: "pointer",
              fontSize: 12,
              fontWeight: isActive ? 600 : 400,
              color: isActive ? "var(--text-0)" : "var(--text-3)",
              zIndex: isActive ? 1 : 0,
              transition: "color 0.15s ease",
            }}
          >
            {isActive && (
              <motion.div
                layoutId="viewmode-indicator"
                style={{
                  position: "absolute",
                  inset: 0,
                  borderRadius: "calc(var(--radius-md) - 2px)",
                  background: "var(--surface)",
                  border: "1px solid var(--surface-border-hover)",
                  boxShadow: "0 1px 2px rgba(0,0,0,0.15)",
                }}
                transition={{ type: "spring", stiffness: 400, damping: 30 }}
              />
            )}
            <Icon size={14} style={{ position: "relative", zIndex: 1 }} />
            <span style={{ position: "relative", zIndex: 1 }}>{label}</span>
          </button>
        );
      })}
    </div>
  );
}
