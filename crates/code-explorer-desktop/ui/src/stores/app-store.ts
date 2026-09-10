import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { ZoomLevel } from "../lib/tauri-commands";

export type DetailTab = "context" | "code" | "properties" | "layers" | "health";
export type ThemeMode = "dark" | "light" | "system";
export type AppMode = 'explorer' | 'analyze' | 'chat' | 'manage';
export type AnalyzeView =
  | 'overview'
  | 'hotspots'
  | 'coupling'
  | 'ownership'
  | 'coverage'
  | 'diagram'
  | 'report'
  | 'health'
  | 'processes'
  | 'snapshots'
  | 'cycles'
  | 'clones'
  | 'todos'
  | 'complexity'
  // Schema & API Inventory (Theme D)
  | 'endpoints'
  | 'schema'
  | 'env_vars';
export type LensType = 'all' | 'calls' | 'structure' | 'heritage' | 'impact' | 'dead-code' | 'tracing' | 'hotspots' | 'risk';

interface HistoryEntry {
  nodeId: string;
  nodeName: string | null;
}

export interface AppState {
  activeRepo: string | null;
  setActiveRepo: (name: string | null) => void;

  selectedNodeId: string | null;
  selectedNodeName: string | null;
  setSelectedNodeId: (id: string | null, name?: string | null) => void;

  // Navigation history (browser-style back/forward)
  navigationHistory: HistoryEntry[];
  historyIndex: number;
  canGoBack: boolean;
  canGoForward: boolean;
  goBack: () => void;
  goForward: () => void;

  detailTab: DetailTab;
  setDetailTab: (tab: DetailTab) => void;

  zoomLevel: ZoomLevel;
  setZoomLevel: (level: ZoomLevel) => void;

  searchOpen: boolean;
  setSearchOpen: (open: boolean) => void;

  searchQuery: string;
  setSearchQuery: (query: string) => void;

  settingsOpen: boolean;
  setSettingsOpen: (open: boolean) => void;

  commandPaletteOpen: boolean;
  setCommandPaletteOpen: (open: boolean) => void;

  searchMatchIds: string[];
  setSearchMatchIds: (ids: string[]) => void;

  theme: ThemeMode;
  setTheme: (theme: ThemeMode) => void;

  mode: AppMode;
  setMode: (mode: AppMode) => void;
  analyzeView: AnalyzeView;
  setAnalyzeView: (view: AnalyzeView) => void;
  activeLens: LensType;
  setActiveLens: (lens: LensType) => void;
  clusterByCommunity: boolean;
  setClusterByCommunity: (v: boolean) => void;
  riskThreshold: number;
  setRiskThreshold: (v: number) => void;
  egoDepth: 1 | 2 | 3;
  setEgoDepth: (depth: 1 | 2 | 3) => void;
  explorerLeftCollapsed: boolean;
  setExplorerLeftCollapsed: (collapsed: boolean) => void;

  sidebarExpanded: boolean;
  setSidebarExpanded: (expanded: boolean) => void;
  toggleSidebar: () => void;

  selectedFeatures: Set<string>;
  toggleFeature: (id: string) => void;
  resetFeatures: () => void;

  // Rename modal (open + initial target)
  renameModal: { open: boolean; initialTarget?: string };
  openRenameModal: (initialTarget?: string) => void;
  closeRenameModal: () => void;

  // Notebooks panel
  notebooksOpen: boolean;
  setNotebooksOpen: (open: boolean) => void;

  dashboardsOpen: boolean;
  setDashboardsOpen: (open: boolean) => void;

  workflowsOpen: boolean;
  setWorkflowsOpen: (open: boolean) => void;

  userCommandsOpen: boolean;
  setUserCommandsOpen: (open: boolean) => void;

  /**
   * Chat settings modal — lifted to the store so it can be opened from
   * anywhere in the app (e.g., clicking the LLM model name in the
   * StatusBar) instead of only from the Chat mode's local state.
   */
  chatSettingsOpen: boolean;
  setChatSettingsOpen: (open: boolean) => void;
}

const MAX_HISTORY = 50;

export const useAppStore = create<AppState>()(
  persist(
    (set, get) => ({
      activeRepo: null,
      setActiveRepo: (name) => set({ activeRepo: name }),

      selectedNodeId: null,
      selectedNodeName: null,
      setSelectedNodeId: (id, name) => {
        const state = get();
        if (id && id !== state.selectedNodeId) {
          // Push to navigation history (truncate forward history)
          const truncated = state.navigationHistory.slice(0, state.historyIndex + 1);
          const entry: HistoryEntry = { nodeId: id, nodeName: name ?? null };
          const newHistory = [...truncated, entry].slice(-MAX_HISTORY);
          const newIndex = newHistory.length - 1;
          set({
            selectedNodeId: id,
            selectedNodeName: name ?? null,
            navigationHistory: newHistory,
            historyIndex: newIndex,
            canGoBack: newIndex > 0,
            canGoForward: false,
          });
        } else {
          set({ selectedNodeId: id, selectedNodeName: name ?? null });
        }
      },

      navigationHistory: [],
      historyIndex: -1,
      canGoBack: false,
      canGoForward: false,

      goBack: () => {
        const { navigationHistory, historyIndex } = get();
        if (historyIndex > 0) {
          const newIndex = historyIndex - 1;
          const entry = navigationHistory[newIndex];
          set({
            selectedNodeId: entry.nodeId,
            selectedNodeName: entry.nodeName,
            historyIndex: newIndex,
            canGoBack: newIndex > 0,
            canGoForward: true,
          });
        }
      },

      goForward: () => {
        const { navigationHistory, historyIndex } = get();
        if (historyIndex < navigationHistory.length - 1) {
          const newIndex = historyIndex + 1;
          const entry = navigationHistory[newIndex];
          set({
            selectedNodeId: entry.nodeId,
            selectedNodeName: entry.nodeName,
            historyIndex: newIndex,
            canGoBack: true,
            canGoForward: newIndex < navigationHistory.length - 1,
          });
        }
      },

      detailTab: "context",
      setDetailTab: (tab) => set({ detailTab: tab }),

      zoomLevel: "package",
      setZoomLevel: (level) => set({ zoomLevel: level }),

      searchOpen: false,
      setSearchOpen: (open) => set({ searchOpen: open }),

      searchQuery: "",
      setSearchQuery: (query) => set({ searchQuery: query }),

      settingsOpen: false,
      setSettingsOpen: (open) => set({ settingsOpen: open }),

      commandPaletteOpen: false,
      setCommandPaletteOpen: (open) => set({ commandPaletteOpen: open }),

      searchMatchIds: [],
      setSearchMatchIds: (ids) => set({ searchMatchIds: ids }),

      theme: (() => {
        const saved = localStorage.getItem("code-explorer-theme");
        return saved === "dark" || saved === "light" || saved === "system" ? saved : "dark";
      })(),
      setTheme: (theme) => {
        localStorage.setItem("code-explorer-theme", theme);
        const resolved = theme === "system"
          ? (window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light")
          : theme;
        document.documentElement.setAttribute("data-theme", resolved);
        set({ theme });
      },

      mode: 'chat',
      setMode: (mode) => set({ mode }),
      analyzeView: 'overview',
      setAnalyzeView: (view) => set({ analyzeView: view }),
      activeLens: 'all',
      setActiveLens: (lens) => set({ activeLens: lens }),
      clusterByCommunity: false,
      setClusterByCommunity: (clusterByCommunity) => set({ clusterByCommunity }),
      riskThreshold: 0.0,
      setRiskThreshold: (riskThreshold) => set({ riskThreshold }),
      egoDepth: 2,
      setEgoDepth: (depth) => set({ egoDepth: depth }),
      explorerLeftCollapsed: false,
      setExplorerLeftCollapsed: (collapsed) => set({ explorerLeftCollapsed: collapsed }),

      sidebarExpanded: true,
      setSidebarExpanded: (expanded) => set({ sidebarExpanded: expanded }),
      toggleSidebar: () => set((s) => ({ sidebarExpanded: !s.sidebarExpanded })),

      selectedFeatures: new Set<string>(),
      toggleFeature: (id) => set((s) => {
        const next = new Set(s.selectedFeatures);
        if (next.has(id)) next.delete(id); else next.add(id);
        return { selectedFeatures: next };
      }),
      resetFeatures: () => set({ selectedFeatures: new Set<string>() }),

      renameModal: { open: false },
      openRenameModal: (initialTarget) => set({ renameModal: { open: true, initialTarget } }),
      closeRenameModal: () => set({ renameModal: { open: false } }),

      notebooksOpen: false,
      setNotebooksOpen: (open) => set({ notebooksOpen: open }),

      dashboardsOpen: false,
      setDashboardsOpen: (open) => set({ dashboardsOpen: open }),

      workflowsOpen: false,
      setWorkflowsOpen: (open) => set({ workflowsOpen: open }),

      userCommandsOpen: false,
      setUserCommandsOpen: (open) => set({ userCommandsOpen: open }),

      chatSettingsOpen: false,
      setChatSettingsOpen: (open) => set({ chatSettingsOpen: open }),
    }),
    {
      name: "code-explorer-app-state",
      partialize: (state) => ({
        activeRepo: state.activeRepo,
        theme: state.theme,
        mode: state.mode,
        analyzeView: state.analyzeView,
        activeLens: state.activeLens,
        clusterByCommunity: state.clusterByCommunity,
        riskThreshold: state.riskThreshold,
        egoDepth: state.egoDepth,
        zoomLevel: state.zoomLevel,
      }),
    }
  )
);
