/**
 * Pre-configured Sonner toaster themed for Obsidian Observatory.
 */

import { Toaster as SonnerToaster } from "sonner";
import { useAppStore } from "../../stores/app-store";

export function Toaster() {
  const theme = useAppStore((s) => s.theme);
  const sonnerTheme = theme === "light" ? "light" : "dark";

  return (
    <SonnerToaster
      theme={sonnerTheme}
      position="bottom-right"
      closeButton
      toastOptions={{
        style: {
          background: "var(--bg-2)",
          border: "1px solid var(--surface-border)",
          color: "var(--text-0)",
          fontFamily: "var(--font-body)",
          fontSize: "13px",
          boxShadow: "var(--shadow-lg)",
        },
      }}
      gap={8}
      visibleToasts={4}
    />
  );
}
