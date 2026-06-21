import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { commands } from "../lib/tauri-commands";
import type { GraphFilter } from "../lib/tauri-commands";
import { useAppStore } from "../stores/app-store";

// All graph/file/symbol queries scope their cache keys on `activeRepo`.
// Although `useOpenRepo` below already calls `removeQueries()` to wipe the
// cache on repo switch, scoping by `activeRepo` provides defense-in-depth
// for paths that change the active repo without going through that mutation
// hook (e.g. initial load from persisted localStorage state).

export function useRepos() {
  return useQuery({
    queryKey: ["repos"],
    queryFn: () => commands.listRepos(),
  });
}

export function useOpenRepo() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (name: string) => commands.openRepo(name),
    onSuccess: () => {
      // `invalidateQueries()` only marks entries stale; with `staleTime: Infinity`
      // any active subscriber would still render the *previous* repo's data for
      // one tick before the refetch lands, causing a flash of the wrong graph
      // and burning a layout pass on stale nodes. `removeQueries()` drops the
      // cache entirely so the next render goes through `queryFn` against the
      // freshly opened repo.
      // Only clear graph/analysis data — preserve chat config and other caches
      queryClient.removeQueries({ predicate: (q) => {
        const key = q.queryKey[0];
        return key === "graph" || key === "subgraph" || key === "features"
          || key === "hotspots" || key === "coupling" || key === "ownership"
          || key === "health" || key === "coverage" || key === "fileTree"
          || key === "processFlows" || key === "docIndex";
      }});
    },
  });
}

export function useGraphData(filter: GraphFilter, enabled: boolean = true) {
  const activeRepo = useAppStore((s) => s.activeRepo);
  return useQuery({
    queryKey: ["graph", activeRepo, filter],
    queryFn: () => commands.getGraphData(filter),
    enabled,
    staleTime: Infinity,
    // Bound retention so cycling through zoom levels doesn't pin a stale graph
    // dataset per level forever (each can be tens of thousands of serialized
    // nodes on large repos).
    gcTime: 2 * 60 * 1000,
  });
}

export function useSubgraph(centerNodeId: string | null, depth?: number) {
  const activeRepo = useAppStore((s) => s.activeRepo);
  return useQuery({
    queryKey: ["subgraph", activeRepo, centerNodeId, depth],
    queryFn: () => commands.getSubgraph(centerNodeId!, depth),
    enabled: !!centerNodeId,
    staleTime: Infinity,
    gcTime: 2 * 60 * 1000,
  });
}

export function useSearchSymbols(query: string, enabled: boolean = true) {
  const activeRepo = useAppStore((s) => s.activeRepo);
  return useQuery({
    queryKey: ["search", activeRepo, query],
    queryFn: () => commands.searchSymbols(query),
    enabled: enabled && query.length > 0,
    staleTime: 30_000,
  });
}

export function useSymbolContext(nodeId: string | null) {
  const activeRepo = useAppStore((s) => s.activeRepo);
  return useQuery({
    queryKey: ["context", activeRepo, nodeId],
    queryFn: () => commands.getSymbolContext(nodeId!),
    enabled: !!nodeId,
    staleTime: Infinity,
  });
}

export function useImpactAnalysis(
  targetId: string | null,
  direction?: string,
  maxDepth?: number
) {
  const activeRepo = useAppStore((s) => s.activeRepo);
  return useQuery({
    queryKey: ["impact", activeRepo, targetId, direction, maxDepth],
    queryFn: () => commands.getImpactAnalysis(targetId!, direction, maxDepth),
    enabled: !!targetId,
    staleTime: Infinity,
  });
}

export function useFileTree(enabled: boolean = true) {
  const activeRepo = useAppStore((s) => s.activeRepo);
  return useQuery({
    queryKey: ["file-tree", activeRepo],
    queryFn: () => commands.getFileTree(),
    enabled,
    staleTime: Infinity,
  });
}

export function useFileContent(
  filePath: string | null,
  startLine?: number,
  endLine?: number
) {
  const activeRepo = useAppStore((s) => s.activeRepo);
  return useQuery({
    queryKey: ["file-content", activeRepo, filePath, startLine, endLine],
    queryFn: () => commands.readFileContent(filePath!, startLine, endLine),
    enabled: !!filePath,
    staleTime: Infinity,
  });
}
