import { lazy, Suspense, useRef, useEffect } from "react";
import { Group, Panel, type PanelImperativeHandle } from "react-resizable-panels";
import { PanelSeparator } from "../layout/PanelSeparator";
import { ErrorBoundary } from "../shared/ErrorBoundary";
import { useAppStore } from "../../stores/app-store";
import { useResponsive } from "../../hooks/use-responsive";
import { WelcomeScreen } from "./WelcomeScreen";
import { LoadingOrbs } from "../shared/LoadingOrbs";

const ExplorerLeftPanel = lazy(() =>
  import("./ExplorerLeftPanel").then((m) => ({ default: m.ExplorerLeftPanel })),
);
const ExplorerRightPanel = lazy(() =>
  import("./ExplorerRightPanel").then((m) => ({ default: m.ExplorerRightPanel })),
);
const GraphExplorer = lazy(() =>
  import("../graph/GraphExplorer").then((m) => ({ default: m.GraphExplorer })),
);

const explorerFallback = (
  <div className="h-full flex items-center justify-center">
    <LoadingOrbs />
  </div>
);

export function ExplorerMode() {
  const activeRepo = useAppStore((s) => s.activeRepo);
  const selectedNodeId = useAppStore((s) => s.selectedNodeId);
  const { isCompact, isNarrow } = useResponsive();
  const leftPanelRef = useRef<PanelImperativeHandle>(null);
  const rightPanelRef = useRef<PanelImperativeHandle>(null);

  // Right panel: collapsed when narrow (regardless of selection), expanded
  // when a node is selected on a wide enough viewport, otherwise collapsed.
  // Merging the two responsibilities into a single effect avoids the previous
  // bug where, on initial mount with a persisted `selectedNodeId` and a narrow
  // viewport, the selection effect ran first (expand) and the narrow effect
  // ran second (collapse), but on later re-renders only the selection effect
  // ran (re-expanding when it shouldn't on narrow).
  useEffect(() => {
    if (isNarrow) {
      rightPanelRef.current?.collapse();
    } else if (selectedNodeId) {
      // Use resize(size) — in react-resizable-panels v4, expand() takes no
      // arguments and restores to the last pre-collapse size, which is
      // effectively `minSize` (here "0%") for a panel that has never been
      // manually expanded. resize() reliably sets the target width.
      rightPanelRef.current?.resize("28%");
    } else {
      rightPanelRef.current?.collapse();
    }
  }, [selectedNodeId, isNarrow]);

  // Responsive: collapse left panel at <900px
  useEffect(() => {
    if (isCompact) {
      leftPanelRef.current?.collapse();
    } else {
      // Same caveat as above — restore to the default 20% rather than minSize.
      leftPanelRef.current?.resize("20%");
    }
  }, [isCompact]);

  if (!activeRepo) {
    return <WelcomeScreen />;
  }

  return (
    <Group orientation="horizontal" className="h-full">
      <Panel
        panelRef={leftPanelRef}
        defaultSize={isCompact ? "0%" : "20%"}
        minSize="12%"
        maxSize="25%"
        collapsible
      >
        <ErrorBoundary>
          <Suspense fallback={explorerFallback}>
            <ExplorerLeftPanel />
          </Suspense>
        </ErrorBoundary>
      </Panel>
      <PanelSeparator />
      <Panel minSize="30%">
        <div className="h-full w-full overflow-hidden">
          <ErrorBoundary>
            <Suspense fallback={explorerFallback}>
              <GraphExplorer />
            </Suspense>
          </ErrorBoundary>
        </div>
      </Panel>
      <PanelSeparator />
      <Panel
        panelRef={rightPanelRef}
        defaultSize="0%"
        minSize="0%"
        maxSize="35%"
        collapsible
      >
        <div className="h-full w-full overflow-hidden">
          <ErrorBoundary>
            <Suspense fallback={explorerFallback}>
              <ExplorerRightPanel />
            </Suspense>
          </ErrorBoundary>
        </div>
      </Panel>
    </Group>
  );
}
