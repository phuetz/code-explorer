import { useState, useEffect } from "react";

/** Breakpoints matching common viewport widths */
const BREAKPOINTS = {
  /** Below this, sidebar auto-collapses */
  compact: 900,
  /** Below this, hide detail panel entirely */
  narrow: 700,
};

export interface ResponsiveState {
  /** Window width in pixels */
  width: number;
  /** true when window width < 900px */
  isCompact: boolean;
  /** true when window width < 700px */
  isNarrow: boolean;
}

/**
 * Hook that tracks the viewport width and returns responsive breakpoint flags.
 * Components can use these flags to adapt their layout.
 */
export function useResponsive(): ResponsiveState {
  const [width, setWidth] = useState(window.innerWidth);

  useEffect(() => {
    let raf: number;
    const handler = () => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => setWidth(window.innerWidth));
    };
    window.addEventListener("resize", handler);
    return () => {
      window.removeEventListener("resize", handler);
      cancelAnimationFrame(raf);
    };
  }, []);

  return {
    width,
    isCompact: width < BREAKPOINTS.compact,
    isNarrow: width < BREAKPOINTS.narrow,
  };
}
