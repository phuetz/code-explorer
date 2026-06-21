/**
 * Reusable drag handle separator for react-resizable-panels.
 * Consistent styling with accent glow on hover and 3 grip dots.
 */

import { Separator } from "react-resizable-panels";

export function PanelSeparator() {
  return (
    <Separator
      className="cursor-col-resize group relative"
      style={{ width: 5, background: "transparent" }}
    >
      {/* Thin line */}
      <div
        className="absolute inset-y-0 left-1/2 -translate-x-1/2"
        style={{
          width: 1,
          backgroundColor: "var(--surface-border)",
        }}
      />
      {/* Accent line on hover */}
      <div
        className="absolute inset-y-0 left-1/2 -translate-x-1/2 transition-opacity duration-150 opacity-0 group-hover:opacity-100"
        style={{
          width: 3,
          backgroundColor: "var(--accent)",
          borderRadius: 2,
        }}
      />
      {/* 3 grip dots */}
      <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 flex flex-col gap-1 opacity-0 group-hover:opacity-100 transition-opacity duration-150">
        <span
          className="w-1 h-1 rounded-full"
          style={{ backgroundColor: "var(--accent)" }}
        />
        <span
          className="w-1 h-1 rounded-full"
          style={{ backgroundColor: "var(--accent)" }}
        />
        <span
          className="w-1 h-1 rounded-full"
          style={{ backgroundColor: "var(--accent)" }}
        />
      </div>
    </Separator>
  );
}
