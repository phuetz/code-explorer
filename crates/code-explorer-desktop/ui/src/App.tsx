import { useEffect } from "react";
import { MotionConfig, useReducedMotion } from "framer-motion";
import { ModeBar } from "./components/layout/ModeBar";
import { ModeRouter } from "./components/layout/ModeRouter";
import { StatusBar } from "./components/layout/StatusBar";
import { ModalManager } from "./components/layout/ModalManager";
import { ConfirmDialogProvider } from "./components/shared/ConfirmDialog";
import { useKeyboardShortcuts } from "./hooks/use-keyboard-shortcuts";
import { useScreenCapture } from "./hooks/use-screen-capture";
import { useShareLink } from "./hooks/use-share-link";
import { useI18n } from "./hooks/use-i18n";
import { useAppStore } from "./stores/app-store";
import { commands } from "./lib/tauri-commands";

function App() {
  const { t, locale } = useI18n();
  const shouldReduceMotion = useReducedMotion();
  
  useKeyboardShortcuts();
  useScreenCapture();
  useShareLink();

  // Set HTML lang attribute based on locale
  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);

  // Listen for OS theme changes when "system" theme is selected
  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = () => {
      if (useAppStore.getState().theme === "system") {
        document.documentElement.setAttribute("data-theme", mq.matches ? "dark" : "light");
      }
    };
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, []);

  // Auto-restore the last active repo on startup (persisted via Zustand)
  useEffect(() => {
    let ignore = false;
    const saved = useAppStore.getState().activeRepo;
    if (saved) {
      commands.listRepos().then(() =>
        commands.openRepo(saved)
      ).catch((err) => {
        console.warn("Failed to restore repo:", err);
        if (!ignore) useAppStore.getState().setActiveRepo(null);
      });
    }
    return () => { ignore = true; };
  }, []);

  return (
    <MotionConfig reducedMotion={shouldReduceMotion ? "always" : "never"}>
      <div className="flex flex-col h-screen w-screen overflow-hidden bg-bg-0">
        <a 
          href="#main-content" 
          className="sr-only focus:not-sr-only focus:absolute focus:z-[9999] top-2 left-2 px-4 py-2 bg-accent text-white rounded-lg font-semibold text-[13px]"
        >
          {t("a11y.skipToContent")}
        </a>
        <h1 className="sr-only">{t("a11y.codeIntelligencePlatform")}</h1>

        <div className="flex flex-1 min-h-0">
          <nav aria-label="Mode navigation">
            <ModeBar />
          </nav>
          <main id="main-content" className="flex-1 min-w-0 relative">
            <ModeRouter />
          </main>
        </div>

        <footer>
          <StatusBar />
        </footer>

        <ModalManager />
        <ConfirmDialogProvider />
      </div>
    </MotionConfig>
  );
}

export default App;
