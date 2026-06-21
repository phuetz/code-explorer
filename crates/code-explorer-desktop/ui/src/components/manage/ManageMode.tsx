import React, { lazy, Suspense, useState } from "react";
import { Database, Download, BookOpen, Settings, Globe, Sun, Moon, Monitor } from "lucide-react";
import { useAppStore, type ThemeMode } from "../../stores/app-store";
import { useI18n, type Locale } from "../../hooks/use-i18n";
import { LoadingOrbs } from "../shared/LoadingOrbs";

const RepoManager = lazy(() =>
  import("../repos/RepoManager").then((m) => ({ default: m.RepoManager })),
);
const MultiRepoOverview = lazy(() =>
  import("./MultiRepoOverview").then((m) => ({ default: m.MultiRepoOverview })),
);
const ExportPanel = lazy(() =>
  import("../export/ExportPanel").then((m) => ({ default: m.ExportPanel })),
);
const DocsViewer = lazy(() =>
  import("../docs/DocsViewer").then((m) => ({ default: m.DocsViewer })),
);

type ManageTab = "repos" | "docs" | "settings";

const TABS: { id: ManageTab; icon: typeof Database; labelKey: string }[] = [
  { id: "repos", icon: Database, labelKey: "manage.repositories" },
  { id: "docs", icon: BookOpen, labelKey: "manage.documentation" },
  { id: "settings", icon: Settings, labelKey: "manage.settings" },
];

function Section({ icon: Icon, title, children }: { icon: typeof Database; title: string; children: React.ReactNode }) {
  return (
    <section
      className="rounded-xl p-5 mb-4"
      style={{
        background: "var(--glass-bg)",
        backdropFilter: "blur(var(--glass-blur))",
        border: "1px solid var(--glass-border)",
      }}
    >
      <h2 className="flex items-center gap-2 mb-4" style={{ fontFamily: "var(--font-display)", fontSize: 16, fontWeight: 600, color: "var(--text-0)" }}>
        <Icon size={18} style={{ color: "var(--accent)" }} />
        {title}
      </h2>
      {children}
    </section>
  );
}

// ─── Settings content (extracted from SettingsModal) ─────────────────

const LANGUAGES: { code: Locale; label: string; flag: string }[] = [
  { code: "fr", label: "Français", flag: "FR" },
  { code: "en", label: "English", flag: "EN" },
];

const THEMES: { mode: ThemeMode; icon: typeof Sun; labelKey: string }[] = [
  { mode: "dark", icon: Moon, labelKey: "manage.theme.dark" },
  { mode: "light", icon: Sun, labelKey: "manage.theme.light" },
  { mode: "system", icon: Monitor, labelKey: "manage.theme.system" },
];

function SettingsContent() {
  const { locale, setLocale, tt, t } = useI18n();
  const theme = useAppStore((s) => s.theme);
  const setTheme = useAppStore((s) => s.setTheme);

  const langEntry = tt("settings.language");
  const themeEntry = tt("settings.theme");

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
      {/* Language selector */}
      <div
        className="rounded-lg"
        style={{
          background: "var(--bg-1)",
          border: "1px solid var(--surface-border)",
          padding: "16px 20px",
        }}
      >
        <div className="flex items-center" style={{ gap: 10, marginBottom: 12 }}>
          <Globe size={16} style={{ color: "var(--accent)" }} />
          <div>
            <p className="text-sm font-medium" style={{ color: "var(--text-0)" }}>
              {langEntry.label}
            </p>
            {langEntry.tip && (
              <p className="text-[11px]" style={{ color: "var(--text-3)", marginTop: 2 }}>
                {langEntry.tip}
              </p>
            )}
          </div>
        </div>
        <div className="flex" style={{ gap: 8 }}>
          {LANGUAGES.map((lang) => {
            const isActive = locale === lang.code;
            return (
              <button
                key={lang.code}
                onClick={() => setLocale(lang.code)}
                className="rounded-md text-xs font-medium"
                style={{
                  padding: "8px 16px",
                  background: isActive ? "var(--accent)" : "var(--bg-3)",
                  color: isActive ? "#fff" : "var(--text-2)",
                  border: isActive ? "1px solid var(--accent)" : "1px solid var(--surface-border)",
                  cursor: "pointer",
                  flex: 1,
                }}
              >
                {lang.flag}  {lang.label}
              </button>
            );
          })}
        </div>
      </div>

      {/* Theme selector */}
      <div
        className="rounded-lg"
        style={{
          background: "var(--bg-1)",
          border: "1px solid var(--surface-border)",
          padding: "16px 20px",
        }}
      >
        <div className="flex items-center" style={{ gap: 10, marginBottom: 12 }}>
          <Sun size={16} style={{ color: "var(--amber)" }} />
          <p className="text-sm font-medium" style={{ color: "var(--text-0)" }}>
            {themeEntry.label}
          </p>
        </div>
        <div className="flex" style={{ gap: 8 }}>
          {THEMES.map((themeOption) => {
            const isActive = theme === themeOption.mode;
            const Icon = themeOption.icon;
            return (
              <button
                key={themeOption.mode}
                onClick={() => setTheme(themeOption.mode)}
                className="rounded-md text-xs font-medium transition-all"
                style={{
                  padding: "8px 16px",
                  background: isActive ? "var(--accent)" : "var(--bg-3)",
                  color: isActive ? "#fff" : "var(--text-2)",
                  border: isActive ? "1px solid var(--accent)" : "1px solid var(--surface-border)",
                  cursor: "pointer",
                  flex: 1,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  gap: 6,
                }}
              >
                <Icon size={14} />
                {t(themeOption.labelKey)}
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}

// ─── ManageMode ───────────────────────────────────────────────────────

export function ManageMode() {
  const { t } = useI18n();
  const [tab, setTab] = useState<ManageTab>("repos");
  const manageFallback = (
    <div className="flex items-center justify-center h-full">
      <LoadingOrbs />
    </div>
  );

  return (
    <div className="h-full flex flex-col">
      {/* Tab bar */}
      <div
        className="flex items-center gap-1 px-4 shrink-0"
        role="tablist"
        aria-label={t("manage.title")}
        style={{
          height: 44,
          borderBottom: "1px solid var(--surface-border)",
          background: "var(--bg-1)",
        }}
      >
        {TABS.map(({ id, icon: Icon, labelKey }) => {
          const active = tab === id;
          return (
            <button
              key={id}
              role="tab"
              aria-selected={active}
              onClick={() => setTab(id)}
              className="flex items-center gap-2 px-4 py-1.5 rounded-md text-xs font-medium transition-colors"
              style={{
                color: active ? "var(--accent)" : "var(--text-3)",
                background: active ? "var(--accent-subtle)" : "transparent",
                border: "none",
                cursor: "pointer",
              }}
            >
              <Icon size={14} />
              {t(labelKey)}
            </button>
          );
        })}
      </div>

      {/* Tab content */}
      {tab === "docs" ? (
        /* Docs get full remaining height for proper treeview layout */
        <div className="flex-1 min-h-0">
          <Suspense fallback={manageFallback}>
            <DocsViewer />
          </Suspense>
        </div>
      ) : (
        <div className="flex-1 overflow-auto p-6" style={{ maxWidth: 900, margin: "0 auto", width: "100%" }}>
          {tab === "repos" && (
            <>
              <Section icon={Database} title={t("manage.multiRepoOverview")}>
                <Suspense fallback={manageFallback}>
                  <MultiRepoOverview />
                </Suspense>
              </Section>
              <Section icon={Database} title={t("manage.repositories")}>
                <Suspense fallback={manageFallback}>
                  <RepoManager />
                </Suspense>
              </Section>
              <Section icon={Download} title={t("manage.export")}>
                <Suspense fallback={manageFallback}>
                  <ExportPanel />
                </Suspense>
              </Section>
            </>
          )}
          {tab === "settings" && (
            <Section icon={Settings} title={t("manage.settings")}>
              <SettingsContent />
            </Section>
          )}
        </div>
      )}
    </div>
  );
}
