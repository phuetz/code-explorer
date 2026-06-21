import type { ReactNode } from "react";
import type { LucideIcon } from "lucide-react";

interface EmptyStateProps {
  icon: LucideIcon;
  title: string;
  description?: string;
  action?: {
    label: string;
    onClick: () => void;
  };
  children?: ReactNode;
}

export function EmptyState({ icon: Icon, title, description, action, children }: EmptyStateProps) {
  return (
    <div
      className="flex flex-col items-center justify-center h-full select-none"
      style={{
        padding: "40px 24px",
        background: "radial-gradient(ellipse at center, var(--accent-glow) 0%, transparent 70%)",
      }}
    >
      <div
        style={{
          width: 72,
          height: 72,
          borderRadius: "50%",
          background: "var(--accent-subtle)",
          border: "1px solid var(--accent-border)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          marginBottom: 20,
        }}
      >
        <Icon size={32} style={{ color: "var(--accent)", opacity: 0.8 }} />
      </div>

      <h3
        style={{
          fontFamily: "var(--font-display)",
          fontSize: 17,
          fontWeight: 600,
          color: "var(--text-1)",
          marginBottom: 8,
          textAlign: "center",
        }}
      >
        {title}
      </h3>

      {description && (
        <p
          style={{
            fontSize: 13,
            color: "var(--text-3)",
            textAlign: "center",
            maxWidth: 360,
            lineHeight: 1.5,
            marginBottom: action ? 20 : 0,
          }}
        >
          {description}
        </p>
      )}

      {action && (
        <button
          onClick={action.onClick}
          style={{
            padding: "10px 24px",
            borderRadius: "var(--radius-lg)",
            background: "var(--accent)",
            color: "white",
            fontFamily: "var(--font-display)",
            fontSize: 13,
            fontWeight: 600,
            border: "none",
            cursor: "pointer",
            boxShadow: "0 0 20px var(--accent-glow)",
            transition: "transform var(--transition-fast), box-shadow var(--transition-fast)",
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.transform = "translateY(-1px)";
            e.currentTarget.style.boxShadow = "0 4px 24px rgba(106, 161, 248, 0.3)";
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.transform = "translateY(0)";
            e.currentTarget.style.boxShadow = "0 0 20px var(--accent-glow)";
          }}
        >
          {action.label}
        </button>
      )}

      {children}
    </div>
  );
}
