/**
 * Chat Intelligence Store — Zustand state for the smart chat system.
 *
 * Manages:
 * - Context filters (files, symbols, modules, languages)
 * - Active research plan and step execution state
 * - Filter modal visibility
 * - Quick-pick search state
 */

import { create } from "zustand";
import type {
  ChatContextFilter,
  ResearchPlan,
  QueryAnalysis,
  FileQuickPick,
  SymbolQuickPick,
  ModuleQuickPick,
} from "../lib/tauri-commands";

// ─── Types ───────────────────────────────────────────────────────────

export type FilterModalType = "files" | "symbols" | "modules" | null;

/** Chat interaction mode. Only one is active at a time. */
export type ChatMode =
  | "qa"
  | "deep_research"
  | "feature_dev"
  | "code_review"
  | "simplify";

interface ChatStoreState {
  // ── Context Filters ──────────────────────────────────────────
  filters: ChatContextFilter;
  setFilters: (filters: ChatContextFilter) => void;
  addFileFilter: (file: string) => void;
  removeFileFilter: (file: string) => void;
  addSymbolFilter: (symbol: string) => void;
  removeSymbolFilter: (symbol: string) => void;
  addModuleFilter: (module: string) => void;
  removeModuleFilter: (module: string) => void;
  addLanguageFilter: (lang: string) => void;
  removeLanguageFilter: (lang: string) => void;
  clearFilters: () => void;
  hasActiveFilters: () => boolean;

  // ── Filter Modals ────────────────────────────────────────────
  activeModal: FilterModalType;
  openModal: (modal: FilterModalType) => void;
  closeModal: () => void;

  // ── Research Plan ────────────────────────────────────────────
  activePlan: ResearchPlan | null;
  setActivePlan: (plan: ResearchPlan | null) => void;
  updatePlanStep: (stepId: string, update: Partial<ResearchPlan["steps"][0]>) => void;

  // ── Query Analysis ───────────────────────────────────────────
  lastAnalysis: QueryAnalysis | null;
  setLastAnalysis: (analysis: QueryAnalysis | null) => void;

  // ── Chat Mode (exclusive) ────────────────────────────────────
  chatMode: ChatMode;
  setChatMode: (mode: ChatMode) => void;

  // ── Pending question (dispatched from elsewhere — e.g. context menu) ──
  // ChatPanel consumes & clears on mount and on subsequent changes.
  pendingQuestion: { mode: ChatMode; text: string; autoSend?: boolean } | null;
  dispatchQuestion: (mode: ChatMode, text: string, autoSend?: boolean) => void;
  clearPendingQuestion: () => void;

  // ── Deep Research Mode (derived from chatMode for back-compat) ──
  deepResearchEnabled: boolean;
  toggleDeepResearch: () => void;
  setDeepResearch: (enabled: boolean) => void;

  // ── Plan Panel Visibility ────────────────────────────────────
  planPanelOpen: boolean;
  setPlanPanelOpen: (open: boolean) => void;

  // ── Quick Pick Results (cached for display) ──────────────────
  filePickResults: FileQuickPick[];
  setFilePickResults: (results: FileQuickPick[]) => void;
  symbolPickResults: SymbolQuickPick[];
  setSymbolPickResults: (results: SymbolQuickPick[]) => void;
  modulePickResults: ModuleQuickPick[];
  setModulePickResults: (results: ModuleQuickPick[]) => void;
}

// ─── Default Filter ──────────────────────────────────────────────────

const emptyFilter: ChatContextFilter = {
  files: [],
  symbols: [],
  modules: [],
  languages: [],
  labels: [],
};

// ─── Store ───────────────────────────────────────────────────────────

export const useChatStore = create<ChatStoreState>((set, get) => ({
  // ── Context Filters ──────────────────────────────────────────
  filters: { ...emptyFilter },
  setFilters: (filters) => set({ filters }),

  addFileFilter: (file) =>
    set((s) => ({
      filters: {
        ...s.filters,
        files: s.filters.files.includes(file)
          ? s.filters.files
          : [...s.filters.files, file],
      },
    })),

  removeFileFilter: (file) =>
    set((s) => ({
      filters: {
        ...s.filters,
        files: s.filters.files.filter((f) => f !== file),
      },
    })),

  addSymbolFilter: (symbol) =>
    set((s) => ({
      filters: {
        ...s.filters,
        symbols: s.filters.symbols.includes(symbol)
          ? s.filters.symbols
          : [...s.filters.symbols, symbol],
      },
    })),

  removeSymbolFilter: (symbol) =>
    set((s) => ({
      filters: {
        ...s.filters,
        symbols: s.filters.symbols.filter((sym) => sym !== symbol),
      },
    })),

  addModuleFilter: (module) =>
    set((s) => ({
      filters: {
        ...s.filters,
        modules: s.filters.modules.includes(module)
          ? s.filters.modules
          : [...s.filters.modules, module],
      },
    })),

  removeModuleFilter: (module) =>
    set((s) => ({
      filters: {
        ...s.filters,
        modules: s.filters.modules.filter((m) => m !== module),
      },
    })),

  addLanguageFilter: (lang) =>
    set((s) => ({
      filters: {
        ...s.filters,
        languages: s.filters.languages.includes(lang)
          ? s.filters.languages
          : [...s.filters.languages, lang],
      },
    })),

  removeLanguageFilter: (lang) =>
    set((s) => ({
      filters: {
        ...s.filters,
        languages: s.filters.languages.filter((l) => l !== lang),
      },
    })),

  clearFilters: () => set({ filters: { ...emptyFilter } }),

  hasActiveFilters: () => {
    const { filters } = get();
    return (
      filters.files.length > 0 ||
      filters.symbols.length > 0 ||
      filters.modules.length > 0 ||
      filters.languages.length > 0 ||
      filters.labels.length > 0
    );
  },

  // ── Filter Modals ────────────────────────────────────────────
  activeModal: null,
  openModal: (modal) => set({ activeModal: modal }),
  closeModal: () => set({ activeModal: null }),

  // ── Research Plan ────────────────────────────────────────────
  activePlan: null,
  setActivePlan: (plan) =>
    set({ activePlan: plan, planPanelOpen: plan !== null }),

  updatePlanStep: (stepId, update) =>
    set((s) => {
      if (!s.activePlan) return {};
      return {
        activePlan: {
          ...s.activePlan,
          steps: s.activePlan.steps.map((step) =>
            step.id === stepId ? { ...step, ...update } : step
          ),
        },
      };
    }),

  // ── Query Analysis ───────────────────────────────────────────
  lastAnalysis: null,
  setLastAnalysis: (analysis) => set({ lastAnalysis: analysis }),

  // ── Chat Mode ────────────────────────────────────────────────
  chatMode: "qa" as ChatMode,
  setChatMode: (mode) =>
    set({
      chatMode: mode,
      // Keep the legacy flag in sync so existing readers still work.
      deepResearchEnabled: mode === "deep_research",
    }),

  // ── Pending question dispatch ────────────────────────────────
  pendingQuestion: null,
  dispatchQuestion: (mode, text, autoSend) =>
    set({
      chatMode: mode,
      deepResearchEnabled: mode === "deep_research",
      pendingQuestion: { mode, text, autoSend },
    }),
  clearPendingQuestion: () => set({ pendingQuestion: null }),

  // ── Deep Research Mode (back-compat shims) ───────────────────
  deepResearchEnabled: false,
  toggleDeepResearch: () =>
    set((s) => {
      const next = s.chatMode === "deep_research" ? "qa" : "deep_research";
      return {
        chatMode: next as ChatMode,
        deepResearchEnabled: next === "deep_research",
      };
    }),
  setDeepResearch: (enabled) =>
    set({
      chatMode: enabled ? "deep_research" : "qa",
      deepResearchEnabled: enabled,
    }),

  // ── Plan Panel Visibility ────────────────────────────────────
  planPanelOpen: false,
  setPlanPanelOpen: (open) => set({ planPanelOpen: open }),

  // ── Quick Pick Results ───────────────────────────────────────
  filePickResults: [],
  setFilePickResults: (results) => set({ filePickResults: results }),
  symbolPickResults: [],
  setSymbolPickResults: (results) => set({ symbolPickResults: results }),
  modulePickResults: [],
  setModulePickResults: (results) => set({ modulePickResults: results }),
}));
