import { ChevronRight, FolderOpen, Box, FileCode, Network } from "lucide-react";
import { useAppStore } from "../../stores/app-store";
import { useQuery } from "@tanstack/react-query";
import { commands } from "../../lib/tauri-commands";

/**
 * Breadcrumb showing the hierarchy of the selected symbol:
 * Community > File > Class/Container > Symbol
 */
export function SymbolBreadcrumb() {
  const selectedNodeId = useAppStore((s) => s.selectedNodeId);
  const setSelectedNodeId = useAppStore((s) => s.setSelectedNodeId);
  const activeRepo = useAppStore((s) => s.activeRepo);

  // Scope by `activeRepo` so that a selected node ID persisted in the store
  // across a repo switch doesn't return the previous repo's cached context.
  // Two different repos can theoretically produce the same qualified node id.
  const { data: context } = useQuery({
    queryKey: ["symbol-context", activeRepo, selectedNodeId],
    queryFn: () => commands.getSymbolContext(selectedNodeId!),
    enabled: !!selectedNodeId,
    staleTime: 30_000,
  });

  if (!selectedNodeId || !context) return null;

  const crumbs: { label: string; icon: React.ReactNode; nodeId?: string }[] = [];

  // Community (if available)
  if (context.community) {
    crumbs.push({
      label: context.community.name,
      icon: <Network size={11} />,
      nodeId: context.community.id,
    });
  }

  // File
  if (context.node.filePath) {
    const fileName = context.node.filePath.split("/").pop() || context.node.filePath;
    crumbs.push({
      label: fileName,
      icon: <FileCode size={11} />,
    });
  }

  // Container class (from callers that have HasMethod edge to this symbol)
  if (context.callers) {
    const container = context.callers.find(
      (c) => c.label === "Class" || c.label === "Controller" || c.label === "Service" || c.label === "Interface"
    );
    if (container) {
      crumbs.push({
        label: container.name,
        icon: <Box size={11} />,
        nodeId: container.id,
      });
    }
  }

  // Current symbol (always last)
  crumbs.push({
    label: context.node.name,
    icon: <FolderOpen size={11} />,
  });

  if (crumbs.length <= 1) return null;

  return (
    <nav
      aria-label="Symbol breadcrumb"
      className="flex items-center text-[11px] overflow-hidden"
      style={{ gap: 2, minHeight: 24, color: "var(--text-3)" }}
    >
      {crumbs.map((crumb, i) => (
        <span key={i} className="flex items-center" style={{ gap: 2 }}>
          {i > 0 && <ChevronRight size={10} style={{ color: "var(--text-4)", flexShrink: 0 }} />}
          {crumb.nodeId ? (
            <button
              onClick={() => setSelectedNodeId(crumb.nodeId!, crumb.label)}
              className="flex items-center gap-1 rounded px-1 py-0.5 transition-colors truncate hover:brightness-110"
              title={crumb.label}
              style={{
                color: "var(--text-2)",
                background: "transparent",
                border: "none",
                cursor: "pointer",
                maxWidth: 140,
              }}
            >
              {crumb.icon}
              <span className="truncate">{crumb.label}</span>
            </button>
          ) : (
            <span
              className="flex items-center gap-1 truncate"
              title={crumb.label}
              style={{
                color: i === crumbs.length - 1 ? "var(--text-0)" : "var(--text-2)",
                fontWeight: i === crumbs.length - 1 ? 600 : 400,
                maxWidth: 160,
              }}
            >
              {crumb.icon}
              <span className="truncate">{crumb.label}</span>
            </span>
          )}
        </span>
      ))}
    </nav>
  );
}
