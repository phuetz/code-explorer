import { useState, useCallback, useRef, useEffect } from "react";
import { createPortal } from "react-dom";
import {
  Database,
  RefreshCw,
  BookOpen,
  FileText,
  MoreHorizontal,
  Clock,
} from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { useRepos, useOpenRepo } from "../../hooks/use-tauri-query";
import { useAppStore } from "../../stores/app-store";
import { commands } from "../../lib/tauri-commands";
import { isTauri } from "../../lib/tauri-env";
import { useI18n } from "../../hooks/use-i18n";
import { Tooltip } from "../shared/Tooltip";
import { AnalyzeProgress, AnalyzeButton } from "./AnalyzeProgress";
import { AnimatedCard, AnimatedCounter } from "../shared/motion";
import { toast } from "sonner";

/** Strip the Windows \\?\ long-path prefix for display */
function cleanPath(p: string): string {
  return p.replace(/^\\\\\?\\/, "");
}

/** Format a timestamp to relative time.
 *
 * Accepts two formats:
 *  - Unix epoch seconds with optional trailing "Z" (the format the Rust backend
 *    currently writes via `chrono_now()`).
 *  - ISO 8601 strings like "2024-03-15T10:22:00Z".
 *
 * `parseInt` on an ISO string would return the leading year ("2024") which
 * would silently produce nonsense like "13816286h ago", so we sniff the format
 * before delegating.
 */
function timeAgo(ts: string): string {
  const stripped = ts.replace(/Z$/, "");
  let ms: number;
  // Pure-numeric (Unix seconds) vs ISO date string
  if (/^\d+$/.test(stripped)) {
    ms = Number(stripped) * 1000;
  } else {
    ms = Date.parse(ts);
  }
  if (isNaN(ms)) return ts;
  const diff = Math.floor((Date.now() - ms) / 1000);
  if (diff < 0) return "just now";
  if (diff < 60) return "just now";
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  if (diff < 604800) return `${Math.floor(diff / 86400)}d ago`;
  return new Date(ms).toLocaleDateString();
}

export function RepoManager() {
  const { t, tt } = useI18n();
  const queryClient = useQueryClient();
  const { data: repos, isLoading, error, refetch } = useRepos();
  const openRepo = useOpenRepo();
  const setActiveRepo = useAppStore((s) => s.setActiveRepo);
  const setMode = useAppStore((s) => s.setMode);

  // Analysis state
  const [isAnalyzing, setIsAnalyzing] = useState(false);
  const [analyzeRepoPath, setAnalyzeRepoPath] = useState<string | null>(null);
  const [analyzeError, setAnalyzeError] = useState<string | null>(null);

  const handleOpen = async (name: string) => {
    try {
      await openRepo.mutateAsync(name);
      setActiveRepo(name);
      setMode("explorer");
      toast.success(t("repos.repoOpened").replace("{0}", name));
    } catch (e) {
      console.error("Failed to open repo:", e);
    }
  };

  /** Open native folder picker, then launch analysis */
  const handleAnalyze = useCallback(async () => {
    try {
      if (!isTauri()) {
        return;
      }
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Select a project folder to analyze",
      });

      if (!selected) return; // User cancelled

      const folderPath = typeof selected === "string" ? selected : selected;
      setAnalyzeRepoPath(folderPath);
      setIsAnalyzing(true);

      // Launch analysis in background — progress comes via Tauri events
      setAnalyzeError(null);
      commands.analyzeRepo(folderPath).catch((err) => {
        console.error("Analysis failed:", err);
        setIsAnalyzing(false);
        setAnalyzeError(String(err));
        toast.error(t("repos.analysisFailed").replace("{0}", String(err)));
      });
    } catch (e) {
      console.error("Folder selection failed:", e);
    }
  }, [t]);

  const handleAnalyzeComplete = useCallback(() => {
    setIsAnalyzing(false);
    refetch(); // Refresh the repo list to show the new entry
    queryClient.invalidateQueries(); // Clear all cached graph/search data
  }, [refetch, queryClient]);

  const handleDismissProgress = useCallback(() => {
    setIsAnalyzing(false);
    setAnalyzeRepoPath(null);
  }, []);

  if (isLoading) {
    return (
      <div className="h-full p-4 space-y-3">
        {[1, 2, 3].map(i => (
          <div key={i} className="shimmer rounded-xl" style={{ height: 80, background: "var(--bg-2)" }} />
        ))}
      </div>
    );
  }

  if (error) {
    return (
      <div
        className="h-full flex flex-col items-center justify-center gap-3"
        style={{ color: "var(--rose)" }}
      >
        <span>{t("repos.error")}: {String(error)}</span>
        <button
          onClick={() => refetch()}
          className="px-3 py-1.5 rounded-lg text-xs font-medium"
          style={{ background: "var(--accent)", color: "#fff", border: "none", cursor: "pointer" }}
        >
          {t("common.retry") || "Retry"}
        </button>
      </div>
    );
  }

  if (!repos || repos.length === 0) {
    return (
      <div className="h-full flex flex-col items-center justify-center fade-in" style={{ gap: 20 }}>
        {/* Progress overlay if analyzing */}
        {(isAnalyzing || analyzeRepoPath) && (
          <div style={{ width: 500, marginBottom: 16 }}>
            <AnalyzeProgress
              isAnalyzing={isAnalyzing}
              repoPath={analyzeRepoPath}
              onComplete={handleAnalyzeComplete}
              onDismiss={handleDismissProgress}
            />
          </div>
        )}

        <div
          className="w-16 h-16 rounded-2xl flex items-center justify-center"
          style={{ background: "var(--bg-3)", color: "var(--text-3)" }}
        >
          <Database size={28} />
        </div>
        <div className="text-center">
          <p
            className="text-lg font-semibold"
            style={{
              fontFamily: "var(--font-display)",
              color: "var(--text-1)",
              marginBottom: 4,
            }}
          >
            {t("repos.noRepos")}
          </p>
          <p className="text-sm" style={{ color: "var(--text-3)", marginBottom: 20 }}>
            {t("repos.noReposDesc")}
          </p>
          <AnalyzeButton onClick={handleAnalyze} disabled={isAnalyzing} />

          {/* Onboarding steps */}
          <div
            style={{
              marginTop: 32,
              padding: "20px 24px",
              background: "var(--bg-2)",
              border: "1px solid var(--surface-border)",
              borderRadius: 12,
              maxWidth: 320,
              marginLeft: "auto",
              marginRight: "auto",
            }}
          >
            {/* Step 1 */}
            <div
              className="flex items-center"
              style={{ marginBottom: 20 }}
            >
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  width: 24,
                  height: 24,
                  borderRadius: "50%",
                  background: "var(--bg-3)",
                  color: "var(--accent)",
                  fontWeight: 600,
                  fontSize: 13,
                  marginRight: 12,
                  flexShrink: 0,
                }}
              >
                1
              </div>
              <span
                style={{
                  color: "var(--text-2)",
                  fontSize: 13,
                  textAlign: "left",
                }}
              >
                {t("repos.onboarding.step1.desc")}
              </span>
            </div>

            {/* Step 2 */}
            <div
              className="flex items-center"
              style={{ marginBottom: 20 }}
            >
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  width: 24,
                  height: 24,
                  borderRadius: "50%",
                  background: "var(--bg-3)",
                  color: "var(--accent)",
                  fontWeight: 600,
                  fontSize: 13,
                  marginRight: 12,
                  flexShrink: 0,
                }}
              >
                2
              </div>
              <span
                style={{
                  color: "var(--text-2)",
                  fontSize: 13,
                  textAlign: "left",
                }}
              >
                {t("repos.onboarding.step2.desc")}
              </span>
            </div>

            {/* Step 3 */}
            <div
              className="flex items-center"
            >
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  width: 24,
                  height: 24,
                  borderRadius: "50%",
                  background: "var(--bg-3)",
                  color: "var(--accent)",
                  fontWeight: 600,
                  fontSize: 13,
                  marginRight: 12,
                  flexShrink: 0,
                }}
              >
                3
              </div>
              <span
                style={{
                  color: "var(--text-2)",
                  fontSize: 13,
                  textAlign: "left",
                }}
              >
                {t("repos.onboarding.step3.desc")}
              </span>
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto fade-in">
      {/* Centered content container with generous padding */}
      <div
        style={{
          maxWidth: 780,
          marginLeft: "auto",
          marginRight: "auto",
          paddingLeft: 40,
          paddingRight: 40,
          paddingTop: 40,
          paddingBottom: 40,
        }}
      >
        {/* Header */}
        <div className="flex items-center justify-between" style={{ marginBottom: 32 }}>
          <div>
            <h1
              className="text-xl font-semibold"
              style={{
                fontFamily: "var(--font-display)",
                color: "var(--text-0)",
              }}
            >
              {t("repos.title")}
            </h1>
            <p className="text-xs" style={{ color: "var(--text-3)", marginTop: 4 }}>
              {repos.length} {t("repos.indexed")}{" "}
              {repos.length === 1 ? t("repos.repository") : t("repos.repositories")}
            </p>
          </div>
          <div className="flex items-center" style={{ gap: 10 }}>
            <AnalyzeButton onClick={handleAnalyze} disabled={isAnalyzing} />
            <Tooltip content={tt("repos.refresh").tip}>
              <button
                onClick={() => refetch()}
                className="flex items-center rounded-lg text-xs font-medium transition-all hover:brightness-110"
                style={{
                  gap: 6,
                  paddingLeft: 14,
                  paddingRight: 14,
                  paddingTop: 8,
                  paddingBottom: 8,
                  background: "var(--bg-3)",
                  color: "var(--text-2)",
                  border: "1px solid var(--surface-border)",
                }}
              >
                <RefreshCw size={13} /> {tt("repos.refresh").label}
              </button>
            </Tooltip>
          </div>
        </div>

        {/* Analysis progress */}
        {(isAnalyzing || analyzeRepoPath) && (
          <div style={{ marginBottom: 24 }}>
            <AnalyzeProgress
              isAnalyzing={isAnalyzing}
              repoPath={analyzeRepoPath}
              onComplete={handleAnalyzeComplete}
              onDismiss={handleDismissProgress}
            />
          </div>
        )}

        {/* Analysis error */}
        {analyzeError && (
          <div
            className="rounded-lg text-sm"
            style={{
              marginBottom: 24,
              padding: "12px 16px",
              background: "rgba(240, 100, 120, 0.08)",
              border: "1px solid rgba(240, 100, 120, 0.25)",
              color: "var(--rose)",
            }}
          >
            Analysis failed: {analyzeError}
          </div>
        )}

        {/* Cards */}
        <div className="grid" style={{ gap: 20 }}>
          {repos.map((repo, index) => (
            <AnimatedCard key={repo.name} delay={index * 0.05}>
              <RepoCard
                repo={repo}
                onOpen={() => handleOpen(repo.name)}
                isOpening={openRepo.isPending}
              />
            </AnimatedCard>
          ))}
        </div>
      </div>
    </div>
  );
}

function RepoCard({
  repo,
  onOpen,
  isOpening,
}: {
  repo: {
    name: string;
    path: string;
    indexedAt: string;
    files?: number;
    nodes?: number;
    edges?: number;
    communities?: number;
  };
  onOpen: () => void;
  isOpening: boolean;
}) {
  const { t, tt } = useI18n();
  const queryClient = useQueryClient();
  const [showMenu, setShowMenu] = useState(false);
  const [menuPos, setMenuPos] = useState<{ top: number; left: number } | null>(null);
  const menuBtnRef = useRef<HTMLButtonElement>(null);
  const [status, setStatus] = useState<{
    text: string;
    type: "success" | "error";
  } | null>(null);
  const [busy, setBusy] = useState(false);

  // Close menu on scroll, resize, or outside click. Resize is included so a
  // user dragging the Tauri window border doesn't leave the portalized menu
  // floating at its original screen position — `menuPos` is captured once at
  // open time and never recomputed.
  useEffect(() => {
    if (!showMenu) return;
    let cancelled = false;
    const close = () => setShowMenu(false);
    // Delay adding click listener so the opening click doesn't immediately close it
    const timer = setTimeout(() => {
      if (!cancelled) window.addEventListener("click", close);
    }, 0);
    window.addEventListener("scroll", close, true);
    window.addEventListener("resize", close);
    return () => {
      cancelled = true;
      clearTimeout(timer);
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("resize", close);
      window.removeEventListener("click", close);
    };
  }, [showMenu]);

  const runAction = async (action: () => Promise<unknown>, label: string) => {
    setBusy(true);
    setStatus(null);
    setShowMenu(false);
    try {
      await action();
      setStatus({ text: `${label} completed`, type: "success" });
      queryClient.invalidateQueries();
    } catch (e) {
      setStatus({ text: String(e), type: "error" });
    } finally {
      setBusy(false);
    }
  };

  // Generate a color from repo name
  const hue =
    repo.name.split("").reduce((a, c) => a + c.charCodeAt(0), 0) % 360;

  return (
    <div
      className="rounded-xl transition-all duration-200 group hover:shadow-md hover:brightness-105"
      style={{
        background: "var(--surface)",
        border: "1px solid var(--surface-border)",
      }}
    >
      <div
        role="button"
        tabIndex={0}
        onClick={() => { if (!isOpening && !busy) onOpen(); }}
        onKeyDown={(e) => { if ((e.key === "Enter" || e.key === " ") && !isOpening && !busy) { e.preventDefault(); onOpen(); } }}
        className="flex items-start w-full text-left transition-colors cursor-pointer hover:brightness-105"
        style={{ gap: 16, paddingLeft: 24, paddingRight: 24, paddingTop: 20, paddingBottom: 20, opacity: isOpening || busy ? 0.6 : 1 }}
      >
        {/* Avatar */}
        <div
          className="w-10 h-10 rounded-lg flex items-center justify-center text-white font-bold shrink-0"

          style={{
            background: `linear-gradient(135deg, hsl(${hue}, 60%, 45%), hsl(${hue + 40}, 50%, 35%))`,
            fontFamily: "var(--font-display)",
            fontSize: 16,
          }}
        >
          {repo.name.charAt(0).toUpperCase()}
        </div>

        <div className="flex-1 min-w-0">
          {/* Name + timestamp row */}
          <div className="flex items-center" style={{ gap: 8 }}>
            <h3
              className="font-semibold text-sm"
              style={{
                color: "var(--text-0)",
                fontFamily: "var(--font-display)",
              }}
            >
              {repo.name}
            </h3>
            {repo.indexedAt && (
              <span
                className="flex items-center text-[10px] shrink-0"
                style={{ gap: 4, color: "var(--text-4)" }}
              >
                <Clock size={10} />
                {timeAgo(repo.indexedAt)}
              </span>
            )}
          </div>

          {/* Clean path */}
          <p
            className="text-[11px] truncate"
            style={{ color: "var(--text-3)", fontFamily: "var(--font-mono)", marginTop: 2 }}
          >
            {cleanPath(repo.path)}
          </p>

          {/* Stats badges */}
          <div className="flex flex-wrap" style={{ gap: 6, marginTop: 10 }}>
            {repo.files != null && (
              <StatBadge
                value={repo.files}
                label={t("repos.files")}
                bg="rgba(99, 179, 237, 0.12)"
                fg="var(--accent)"
              />
            )}
            {repo.nodes != null && (
              <StatBadge
                value={formatNumber(repo.nodes)}
                label={t("repos.nodes")}
                bg="rgba(180, 130, 255, 0.12)"
                fg="var(--purple)"
              />
            )}
            {repo.edges != null && (
              <StatBadge
                value={formatNumber(repo.edges)}
                label={t("repos.edges")}
                bg="rgba(100, 220, 220, 0.12)"
                fg="var(--cyan)"
              />
            )}
            {repo.communities != null && (
              <StatBadge
                value={repo.communities}
                label={t("repos.communities")}
                bg="rgba(100, 220, 150, 0.12)"
                fg="var(--green)"
              />
            )}
          </div>
        </div>

        {/* Menu button */}
        <div className="shrink-0">
          <button
            ref={menuBtnRef}
            onClick={(e) => {
              e.stopPropagation();
              if (!showMenu && menuBtnRef.current) {
                const rect = menuBtnRef.current.getBoundingClientRect();
                setMenuPos({ top: rect.bottom + 4, left: rect.right - 192 });
              }
              setShowMenu(!showMenu);
            }}
            className="rounded-md opacity-0 group-hover:opacity-100 focus:opacity-100 transition-opacity"
            style={{ padding: 6, color: "var(--text-3)" }}
            aria-label={`Actions for ${repo.name}`}
            aria-haspopup="menu"
            aria-expanded={showMenu}
          >
            <MoreHorizontal size={16} />
          </button>

          {showMenu && menuPos && createPortal(
            <div
              className="w-48 rounded-lg fade-in"
              style={{
                position: "fixed",
                top: menuPos.top,
                left: menuPos.left,
                zIndex: 9999,
                background: "var(--bg-3)",
                border: "1px solid var(--surface-border-hover)",
                boxShadow: "var(--shadow-lg)",
                paddingTop: 4,
                paddingBottom: 4,
              }}
              onMouseLeave={() => setShowMenu(false)}
            >
              <MenuItem
                icon={<RefreshCw size={13} />}
                label={tt("repos.reindex").label}
                title={tt("repos.reindex").tip}
                onClick={() =>
                  runAction(
                    () => commands.analyzeRepo(repo.path),
                    tt("repos.reindex").label
                  )
                }
              />
              <div
                style={{ marginTop: 4, marginBottom: 4, borderTop: "1px solid var(--surface-border)" }}
              />
              <MenuItem
                icon={<BookOpen size={13} />}
                label={tt("repos.generateWiki").label}
                title={tt("repos.generateWiki").tip}
                onClick={() =>
                  runAction(
                    () => commands.generateDocs("wiki", repo.path),
                    tt("repos.generateWiki").label
                  )
                }
              />
              <MenuItem
                icon={<FileText size={13} />}
                label={tt("repos.generateDocs").label}
                title={tt("repos.generateDocs").tip}
                onClick={() =>
                  runAction(
                    () => commands.generateDocs("docs", repo.path),
                    tt("repos.generateDocs").label
                  )
                }
              />
              <MenuItem
                icon={<FileText size={13} />}
                label={tt("repos.generateAgents").label}
                title={tt("repos.generateAgents").tip}
                onClick={() =>
                  runAction(
                    () => commands.generateDocs("context", repo.path),
                    tt("repos.generateAgents").label
                  )
                }
              />
              <MenuItem
                icon={<FileText size={13} />}
                label={tt("repos.generateAll").label}
                title={tt("repos.generateAll").tip}
                onClick={() =>
                  runAction(
                    () => commands.generateDocs("all", repo.path),
                    tt("repos.generateAll").label
                  )
                }
              />
            </div>,
            document.body
          )}
        </div>
      </div>

      {/* Status */}
      {(status || busy || isOpening) && (
        <div
          className="text-[11px] font-medium"
          style={{
            paddingLeft: 24,
            paddingRight: 24,
            paddingTop: 10,
            paddingBottom: 10,
            borderTop: "1px solid var(--surface-border)",
            color: busy || isOpening
              ? "var(--text-3)"
              : status?.type === "success"
                ? "var(--green)"
                : "var(--rose)",
            background: busy || isOpening
              ? "var(--bg-2)"
              : status?.type === "success"
                ? "rgba(100, 220, 150, 0.08)"
                : "rgba(240, 100, 120, 0.08)",
          }}
        >
          {isOpening ? t("repos.opening") : busy ? t("repos.processing") : status?.text}
        </div>
      )}
    </div>
  );
}

function MenuItem({
  icon,
  label,
  title,
  onClick,
}: {
  icon: React.ReactNode;
  label: string;
  title?: string;
  onClick: () => void;
}) {
  return (
    <button
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
      className="w-full flex items-center text-[12px] text-left transition-colors hover:brightness-125"
      style={{ gap: 10, paddingLeft: 12, paddingRight: 12, paddingTop: 6, paddingBottom: 6, color: "var(--text-2)" }}
      title={title}
    >
      {icon}
      {label}
    </button>
  );
}

function StatBadge({
  value,
  label,
  bg,
  fg,
}: {
  value: string | number;
  label: string;
  bg: string;
  fg: string;
}) {
  return (
    <span
      className="inline-flex items-center rounded-md"
      style={{
        gap: 4,
        paddingLeft: 8,
        paddingRight: 8,
        paddingTop: 2,
        paddingBottom: 2,
        background: bg,
        color: fg,
        fontSize: 10,
        fontWeight: 600,
        letterSpacing: "0.01em",
      }}
    >
      {typeof value === "number" ? (
        <AnimatedCounter value={value} style={{ fontSize: 10, fontWeight: 600 }} />
      ) : (
        value
      )}{" "}
      {label}
    </span>
  );
}

function formatNumber(n: number): string {
  if (n >= 1000) return (n / 1000).toFixed(1) + "k";
  return String(n);
}
