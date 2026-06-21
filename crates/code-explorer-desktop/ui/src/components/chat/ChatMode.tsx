/**
 * ChatMode — Full-screen chat interface for the Chat app mode.
 *
 * - Wraps ChatPanel (which manages its own message state + ChatContextBar)
 * - Provides a settings button that opens ChatSettings modal
 * - Cross-mode navigation: source node clicks navigate to Explorer
 * - No-LLM guard: shows setup card when no API key is configured
 * - No-repo guard: shows an empty state when no repo is loaded
 */

import { lazy, Suspense, useEffect } from "react";
import { useQuery } from "@tanstack/react-query";
import { MessageSquare, Settings2, ChevronDown, Compass, BookOpen, Sparkles } from "lucide-react";
import { Group, Panel } from "react-resizable-panels";
import { PanelSeparator } from "../layout/PanelSeparator";
import { ErrorBoundary } from "../shared/ErrorBoundary";
import { commands, type ChatConfig } from "../../lib/tauri-commands";
import { useI18n } from "../../hooks/use-i18n";
import { useAppStore } from "../../stores/app-store";
import { useRepos, useOpenRepo } from "../../hooks/use-tauri-query";
import { ChatPanel } from "./ChatPanel";
import { LoadingOrbs } from "../shared/LoadingOrbs";
import { useResponsive } from "../../hooks/use-responsive";
import { ChatHistorySidebar } from "./ChatHistorySidebar";
import { toast } from "sonner";

const ChatSettings = lazy(() =>
  import("./ChatSettings").then((m) => ({ default: m.ChatSettings })),
);

// ─── Repo Selector ───────────────────────────────────────────────────

function RepoSelector() {
  const { t } = useI18n();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const setActiveRepo = useAppStore((s) => s.setActiveRepo);
  const { data: repos } = useRepos();
  const openRepo = useOpenRepo();

  const handleSwitchRepo = async (e: React.ChangeEvent<HTMLSelectElement>) => {
    const name = e.target.value;
    if (!name) return;
    try {
      await openRepo.mutateAsync(name);
      setActiveRepo(name);
      toast.success(t("chat.repoSwitched").replace("{0}", name));
    } catch (err) {
      console.error(err);
      toast.error(t("chat.repoSwitchFailed").replace("{0}", String(err)));
    }
  };

  return (
    <div className="relative flex items-center">
      <select
        value={activeRepo || ""}
        onChange={handleSwitchRepo}
        className="appearance-none bg-transparent outline-none pl-2 pr-8 py-1 rounded cursor-pointer text-[14px] font-medium"
        style={{ color: "var(--text-1)", border: "1px solid var(--surface-border)" }}
      >
        <option value="" disabled>{t("chat.selectRepo")}</option>
        {repos?.map((repo) => (
          <option key={repo.name} value={repo.name}>
            {repo.name}
          </option>
        ))}
      </select>
      <ChevronDown size={14} className="absolute right-2 pointer-events-none" style={{ color: "var(--text-3)" }} />
    </div>
  );
}

function formatChatConfigLabel(config: ChatConfig | undefined): string {
  if (!config?.provider || !config.model) return "LLM non configure";
  const effort = config.reasoningEffort ? ` · ${config.reasoningEffort}` : "";
  return `${config.provider} / ${config.model}${effort}`;
}

// ─── Search capabilities banner ──────────────────────────────────────

/**
 * Surfaces a hint when the active repo doesn't have embeddings — without it,
 * the chat silently falls back to BM25-only and users have no way to know
 * they're getting a worse-than-necessary experience. Shown inline above the
 * chat panel, once per active repo, dismissible via a localStorage flag so
 * we don't nag users who deliberately skip embeddings.
 */
function SearchCapabilitiesBanner({ activeRepo }: { activeRepo: string | null }) {
  const { data, isLoading, isError } = useQuery({
    queryKey: ["chat-search-capabilities", activeRepo],
    queryFn: () => commands.chatSearchCapabilities(),
    enabled: !!activeRepo,
    retry: 1,
    staleTime: 60_000,
  });

  // Bail when no repo loaded, while loading, on error (banner is a quality
  // hint — never a blocker), or when embeddings are present.
  if (!activeRepo || isLoading || isError) return null;
  if (data?.embeddingsLoaded) return null;

  const dismissKey = `gnx-embed-banner-dismissed:${activeRepo}`;
  if (typeof window !== "undefined" && window.localStorage.getItem(dismissKey)) {
    return null;
  }

  const handleDismiss = () => {
    if (typeof window !== "undefined") {
      window.localStorage.setItem(dismissKey, "1");
    }
    // Force a re-render via reload of capabilities (cheap — backend just
    // re-checks an Option<Arc<>>).
    window.dispatchEvent(new Event("gnx-embed-banner-dismissed"));
  };

  return (
    <div
      className="shrink-0 flex items-start gap-3 px-4 py-2 text-[12px]"
      style={{
        background: "var(--warning-bg, rgba(234, 179, 8, 0.08))",
        borderBottom: "1px solid var(--warning-border, rgba(234, 179, 8, 0.2))",
        color: "var(--text-2)",
      }}
      role="status"
    >
      <Sparkles size={14} className="mt-[1px] shrink-0" style={{ color: "rgb(234, 179, 8)" }} />
      <div className="flex-1">
        <strong style={{ color: "var(--text-1)" }}>Recherche sémantique désactivée.</strong>{" "}
        Le chat utilise BM25 seul. Pour activer la recherche hybride
        (BM25 + embeddings + RRF), exécutez{" "}
        <code
          className="px-1 py-[1px] rounded font-mono text-[11px]"
          style={{ background: "var(--surface-1)", color: "var(--text-1)" }}
        >
          code-explorer embed --model ~/.codeexplorer/models/&lt;model&gt;/model.onnx --repo {activeRepo}
        </code>{" "}
        puis rechargez le dépôt.
      </div>
      <button
        onClick={handleDismiss}
        className="shrink-0 px-2 py-[2px] rounded text-[11px] hover:opacity-80"
        style={{ color: "var(--text-3)", border: "1px solid var(--surface-border)" }}
        title="Masquer ce bandeau pour ce dépôt"
      >
        Masquer
      </button>
    </div>
  );
}

// ─── Main component ──────────────────────────────────────────────────

export function ChatMode() {
  const { t } = useI18n();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const setActiveRepo = useAppStore((s) => s.setActiveRepo);
  const setMode = useAppStore((s) => s.setMode);
  const setSelectedNodeId = useAppStore((s) => s.setSelectedNodeId);
  // Settings modal state lives in the store so it can be opened from
  // anywhere (e.g., clicking the LLM model in the StatusBar while on any
  // mode, not just Chat).
  const settingsOpen = useAppStore((s) => s.chatSettingsOpen);
  const setSettingsOpen = useAppStore((s) => s.setChatSettingsOpen);
  const { isCompact } = useResponsive();
  const { data: repos } = useRepos();
  const openRepo = useOpenRepo();

  // Auto-select first repo on startup if none is active
  useEffect(() => {
    if (!activeRepo && repos && repos.length > 0) {
      const first = repos[0].name;
      openRepo.mutateAsync(first).then(() => setActiveRepo(first)).catch(() => {});
    }
  }, [repos, activeRepo, openRepo, setActiveRepo]);

  // Fetch LLM config to detect unconfigured state
  const { data: chatConfig, isLoading: configLoading } = useQuery({
    queryKey: ["chat-config"],
    queryFn: () => commands.chatGetConfig(),
    // Don't retry aggressively — missing config is not an error
    retry: 1,
  });
  const llmLabel = formatChatConfigLabel(chatConfig);

  const handleNavigateToNode = (nodeId: string) => {
    setMode("explorer");
    setSelectedNodeId(nodeId, null);
  };

  return (
    <Group orientation="horizontal" className="h-full w-full">
      {/* Sidebar: Chat history (like ChatGPT/Claude left panel) */}
      {!isCompact && (
        <>
          <Panel defaultSize="18%" minSize="12%" maxSize="28%" collapsible>
            <ErrorBoundary>
              <ChatHistorySidebar />
            </ErrorBoundary>
          </Panel>
          <PanelSeparator />
        </>
      )}

      {/* Chat Panel — full width, clean layout like ChatGPT/Claude */}
      <Panel defaultSize="82%" minSize="50%">
        <div className="flex flex-col h-full w-full" style={{ background: "var(--bg-0)" }}>
          {/* Header: Repo selector + settings button */}
          <div
            className="shrink-0 flex items-center justify-between px-4 py-2"
            style={{
              background: "var(--glass-bg)",
              backdropFilter: "blur(var(--glass-blur))",
              borderBottom: "1px solid var(--glass-border)",
            }}
          >
            <div className="flex items-center gap-3">
              <MessageSquare size={16} style={{ color: "var(--accent)" }} />
              <RepoSelector />
            </div>
            
            <div className="flex items-center gap-1">
              <button
                onClick={() => setSettingsOpen(true)}
                className="hidden sm:inline-flex max-w-[320px] items-center gap-1.5 rounded-lg px-2 py-1 text-[11px] transition-colors"
                style={{
                  color: chatConfig?.provider ? "var(--text-2)" : "var(--orange)",
                  border: "1px solid var(--surface-border)",
                  background: "var(--surface)",
                }}
                title={`LLM actif: ${llmLabel}`}
                aria-label={`LLM actif: ${llmLabel}`}
              >
                <Sparkles size={13} style={{ color: chatConfig?.provider ? "var(--accent)" : "var(--orange)" }} />
                <span className="truncate">{llmLabel}</span>
              </button>

              <button
                onClick={() => setMode("explorer")}
                className="p-1.5 rounded-lg transition-colors"
                style={{ color: "var(--text-3)", cursor: "pointer" }}
                title="Open File Explorer & Graph"
                aria-label="Open Explorer"
              >
                <Compass size={16} />
              </button>
              
              <button
                onClick={() => {
                  useAppStore.getState().setMode("manage");
                  // Depending on the implementation, opening docs might require dispatching an event
                  // or just navigating to manage. The manage mode defaults to showing tabs.
                  // (Assuming Manage Mode handles the "docs" tab selection if implemented or user can click it)
                }}
                className="p-1.5 rounded-lg transition-colors"
                style={{ color: "var(--text-3)", cursor: "pointer" }}
                title="Open Documentation"
                aria-label="Open Documentation"
              >
                <BookOpen size={16} />
              </button>

              <div style={{ width: 1, height: 16, background: "var(--surface-border)", margin: "0 4px" }} />

              <button
                onClick={() => setSettingsOpen(true)}
                className="p-1.5 rounded-lg transition-colors"
                style={{ color: "var(--text-3)", cursor: "pointer" }}
                title="Chat AI Settings"
                aria-label="Open chat AI settings"
              >
                <Settings2 size={16} />
              </button>
            </div>
          </div>

          {/* Search-capability hint — only shown when embeddings are missing. */}
          <SearchCapabilitiesBanner activeRepo={activeRepo ?? null} />

          {/* Main content area */}
          <div className="flex-1 min-h-0">
            {configLoading ? (
              <div className="flex items-center justify-center h-full">
                <div className="pulse-subtle" style={{ color: "var(--text-3)", fontSize: 13 }}>
                  {t("chat.loadingConfig")}
                </div>
              </div>
            ) : (
              <ChatPanel
                onOpenSettings={() => setSettingsOpen(true)}
                onNavigateToNode={handleNavigateToNode}
              />
            )}
          </div>

          {/* Settings modal */}
          {settingsOpen && (
            <Suspense
              fallback={
                <div className="fixed inset-0 z-50 flex items-center justify-center">
                  <LoadingOrbs />
                </div>
              }
            >
              <ChatSettings onClose={() => setSettingsOpen(false)} />
            </Suspense>
          )}
        </div>
      </Panel>
    </Group>
  );
}
