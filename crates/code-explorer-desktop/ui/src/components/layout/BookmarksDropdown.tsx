/**
 * BookmarksDropdown — popover button listing all bookmarks for the active repo.
 *
 * Sits in the StatusBar (or any chrome). Clicking a bookmark navigates to
 * the corresponding node in Explorer.
 */

import { useState, useRef, useEffect } from "react";
import { Bookmark as BookmarkIcon, X } from "lucide-react";
import { useQuery, useQueryClient, useMutation } from "@tanstack/react-query";
import { commands } from "../../lib/tauri-commands";
import { useAppStore } from "../../stores/app-store";

export function BookmarksDropdown() {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const queryClient = useQueryClient();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const setSelectedNodeId = useAppStore((s) => s.setSelectedNodeId);
  const setMode = useAppStore((s) => s.setMode);

  const { data: bookmarks = [] } = useQuery({
    queryKey: ["bookmarks", activeRepo],
    queryFn: () => commands.bookmarksList(),
    enabled: !!activeRepo,
    staleTime: 30_000,
  });

  const removeMut = useMutation({
    mutationFn: (nodeId: string) => commands.bookmarksRemove(nodeId),
    onSuccess: (next) => queryClient.setQueryData(["bookmarks", activeRepo], next),
  });

  // Close on click outside
  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  if (!activeRepo) return null;

  return (
    <div ref={ref} style={{ position: "relative" }}>
      <button
        onClick={() => setOpen((v) => !v)}
        title={`${bookmarks.length} bookmark(s)`}
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: 4,
          padding: "2px 8px",
          background: "transparent",
          border: "1px solid var(--surface-border)",
          borderRadius: 6,
          cursor: "pointer",
          color: bookmarks.length > 0 ? "var(--amber)" : "var(--text-3)",
          fontSize: 11,
          fontFamily: "inherit",
        }}
      >
        <BookmarkIcon size={11} />
        <span>{bookmarks.length}</span>
      </button>

      {open && (
        <div
          style={{
            position: "absolute",
            bottom: "calc(100% + 6px)",
            right: 0,
            width: 280,
            maxHeight: 360,
            overflow: "auto",
            background: "var(--bg-2)",
            border: "1px solid var(--surface-border)",
            borderRadius: 8,
            boxShadow: "var(--shadow-lg)",
            zIndex: 1000,
          }}
        >
          <div
            style={{
              padding: "8px 12px",
              borderBottom: "1px solid var(--surface-border)",
              fontSize: 11,
              fontWeight: 600,
              textTransform: "uppercase",
              color: "var(--text-3)",
            }}
          >
            Bookmarks
          </div>
          {bookmarks.length === 0 ? (
            <div style={{ padding: "12px", fontSize: 11, color: "var(--text-3)" }}>
              No bookmarks yet. Click the star next to a node to add one.
            </div>
          ) : (
            bookmarks.map((b) => (
              <div
                key={b.nodeId}
                style={{
                  display: "flex",
                  alignItems: "center",
                  padding: "6px 12px",
                  borderBottom: "1px solid var(--surface-border)",
                }}
              >
                <button
                  onClick={() => {
                    setSelectedNodeId(b.nodeId, b.name);
                    setMode("explorer");
                    setOpen(false);
                  }}
                  style={{
                    flex: 1,
                    background: "transparent",
                    border: "none",
                    textAlign: "left",
                    cursor: "pointer",
                    color: "var(--text-1)",
                    fontFamily: "inherit",
                    padding: 0,
                  }}
                >
                  <div style={{ fontSize: 12, fontWeight: 600 }}>{b.name}</div>
                  <div style={{ fontSize: 10, color: "var(--text-3)" }}>
                    {b.label}
                    {b.filePath ? ` · ${b.filePath}` : ""}
                  </div>
                </button>
                <button
                  onClick={() => removeMut.mutate(b.nodeId)}
                  title="Remove bookmark"
                  style={{
                    padding: 4,
                    background: "transparent",
                    border: "none",
                    color: "var(--text-3)",
                    cursor: "pointer",
                  }}
                >
                  <X size={11} />
                </button>
              </div>
            ))
          )}
        </div>
      )}
    </div>
  );
}
