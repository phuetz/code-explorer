import { memo, useMemo } from "react";
import { useQuery, useIsFetching } from "@tanstack/react-query";
import { Link as LinkIcon } from "lucide-react";
import { toast } from "sonner";
import { useAppStore } from "../../stores/app-store";
import { useI18n } from "../../hooks/use-i18n";
import { useGraphData } from "../../hooks/use-tauri-query";
import { commands } from "../../lib/tauri-commands";
import { buildShareUrl } from "../../hooks/use-share-link";
import { BookmarksDropdown } from "./BookmarksDropdown";
import { copyTextToClipboard } from "../../lib/clipboard";

/** Separator — extracted outside StatusBar to satisfy react-hooks/static-components. */
function Sep() {
  return <div style={{ width: "1px", height: "12px", background: "var(--surface-border)" }} />;
}

function GraphStats() {
  const zoomLevel = useAppStore((s) => s.zoomLevel);
  const { data } = useGraphData({ zoomLevel, maxNodes: 200 }, true);
  if (!data?.stats) return null;
  return (
    <>
      <Sep />
      <span style={{ color: "var(--text-3)", fontVariantNumeric: "tabular-nums" }}>
        {data.stats.nodeCount}n | {data.stats.edgeCount}e
      </span>
    </>
  );
}

function NetworkActivity() {
  const fetching = useIsFetching();
  if (fetching === 0) return null;
  return (
    <>
      <Sep />
      <span className="flex items-center gap-1" style={{ color: "var(--text-3)" }}>
        <span
          className="w-1.5 h-1.5 rounded-full"
          style={{ background: "var(--accent)", animation: "pulse-subtle 1s ease-in-out infinite" }}
        />
        {fetching} req
      </span>
    </>
  );
}

export const StatusBar = memo(function StatusBar() {
  const { t } = useI18n();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const mode = useAppStore((s) => s.mode);
  const analyzeView = useAppStore((s) => s.analyzeView);
  const zoomLevel = useAppStore((s) => s.zoomLevel);

  const MODE_LABELS: Record<string, string> = {
    explorer: t("sidebar.graphExplorer"),
    analyze: t("sidebar.analysis"),
    chat: t("sidebar.chat"),
    manage: t("manage.title"),
  };

  const { data: chatConfig } = useQuery({
    queryKey: ["chat-config"],
    queryFn: () => commands.chatGetConfig(),
    staleTime: 30_000,
  });

  const llmConnected = useMemo(() => {
    if (!chatConfig) return false;
    if (chatConfig.provider === "ollama") return true;
    return chatConfig.apiKey.length > 0;
  }, [chatConfig]);

  const llmModelName = chatConfig?.model || null;

  /** Contextual info that changes per mode */
  const ctxInfo = useMemo(() => {
    if (!activeRepo) return null;
    switch (mode) {
      case "explorer": {
        const levelName = {
          "package": t("status.packageLevel"),
          "module": t("status.moduleLevel"),
          "symbol": t("status.symbolLevel"),
        }[zoomLevel] || (zoomLevel.charAt(0).toUpperCase() + zoomLevel.slice(1) + " level");
        return (
          <span style={{ color: "var(--text-3)" }}>
            <span style={{ fontWeight: 500, color: "var(--text-2)" }}>{t("status.view")}:</span>{" "}
            {levelName}
          </span>
        );
      }
      case "analyze":
        return (
          <span style={{ color: "var(--text-3)" }}>
            <span style={{ fontWeight: 500, color: "var(--text-2)" }}>{t("status.view")}:</span>{" "}
            {analyzeView.charAt(0).toUpperCase() + analyzeView.slice(1)}
          </span>
        );
      case "chat":
        return (
          <span style={{ color: "var(--text-3)" }}>
            <span style={{ fontWeight: 500, color: "var(--text-2)" }}>AI:</span> {t("status.aiChat")}
          </span>
        );
      case "manage":
        return (
          <span style={{ color: "var(--text-3)" }}>
            <span style={{ fontWeight: 500, color: "var(--text-2)" }}>{t("manage.title")}:</span> {t("status.reposSettings")}
          </span>
        );
      default:
        return null;
    }
  }, [activeRepo, mode, analyzeView, zoomLevel, t]);

  return (
    <div
      className="flex items-center text-[10px] shrink-0 select-none"
      style={{
        height: 26,
        paddingLeft: 16,
        paddingRight: 16,
        gap: 16,
        background: "var(--bg-1)",
        borderTop: "1px solid var(--surface-border)",
        color: "var(--text-3)",
        fontFamily: "var(--font-mono)",
      }}
    >
      {activeRepo ? (
        <>
          {/* Repo status indicator */}
          <div className="flex items-center gap-1.5">
            <span
              className="w-1.5 h-1.5 rounded-full"
              style={{
                background: "var(--green)",
                animation: "pulse-subtle 2s ease-in-out infinite",
              }}
            />
            <span style={{ fontWeight: 500, color: "var(--text-2)" }}>
              {activeRepo}
            </span>
          </div>

          <Sep />

          {/* Current page */}
          <span style={{ color: "var(--text-3)" }}>
            {MODE_LABELS[mode] || mode}
          </span>

          {/* Contextual info per page */}
          {ctxInfo && (
            <>
              <Sep />
              {ctxInfo}
            </>
          )}

          {/* Graph stats when in explorer */}
          {mode === "explorer" && <GraphStats />}

          {/* Network activity */}
          <NetworkActivity />
        </>
      ) : (
        <span style={{ color: "var(--text-3)" }}>{t("status.noRepo")}</span>
      )}

      {/* Right: bookmarks + share + LLM status + version */}
      <div className="flex items-center gap-3" style={{ marginLeft: "auto" }}>
        {activeRepo && <BookmarksDropdown />}

        {activeRepo && (
          <button
            onClick={async () => {
              const url = buildShareUrl();
              const ok = await copyTextToClipboard(url);
              if (ok) {
                toast.success(t("share.linkCopied"));
              } else {
                toast.error(t("share.copyFailed"));
              }
            }}
            title={t("share.tooltip")}
            aria-label={t("share.label")}
            style={{
              padding: "2px 6px",
              background: "transparent",
              border: "1px solid var(--surface-border)",
              borderRadius: 6,
              color: "var(--text-3)",
              cursor: "pointer",
              display: "inline-flex",
              alignItems: "center",
              gap: 4,
              fontSize: 11,
              fontFamily: "inherit",
            }}
          >
            <LinkIcon size={11} />
            <span>{t("share.button")}</span>
          </button>
        )}

        <Sep />

        {/* LLM status indicator — click opens Chat Settings (quick model switch) */}
        <button
          type="button"
          onClick={() => {
            // The ChatSettings modal is currently rendered inside <ChatMode>,
            // so we need to switch to Chat first if we're somewhere else.
            // When the modal is lifted to ModalManager (future refactor)
            // this `setMode` call can be dropped.
            const store = useAppStore.getState();
            if (store.mode !== "chat") store.setMode("chat");
            store.setChatSettingsOpen(true);
          }}
          title={t("statusbar.llm.tooltip")}
          aria-label={t("statusbar.llm.tooltip")}
          className="flex items-center gap-1.5 transition-colors hover:brightness-125"
          style={{
            background: "transparent",
            border: "none",
            padding: 0,
            cursor: "pointer",
            color: "inherit",
            font: "inherit",
          }}
        >
          <span
            className="rounded-full shrink-0"
            style={{
              width: 7,
              height: 7,
              background: llmConnected ? "var(--green)" : "var(--rose)",
            }}
          />
          {llmModelName && (
            <span style={{ color: "var(--text-3)" }}>{llmModelName}</span>
          )}
        </button>

        <Sep />

        <span style={{ color: "var(--text-3)" }}>
          <span style={{ fontWeight: 500, color: "var(--text-2)" }}>Code Explorer</span> v0.1.0
        </span>
      </div>
    </div>
  );
});
