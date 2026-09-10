/**
 * DocsViewer — DeepWiki-style documentation viewer with integrated Q&A chat.
 *
 * Layout:
 *  ┌──────────┬───────────────────────────────┐
 *  │ DocsNav  │ DocsContent / ChatPanel        │
 *  │ (tree)   │ (Markdown + Mermaid)          │
 *  │          │                               │
 *  │  Ask    │ [Chat collapsed at bottom]     │
 *  └──────────┴───────────────────────────────┘
 */

import { lazy, Suspense, useState, useCallback } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { BookOpen, RefreshCw, Sparkles, MessageSquare, Package, Link2, FileText as FileTextIcon, MessageCircle } from "lucide-react";
import { commands, type RepoInfo } from "../../lib/tauri-commands";
import { useAppStore } from "../../stores/app-store";
import { useI18n } from "../../hooks/use-i18n";
import { DocsNav, type DocPage } from "./DocsNav";
import { DocsContent } from "./DocsContent";
import { LoadingOrbs } from "../shared/LoadingOrbs";

const ChatPanel = lazy(() =>
  import("../chat/ChatPanel").then((m) => ({ default: m.ChatPanel })),
);
const ChatSettings = lazy(() =>
  import("../chat/ChatSettings").then((m) => ({ default: m.ChatSettings })),
);

export function DocsViewer() {
  const { t } = useI18n();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const [activePath, setActivePath] = useState<string | null>(null);
  const [chatOpen, setChatOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const queryClient = useQueryClient();
  const docsFallback = (
    <div className="flex items-center justify-center h-full">
      <LoadingOrbs />
    </div>
  );

  // Fetch doc index
  const {
    data: docIndex,
    isLoading: indexLoading,
  } = useQuery({
    queryKey: ["doc-index", activeRepo],
    queryFn: () => commands.getDocIndex(),
    enabled: !!activeRepo,
  });

  // Fetch active page content.
  // Scope by `activeRepo` so two repos that share a doc filename (common:
  // `overview.md`, `architecture.md`) don't serve the previously opened
  // repo's cached markdown when the user switches.
  const {
    data: pageContent,
    isLoading: contentLoading,
  } = useQuery({
    queryKey: ["doc-content", activeRepo, activePath],
    queryFn: () => commands.readDoc(activePath!),
    enabled: !!activePath,
    staleTime: Infinity,
  });

  // Generate docs mutation
  const generateMutation = useMutation({
    mutationFn: async () => {
      // Get repo path from the repos list
      const repos = await commands.listRepos();
      const repo = repos.find((r: RepoInfo) => r.name === activeRepo);
      if (!repo) throw new Error("Repository not found");
      return commands.generateDocs("docs", repo.path);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["doc-index"] });
      queryClient.invalidateQueries({ queryKey: ["doc-content"] });
    },
  });

  // Reset active path when repo changes (render-time state adjustment)
  const [prevRepo, setPrevRepo] = useState(activeRepo);
  if (activeRepo !== prevRepo) {
    setPrevRepo(activeRepo);
    setActivePath(null);
  }

  // Auto-select first page when index loads (render-time state adjustment)
  const [prevDocIndex, setPrevDocIndex] = useState(docIndex);
  if (docIndex !== prevDocIndex) {
    setPrevDocIndex(docIndex);
    if (docIndex && !activePath) {
      const firstPage = findFirstPage(docIndex.pages);
      if (firstPage) setActivePath(firstPage);
    }
  }

  const handleNavigate = useCallback((path: string) => {
    setActivePath(path);
  }, []);

  const handleGenerate = useCallback(() => {
    generateMutation.mutate();
  }, [generateMutation]);

  // No docs generated yet
  if (!indexLoading && !docIndex) {
    return (
      <div className="h-full flex items-center justify-center fade-in">
        <div className="text-center max-w-md">
          <div
            className="w-16 h-16 rounded-2xl flex items-center justify-center mx-auto mb-6"
            style={{ background: "var(--accent-subtle)", color: "var(--accent)" }}
          >
            <BookOpen size={28} />
          </div>
          <h2
            className="text-xl mb-3"
            style={{ fontFamily: "var(--font-display)", color: "var(--text-0)" }}
          >
            {t("docs.generateTitle")}
          </h2>
          <p className="text-sm mb-4" style={{ color: "var(--text-2)" }}>
            {t("docs.generateDesc")}
          </p>
          <div
            className="flex flex-col gap-2 mb-6 text-left mx-auto"
            style={{ maxWidth: "320px" }}
          >
            {[
              { icon: <Package size={14} style={{ color: "var(--accent)", flexShrink: 0 }} />, text: t("docs.featureModules") },
              { icon: <Link2 size={14} style={{ color: "var(--purple)", flexShrink: 0 }} />, text: t("docs.featureCrossRef") },
              { icon: <FileTextIcon size={14} style={{ color: "var(--green)", flexShrink: 0 }} />, text: t("docs.featureApiDocs") },
              { icon: <MessageCircle size={14} style={{ color: "var(--cyan)", flexShrink: 0 }} />, text: t("docs.featureChat") },
            ].map((item) => (
              <div
                key={item.text}
                className="flex items-center gap-2.5 px-3 py-2 rounded-lg text-xs"
                style={{ background: "var(--bg-2)", color: "var(--text-2)" }}
              >
                {item.icon}
                <span>{item.text}</span>
              </div>
            ))}
          </div>
          <button
            onClick={handleGenerate}
            disabled={generateMutation.isPending}
            className="inline-flex items-center gap-2 px-5 py-2.5 rounded-lg text-sm font-medium transition-all"
            style={{
              background: generateMutation.isPending ? "var(--bg-3)" : "var(--accent)",
              color: generateMutation.isPending ? "var(--text-2)" : "#fff",
              cursor: generateMutation.isPending ? "wait" : "pointer",
            }}
          >
            {generateMutation.isPending ? (
              <>
                <RefreshCw size={15} className="animate-spin" />
                {t("docs.generating")}
              </>
            ) : (
              <>
                <Sparkles size={15} />
                {t("docs.generateButton")}
              </>
            )}
          </button>
          {generateMutation.isError && (
            <p className="mt-4 text-xs" style={{ color: "var(--rose)" }}>
              {(generateMutation.error as Error).message}
            </p>
          )}
        </div>
      </div>
    );
  }

  // Loading
  if (indexLoading) {
    return (
      <div className="h-full flex items-center justify-center">
        <div className="pulse-subtle" style={{ color: "var(--text-3)" }}>
          {t("docs.loadingDocs")}
        </div>
      </div>
    );
  }

  // Docs available — render the wiki viewer with chat
  return (
    <div className="h-full flex fade-in">
      {/* Navigation sidebar */}
      <div
        className="flex-shrink-0 overflow-y-auto flex flex-col"
        style={{
          width: 240,
          borderRight: "1px solid var(--surface-border)",
          background: "var(--bg-1)",
        }}
      >
        <div className="flex-1 overflow-y-auto">
          <DocsNav
            index={docIndex!}
            activePath={activePath}
            onNavigate={handleNavigate}
            onRegenerate={handleGenerate}
            isRegenerating={generateMutation.isPending}
          />
        </div>

        {/* Chat toggle button at bottom of nav */}
        <div className="flex-shrink-0 p-3" style={{ borderTop: "1px solid var(--surface-border)" }}>
          <button
            onClick={() => setChatOpen(!chatOpen)}
            className="w-full flex items-center gap-2 px-3 py-2 rounded-lg text-[13px] font-medium transition-all"
            style={{
              background: chatOpen ? "var(--purple-subtle)" : "var(--surface)",
              color: chatOpen ? "var(--purple)" : "var(--text-2)",
              border: `1px solid ${chatOpen ? "rgba(167,139,250,0.25)" : "var(--surface-border)"}`,
            }}
          >
            <MessageSquare size={14} />
            {t("docs.askAboutCode")}
          </button>
        </div>
      </div>

      {/* Main content area */}
      <div className="flex-1 flex flex-col min-h-0">
        {chatOpen ? (
          /* Chat mode — full content area becomes chat */
          <Suspense fallback={docsFallback}>
            <ChatPanel onOpenSettings={() => setSettingsOpen(true)} />
          </Suspense>
        ) : (
          /* Docs mode — normal content viewer */
          <div className="flex-1 overflow-y-auto">
            {contentLoading ? (
              <div className="flex items-center justify-center h-full">
                <div className="pulse-subtle" style={{ color: "var(--text-3)" }}>
                  {t("docs.loadingPage")}
                </div>
              </div>
            ) : pageContent ? (
              <DocsContent content={pageContent.content} title={pageContent.title} onNavigate={handleNavigate} />
            ) : (
              <div className="flex items-center justify-center h-full" style={{ color: "var(--text-3)" }}>
                {t("docs.selectPage")}
              </div>
            )}
          </div>
        )}
      </div>

      {/* Settings modal */}
      {settingsOpen && (
        <Suspense fallback={docsFallback}>
          <ChatSettings onClose={() => setSettingsOpen(false)} />
        </Suspense>
      )}
    </div>
  );
}

/** Find the first navigable page in the tree. */
function findFirstPage(pages: DocPage[]): string | null {
  for (const page of pages) {
    if (page.path) return page.path;
    if (page.children) {
      const child = findFirstPage(page.children);
      if (child) return child;
    }
  }
  return null;
}
