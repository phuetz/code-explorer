/**
 * SavedViewsMenu — Theme C dropdown that lives in the GraphToolbar.
 *
 * Two responsibilities:
 *   1. List the user's saved views and let them apply / delete one.
 *   2. Save the current graph configuration (lens + filters + camera +
 *      manual selection) as a new named view.
 *
 * The UI deliberately stays small (no modal): a popover with a list and a
 * compact "Save current view" form. Persistence is handled by the
 * `useSavedViewsStore` Zustand store, which round-trips through Tauri.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { Bookmark, Trash2, ChevronDown, Save, Loader2 } from "lucide-react";
import { toast } from "sonner";
import {
  useSavedViewsStore,
  selectViewsForRepo,
} from "../../stores/saved-views-store";
import { useAppStore } from "../../stores/app-store";
import type { CameraState, SavedView } from "../../lib/tauri-commands";

export interface SavedViewsMenuProps {
  /** Capture the current graph state when the user clicks "Save". */
  collectCurrentState: () => {
    name?: string;
    lens?: string;
    filters?: unknown;
    cameraState?: CameraState;
    nodeSelection?: string[];
  };
  /** Apply a saved view. The parent re-applies camera/filters/lens. */
  onApplyView: (view: SavedView) => void;
}

export function SavedViewsMenu({ collectCurrentState, onApplyView }: SavedViewsMenuProps) {
  const activeRepo = useAppStore((s) => s.activeRepo);
  const [open, setOpen] = useState(false);
  const [savingName, setSavingName] = useState("");
  const [showSaveInput, setShowSaveInput] = useState(false);
  const popoverRef = useRef<HTMLDivElement | null>(null);

  const views = useSavedViewsStore((s) => s.views);
  const loading = useSavedViewsStore((s) => s.loading);
  const error = useSavedViewsStore((s) => s.error);
  const reload = useSavedViewsStore((s) => s.reload);
  const saveView = useSavedViewsStore((s) => s.save);
  const removeView = useSavedViewsStore((s) => s.remove);
  const draft = useSavedViewsStore((s) => s.draft);

  // Hydrate on mount + whenever the active repo changes.
  useEffect(() => {
    if (activeRepo) {
      reload().catch(() => {
        // surfacing is handled via `error` selector; no toast spam on mount
      });
    }
  }, [activeRepo, reload]);

  // Click-outside to close.
  useEffect(() => {
    if (!open) return;
    const onDocClick = (e: MouseEvent) => {
      if (popoverRef.current && !popoverRef.current.contains(e.target as Node)) {
        setOpen(false);
        setShowSaveInput(false);
      }
    };
    document.addEventListener("mousedown", onDocClick);
    return () => document.removeEventListener("mousedown", onDocClick);
  }, [open]);

  const repoViews = useMemo(
    () => selectViewsForRepo({ views, loading, error, reload, save: saveView, remove: removeView, draft }, activeRepo),
    [views, loading, error, reload, saveView, removeView, draft, activeRepo],
  );

  const handleSave = async () => {
    const trimmed = savingName.trim();
    if (!trimmed) {
      toast.error("Please enter a name for this view.");
      return;
    }
    const snapshot = collectCurrentState();
    const view = draft({
      name: trimmed,
      repo: activeRepo ?? undefined,
      lens: snapshot.lens,
      filters: snapshot.filters,
      cameraState: snapshot.cameraState,
      nodeSelection: snapshot.nodeSelection,
    });
    try {
      await saveView(view);
      toast.success(`View "${trimmed}" saved`);
      setSavingName("");
      setShowSaveInput(false);
    } catch (e) {
      toast.error(`Failed to save view: ${(e as Error).message}`);
    }
  };

  const handleApply = (view: SavedView) => {
    onApplyView(view);
    setOpen(false);
    toast.success(`Applied view "${view.name}"`);
  };

  const handleDelete = async (view: SavedView, e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await removeView(view.id);
      toast.success(`Deleted "${view.name}"`);
    } catch (err) {
      toast.error(`Failed to delete: ${(err as Error).message}`);
    }
  };

  return (
    <div className="relative" ref={popoverRef}>
      <button
        onClick={() => setOpen((v) => !v)}
        title="Saved graph views"
        aria-label="Saved graph views"
        aria-expanded={open}
        className="flex items-center gap-1.5 px-2.5 py-1.5 text-xs font-medium rounded-md transition-all hover:bg-surface-hover bg-surface text-text-2 border border-surface-border cursor-pointer"
      >
        <Bookmark size={13} />
        Views
        <span className="text-[10px] text-text-3 font-semibold">{repoViews.length}</span>
        <ChevronDown
          size={12}
          className={`transition-transform duration-150 ${open ? "rotate-180" : "rotate-0"}`}
        />
      </button>

      {open && (
        <div
          className="absolute left-0 top-full mt-1 z-50 min-w-[280px] max-h-[420px] overflow-auto rounded-md border border-surface-border bg-surface shadow-lg"
          role="menu"
        >
          {/* Save current view */}
          <div className="border-b border-surface-border p-2">
            {showSaveInput ? (
              <div className="flex flex-col gap-1.5">
                <input
                  autoFocus
                  type="text"
                  value={savingName}
                  onChange={(e) => setSavingName(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") handleSave();
                    if (e.key === "Escape") {
                      setShowSaveInput(false);
                      setSavingName("");
                    }
                  }}
                  placeholder="View name…"
                  className="w-full px-2 py-1 text-xs bg-bg-2 border border-surface-border rounded outline-none focus:border-accent text-text-1"
                />
                <div className="flex gap-1.5">
                  <button
                    onClick={handleSave}
                    disabled={loading}
                    className="flex-1 flex items-center justify-center gap-1 px-2 py-1 rounded bg-accent text-white text-[11px] font-semibold cursor-pointer disabled:opacity-60"
                  >
                    {loading ? <Loader2 size={11} className="animate-spin" /> : <Save size={11} />}
                    Save
                  </button>
                  <button
                    onClick={() => {
                      setShowSaveInput(false);
                      setSavingName("");
                    }}
                    className="px-2 py-1 rounded text-[11px] font-semibold border border-surface-border bg-transparent text-text-3 cursor-pointer hover:bg-surface-hover"
                  >
                    Cancel
                  </button>
                </div>
              </div>
            ) : (
              <button
                onClick={() => setShowSaveInput(true)}
                className="w-full flex items-center justify-center gap-1.5 px-2 py-1.5 rounded text-[11px] font-semibold bg-accent-subtle text-accent border border-accent cursor-pointer hover:opacity-90"
              >
                <Save size={12} /> Save current view
              </button>
            )}
          </div>

          {/* List */}
          {repoViews.length === 0 ? (
            <div className="p-3 text-[11px] text-text-3 text-center">
              No saved views{activeRepo ? "" : " — open a repo first"}.
            </div>
          ) : (
            <ul className="py-1">
              {repoViews.map((v) => (
                <li key={v.id}>
                  <button
                    onClick={() => handleApply(v)}
                    className="w-full flex items-center gap-2 px-2.5 py-1.5 text-left text-xs hover:bg-surface-hover cursor-pointer text-text-1 border-none bg-transparent"
                    role="menuitem"
                  >
                    <Bookmark size={11} className="text-text-3" />
                    <div className="flex-1 min-w-0">
                      <div className="font-medium truncate">{v.name}</div>
                      <div className="text-[9px] text-text-3 mt-0.5">
                        {v.lens ? `${v.lens} lens · ` : ""}
                        {v.nodeSelection.length > 0 ? `${v.nodeSelection.length} pinned · ` : ""}
                        {new Date(v.updatedAt || v.createdAt).toLocaleDateString()}
                      </div>
                    </div>
                    <button
                      onClick={(e) => handleDelete(v, e)}
                      className="p-1 rounded hover:bg-surface-hover text-text-3 hover:text-rose cursor-pointer"
                      aria-label={`Delete view ${v.name}`}
                      title="Delete view"
                    >
                      <Trash2 size={10} />
                    </button>
                  </button>
                </li>
              ))}
            </ul>
          )}

          {error && (
            <div className="p-2 text-[10px] text-rose border-t border-surface-border bg-bg-2">
              {error}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
