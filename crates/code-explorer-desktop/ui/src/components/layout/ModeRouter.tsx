import { lazy, Suspense } from "react";
import { AnimatePresence } from "framer-motion";
import { useAppStore } from "../../stores/app-store";
import { ErrorBoundary } from "../shared/ErrorBoundary";
import { AnimatedPage } from "../shared/motion";
import { LoadingOrbs } from "../shared/LoadingOrbs";

const ExplorerMode = lazy(() =>
  import("../explorer/ExplorerMode").then((m) => ({ default: m.ExplorerMode })),
);
const AnalyzeMode = lazy(() =>
  import("../analyze/AnalyzeMode").then((m) => ({ default: m.AnalyzeMode })),
);
const ChatMode = lazy(() =>
  import("../chat/ChatMode").then((m) => ({ default: m.ChatMode })),
);
const ManageMode = lazy(() =>
  import("../manage/ManageMode").then((m) => ({ default: m.ManageMode })),
);

const LazyFallback = (
  <div className="flex items-center justify-center h-full">
    <LoadingOrbs />
  </div>
);

export function ModeRouter() {
  const mode = useAppStore((s) => s.mode);

  return (
    <>
      {/* Explorer is ALWAYS mounted to preserve Sigma.js WebGL context.
          Hidden via CSS when not active — no unmount/remount. */}
      <div
        style={{
          visibility: mode === "explorer" ? "visible" : "hidden",
          pointerEvents: mode === "explorer" ? "auto" : "none",
          position: mode === "explorer" ? "relative" : "absolute",
          inset: mode === "explorer" ? undefined : 0,
          width: "100%",
          height: "100%",
        }}
      >
        <ErrorBoundary>
          <Suspense fallback={LazyFallback}>
            <ExplorerMode />
          </Suspense>
        </ErrorBoundary>
      </div>

      <AnimatePresence mode="wait">
        {mode === "analyze" && (
          <AnimatedPage key="analyze">
            <ErrorBoundary>
              <Suspense fallback={LazyFallback}>
                <AnalyzeMode />
              </Suspense>
            </ErrorBoundary>
          </AnimatedPage>
        )}
        {mode === "chat" && (
          <AnimatedPage key="chat">
            <ErrorBoundary>
              <Suspense fallback={LazyFallback}>
                <ChatMode />
              </Suspense>
            </ErrorBoundary>
          </AnimatedPage>
        )}
        {mode === "manage" && (
          <AnimatedPage key="manage">
            <ErrorBoundary>
              <Suspense fallback={LazyFallback}>
                <ManageMode />
              </Suspense>
            </ErrorBoundary>
          </AnimatedPage>
        )}
      </AnimatePresence>
    </>
  );
}
