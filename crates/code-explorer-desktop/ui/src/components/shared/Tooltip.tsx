import { useState, useRef, useId, type ReactNode } from "react";
import { createPortal } from "react-dom";

interface TooltipProps {
  content: string | undefined;
  children: ReactNode;
  /** Delay before showing (ms). Default 400 */
  delay?: number;
  /** Preferred placement. Default "bottom" */
  placement?: "top" | "bottom" | "left" | "right";
}

export function Tooltip({ content, children, delay = 400, placement = "bottom" }: TooltipProps) {
  const tooltipId = useId();
  const [visible, setVisible] = useState(false);
  const [pos, setPos] = useState({ x: 0, y: 0 });
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const wrapRef = useRef<HTMLSpanElement>(null);

  if (!content) {
    return <>{children}</>;
  }

  const show = () => {
    timerRef.current = setTimeout(() => {
      if (!wrapRef.current) return;
      const rect = wrapRef.current.getBoundingClientRect();
      let x: number, y: number;
      switch (placement) {
        case "top":
          x = rect.left + rect.width / 2;
          y = rect.top - 6;
          break;
        case "left":
          x = rect.left - 6;
          y = rect.top + rect.height / 2;
          break;
        case "right":
          x = rect.right + 6;
          y = rect.top + rect.height / 2;
          break;
        default: // bottom
          x = rect.left + rect.width / 2;
          y = rect.bottom + 6;
      }
      setPos({ x, y });
      setVisible(true);
    }, delay);
  };

  const hide = () => {
    if (timerRef.current) clearTimeout(timerRef.current);
    setVisible(false);
  };

  const transformOrigin = {
    top: "translateX(-50%) translateY(-100%)",
    bottom: "translateX(-50%)",
    left: "translateX(-100%) translateY(-50%)",
    right: "translateY(-50%)",
  }[placement];

  return (
    <span
      ref={wrapRef}
      onMouseEnter={show}
      onMouseLeave={hide}
      onFocus={show}
      onBlur={hide}
      aria-describedby={visible ? tooltipId : undefined}
      style={{ display: "inline-flex" }}
    >
      {children}
      {visible &&
        createPortal(
          <div
            id={tooltipId}
            role="tooltip"
            style={{
              position: "fixed",
              left: pos.x,
              top: pos.y,
              transform: transformOrigin,
              zIndex: 99999,
              maxWidth: 260,
              padding: "6px 10px",
              borderRadius: 6,
              background: "var(--bg-4)",
              border: "1px solid var(--surface-border-hover)",
              boxShadow: "var(--shadow-md)",
              color: "var(--text-1)",
              fontSize: 11,
              fontFamily: "var(--font-body)",
              lineHeight: 1.45,
              pointerEvents: "none",
              whiteSpace: "pre-wrap",
            }}
          >
            {content}
          </div>,
          document.body
        )}
    </span>
  );
}
