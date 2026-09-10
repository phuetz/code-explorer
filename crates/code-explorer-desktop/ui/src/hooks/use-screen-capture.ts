import { useEffect, useCallback } from "react";

/**
 * Hook that adds Ctrl+Shift+S to capture a screenshot of the app.
 * Saves as PNG download (triggers browser download dialog).
 * Also provides a captureScreen function for programmatic use.
 */
export function useScreenCapture() {
  const captureScreen = useCallback(async () => {
    const root = document.getElementById("root");
    if (!root) return;

    try {
      const { toPng } = await import("html-to-image");
      const dataUrl = await toPng(root, {
        backgroundColor: "#090b10",
        pixelRatio: 2,
        filter: (node) => {
          // Skip tooltips and modals that might be partially visible
          if (node instanceof HTMLElement) {
            const cl = node.classList;
            if (cl.contains("sr-only")) return false;
          }
          return true;
        },
      });

      // Generate filename with timestamp
      const now = new Date();
      const ts = now.toISOString().replace(/[:.]/g, "-").slice(0, 19);
      const filename = `code-explorer-capture-${ts}.png`;

      // Trigger download
      const link = document.createElement("a");
      link.download = filename;
      link.href = dataUrl;
      document.body.appendChild(link);
      link.click();
      document.body.removeChild(link);

      // Visual feedback
      showCaptureFlash();
    } catch (err) {
      console.error("Screenshot failed:", err);
    }
  }, []);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      // F12 or Ctrl+Shift+S: capture screenshot
      if (e.key === "F12" || ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key === "S")) {
        e.preventDefault();
        captureScreen();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [captureScreen]);

  return { captureScreen };
}

/** Brief white flash overlay to confirm capture */
function showCaptureFlash() {
  const flash = document.createElement("div");
  flash.style.cssText = `
    position: fixed; inset: 0; z-index: 99999;
    background: rgba(255,255,255,0.15);
    pointer-events: none;
    animation: capture-flash 0.3s ease-out forwards;
  `;

  // Add keyframes if not already present
  if (!document.getElementById("capture-flash-style")) {
    const style = document.createElement("style");
    style.id = "capture-flash-style";
    style.textContent = `
      @keyframes capture-flash {
        0% { opacity: 1; }
        100% { opacity: 0; }
      }
    `;
    document.head.appendChild(style);
  }

  document.body.appendChild(flash);
  setTimeout(() => flash.remove(), 300);
}
