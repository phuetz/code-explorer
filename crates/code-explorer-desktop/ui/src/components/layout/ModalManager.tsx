import { lazy, Suspense } from "react";
import { useAppStore } from "../../stores/app-store";
import { LoadingOrbs } from "../shared/LoadingOrbs";

const SearchModal = lazy(() =>
  import("../search/SearchModal").then((m) => ({ default: m.SearchModal })),
);
const SettingsModal = lazy(() =>
  import("./SettingsModal").then((m) => ({ default: m.SettingsModal })),
);
const CommandPalette = lazy(() =>
  import("./CommandPalette").then((m) => ({ default: m.CommandPalette })),
);
const RenameModal = lazy(() =>
  import("./RenameModal").then((m) => ({ default: m.RenameModal })),
);
const NotebookPanel = lazy(() =>
  import("../notebooks/NotebookPanel").then((m) => ({ default: m.NotebookPanel })),
);
const DashboardPanel = lazy(() =>
  import("../dashboards/DashboardPanel").then((m) => ({ default: m.DashboardPanel })),
);
const WorkflowPanel = lazy(() =>
  import("../workflows/WorkflowPanel").then((m) => ({ default: m.WorkflowPanel })),
);
const UserCommandsPanel = lazy(() =>
  import("../plugins/UserCommandsPanel").then((m) => ({ default: m.UserCommandsPanel })),
);

const modalFallback = (
  <div className="fixed inset-0 z-50 flex items-center justify-center pointer-events-none">
    <LoadingOrbs />
  </div>
);

export function ModalManager() {
  const searchOpen = useAppStore((s) => s.searchOpen);
  const settingsOpen = useAppStore((s) => s.settingsOpen);
  const commandPaletteOpen = useAppStore((s) => s.commandPaletteOpen);
  const renameModal = useAppStore((s) => s.renameModal);
  const closeRenameModal = useAppStore((s) => s.closeRenameModal);
  const notebooksOpen = useAppStore((s) => s.notebooksOpen);
  const setNotebooksOpen = useAppStore((s) => s.setNotebooksOpen);
  const dashboardsOpen = useAppStore((s) => s.dashboardsOpen);
  const setDashboardsOpen = useAppStore((s) => s.setDashboardsOpen);
  const workflowsOpen = useAppStore((s) => s.workflowsOpen);
  const setWorkflowsOpen = useAppStore((s) => s.setWorkflowsOpen);
  const userCommandsOpen = useAppStore((s) => s.userCommandsOpen);
  const setUserCommandsOpen = useAppStore((s) => s.setUserCommandsOpen);

  return (
    <>
      {searchOpen && (
        <Suspense fallback={modalFallback}>
          <SearchModal />
        </Suspense>
      )}
      {settingsOpen && (
        <Suspense fallback={modalFallback}>
          <SettingsModal />
        </Suspense>
      )}
      {commandPaletteOpen && (
        <Suspense fallback={modalFallback}>
          <CommandPalette />
        </Suspense>
      )}
      {renameModal.open && (
        <Suspense fallback={null}>
          <RenameModal
            open={renameModal.open}
            initialTarget={renameModal.initialTarget}
            onClose={closeRenameModal}
          />
        </Suspense>
      )}
      {notebooksOpen && (
        <Suspense fallback={null}>
          <NotebookPanel open={notebooksOpen} onClose={() => setNotebooksOpen(false)} />
        </Suspense>
      )}
      {dashboardsOpen && (
        <Suspense fallback={null}>
          <DashboardPanel open={dashboardsOpen} onClose={() => setDashboardsOpen(false)} />
        </Suspense>
      )}
      {workflowsOpen && (
        <Suspense fallback={null}>
          <WorkflowPanel open={workflowsOpen} onClose={() => setWorkflowsOpen(false)} />
        </Suspense>
      )}
      {userCommandsOpen && (
        <Suspense fallback={null}>
          <UserCommandsPanel open={userCommandsOpen} onClose={() => setUserCommandsOpen(false)} />
        </Suspense>
      )}
    </>
  );
}
