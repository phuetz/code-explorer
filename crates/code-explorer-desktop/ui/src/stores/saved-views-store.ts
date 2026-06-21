/**
 * Saved Graph Views Store — Theme C (Graph Time-Travel & Saved Views).
 *
 * A "saved view" captures the entire visual configuration the user assembled
 * to reach an interesting picture of the graph: lens, filters, camera, manual
 * node selection. Restoring a view re-applies all of those at once so the
 * user lands on the same picture in one click.
 *
 * Persistence is delegated to the Rust side (`saved_views.rs`, JSON in
 * `<storage>/saved_views.json`) so views survive app restarts and follow the
 * `.codeexplorer/` repo bundle. The Zustand store keeps a hot copy in memory and
 * exposes a couple of convenience hooks for components.
 *
 * Pattern mirrors the chat store (no `persist` middleware here — backend is
 * the source of truth, the store just hydrates/refreshes from IPC).
 */

import { create } from "zustand";
import { commands, type SavedView, type CameraState } from "../lib/tauri-commands";

interface SavedViewsState {
  views: SavedView[];
  loading: boolean;
  error: string | null;

  /** Refresh the cached list from the backend. Call on repo open / save / delete. */
  reload: () => Promise<void>;

  /** Persist a new or existing view (upsert by id). */
  save: (view: SavedView) => Promise<void>;

  /** Delete by id. */
  remove: (id: string) => Promise<void>;

  /** Helper: build a new SavedView with a generated id + current timestamp. */
  draft: (params: {
    name: string;
    repo?: string;
    lens?: string;
    filters?: unknown;
    cameraState?: CameraState;
    nodeSelection?: string[];
    description?: string;
  }) => SavedView;
}

function genId(): string {
  // Crypto-quality enough for client-side ids; backend stores opaque strings.
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `view_${Date.now()}_${Math.random().toString(36).slice(2, 10)}`;
}

export const useSavedViewsStore = create<SavedViewsState>((set) => ({
  views: [],
  loading: false,
  error: null,

  reload: async () => {
    set({ loading: true, error: null });
    try {
      const views = await commands.savedViewsList();
      set({ views, loading: false });
    } catch (e) {
      set({ loading: false, error: (e as Error).message ?? String(e) });
    }
  },

  save: async (view) => {
    set({ loading: true, error: null });
    try {
      const next = await commands.savedViewsSave(view);
      set({ views: next, loading: false });
    } catch (e) {
      set({ loading: false, error: (e as Error).message ?? String(e) });
      throw e;
    }
  },

  remove: async (id) => {
    set({ loading: true, error: null });
    try {
      const next = await commands.savedViewsDelete(id);
      set({ views: next, loading: false });
    } catch (e) {
      set({ loading: false, error: (e as Error).message ?? String(e) });
      throw e;
    }
  },

  draft: ({ name, repo, lens, filters, cameraState, nodeSelection, description }) => {
    const now = Date.now();
    return {
      id: genId(),
      repo,
      name,
      lens,
      filters,
      cameraState,
      nodeSelection: nodeSelection ?? [],
      description,
      createdAt: now,
      updatedAt: now,
    };
  },
}));

/** Convenience: select only the views for a given repo (or unscoped). */
export function selectViewsForRepo(state: SavedViewsState, repo: string | null): SavedView[] {
  if (!repo) return state.views.filter((v) => !v.repo);
  return state.views.filter((v) => !v.repo || v.repo === repo);
}
