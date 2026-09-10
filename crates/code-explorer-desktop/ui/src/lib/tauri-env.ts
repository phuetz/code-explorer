/**
 * Tauri runtime detection utilities.
 * Uses real Tauri IPC when available, graceful empty defaults otherwise.
 */

/** Returns true when running inside the Tauri webview (IPC available) */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** Minimal empty defaults for commands when not in Tauri (browser dev mode). */
const BROWSER_DEFAULTS: Record<string, unknown> = {
  list_repos: [],
  get_graph_data: { nodes: [], edges: [], stats: { nodeCount: 0, edgeCount: 0, communityCount: 0, processCount: 0 } },
  search_symbols: [],
  chat_get_config: { provider: "", apiKey: "", baseUrl: "", model: "gpt-4", maxTokens: 4096, reasoningEffort: "" },
  get_hotspots: [],
  get_coupling: [],
  get_ownership: [],
  get_coverage: { total: 0, traced: 0, dead: 0, coverage: 0, methods: [] },
  get_process_flows: [],
  get_health_report: null,
};

/**
 * Invoke wrapper — calls real Tauri IPC when available.
 * In browser mode, returns safe empty defaults to keep the UI functional.
 */
export async function safeInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) {
    if (cmd in BROWSER_DEFAULTS) {
      return structuredClone(BROWSER_DEFAULTS[cmd]) as T;
    }
    // For unknown commands, return empty array as safe default
    console.warn(`[Code Explorer] Browser mode — no default for "${cmd}", returning []`);
    return [] as unknown as T;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}
