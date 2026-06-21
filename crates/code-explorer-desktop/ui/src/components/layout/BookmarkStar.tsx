/**
 * BookmarkStar — toggle the bookmark state of the currently selected node.
 *
 * Lives in the DetailPanel header; reads selectedNode + active repo from
 * the app store, hits the per-repo bookmarks store for persistence.
 */

import { Star } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "../../lib/tauri-commands";
import { useAppStore } from "../../stores/app-store";
import { toast } from "sonner";

export function BookmarkStar() {
  const queryClient = useQueryClient();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const selectedNodeId = useAppStore((s) => s.selectedNodeId);
  const selectedNodeName = useAppStore((s) => s.selectedNodeName);

  const { data: bookmarks = [] } = useQuery({
    queryKey: ["bookmarks", activeRepo],
    queryFn: () => commands.bookmarksList(),
    enabled: !!activeRepo,
    staleTime: 30_000,
  });
  const isBookmarked = !!selectedNodeId && bookmarks.some((b) => b.nodeId === selectedNodeId);

  const addMut = useMutation({
    mutationFn: () =>
      commands.bookmarksAdd({
        nodeId: selectedNodeId!,
        name: selectedNodeName ?? selectedNodeId!,
        // Label inferred from the id prefix (Function:..., Class:..., …).
        label: selectedNodeId!.split(":")[0] ?? "Node",
        filePath: undefined,
        note: undefined,
        createdAt: Date.now(),
      }),
    onSuccess: (next) => {
      queryClient.setQueryData(["bookmarks", activeRepo], next);
      toast.success("Bookmarked");
    },
    onError: (e) => toast.error(`Bookmark failed: ${(e as Error).message}`),
  });
  const removeMut = useMutation({
    mutationFn: () => commands.bookmarksRemove(selectedNodeId!),
    onSuccess: (next) => {
      queryClient.setQueryData(["bookmarks", activeRepo], next);
      toast.success("Bookmark removed");
    },
    onError: (e) => toast.error(`Remove failed: ${(e as Error).message}`),
  });

  if (!selectedNodeId) return null;

  const toggle = () => (isBookmarked ? removeMut.mutate() : addMut.mutate());

  return (
    <button
      onClick={toggle}
      title={isBookmarked ? "Remove bookmark" : "Bookmark this node"}
      aria-label={isBookmarked ? "Remove bookmark" : "Add bookmark"}
      style={{
        padding: 4,
        background: "transparent",
        border: "none",
        cursor: "pointer",
        color: isBookmarked ? "var(--amber)" : "var(--text-3)",
        display: "inline-flex",
        alignItems: "center",
      }}
    >
      <Star size={14} fill={isBookmarked ? "var(--amber)" : "transparent"} />
    </button>
  );
}
