import { X, Globe, Sun, Moon, Monitor } from "lucide-react";
import { useAppStore, type ThemeMode } from "../../stores/app-store";
import { useI18n, type Locale } from "../../hooks/use-i18n";

const LANGUAGES: { code: Locale; label: string; flag: string }[] = [
  { code: "fr", label: "Français", flag: "FR" },
  { code: "en", label: "English", flag: "EN" },
];

export function SettingsModal() {
  const settingsOpen = useAppStore((s) => s.settingsOpen);
  const setSettingsOpen = useAppStore((s) => s.setSettingsOpen);
  const { t, tt, locale, setLocale } = useI18n();

  if (!settingsOpen) return null;

  const langEntry = tt("settings.language");

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center"
      role="dialog"
      aria-modal="true"
      aria-label="Settings"
      style={{ backgroundColor: "rgba(0, 0, 0, 0.6)", backdropFilter: "blur(4px)" }}
      onClick={(e) => {
        if (e.target === e.currentTarget) setSettingsOpen(false);
      }}
    >
      <div
        className="rounded-xl shadow-2xl overflow-hidden"
        style={{
          width: "min(520px, calc(100vw - 32px))",
          maxHeight: "80vh",
          background: "var(--bg-0)",
          border: "1px solid var(--surface-border)",
        }}
      >
        {/* Header */}
        <div
          className="flex items-center justify-between"
          style={{ borderBottom: "1px solid var(--surface-border)", background: "var(--bg-1)", padding: "16px 24px" }}
        >
          <h2
            className="text-base font-semibold"
            style={{ color: "var(--text-0)", fontFamily: "var(--font-display)" }}
          >
            {t("settings.title")}
          </h2>
          <button
            onClick={() => setSettingsOpen(false)}
            className="rounded-md hover-surface"
            style={{ color: "var(--text-3)", padding: 6 }}
            title={t("search.close")}
          >
            <X size={16} />
          </button>
        </div>

        {/* Content */}
        <div style={{ padding: 24 }}>
          {/* ── Language selector (active) ── */}
          <div
            className="rounded-lg"
            style={{
              background: "var(--bg-1)",
              border: "1px solid var(--surface-border)",
              padding: "16px 20px",
              marginBottom: 20,
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
                    className={`rounded-md text-xs font-medium hover-lang-btn ${isActive ? "lang-active" : ""}`}
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

          {/* ── Theme selector ── */}
          <ThemeSelector />
        </div>

        {/* Footer */}
        <div
          className="flex justify-end"
          style={{ borderTop: "1px solid var(--surface-border)", background: "var(--bg-1)", padding: "12px 24px" }}
        >
          <button
            onClick={() => setSettingsOpen(false)}
            className="rounded-lg text-xs font-medium transition-all"
            style={{
              padding: "8px 16px",
              background: "var(--accent)",
              color: "#fff",
              cursor: "pointer",
            }}
          >
            {t("search.close")}
          </button>
        </div>
      </div>
    </div>
  );
}

// ─── Theme Selector ─────────────────────────────────────────────────

const THEMES: { mode: ThemeMode; icon: typeof Sun; label: string; labelFr: string }[] = [
  { mode: "dark", icon: Moon, label: "Dark", labelFr: "Sombre" },
  { mode: "light", icon: Sun, label: "Light", labelFr: "Clair" },
  { mode: "system", icon: Monitor, label: "System", labelFr: "Système" },
];

function ThemeSelector() {
  const theme = useAppStore((s) => s.theme);
  const setTheme = useAppStore((s) => s.setTheme);
  const { locale } = useI18n();

  return (
    <div
      className="rounded-lg"
      style={{
        background: "var(--bg-1)",
        border: "1px solid var(--surface-border)",
        padding: "16px 20px",
        marginBottom: 0,
      }}
    >
      <div className="flex items-center" style={{ gap: 10, marginBottom: 12 }}>
        <Sun size={16} style={{ color: "var(--amber)" }} />
        <p className="text-sm font-medium" style={{ color: "var(--text-0)" }}>
          {locale === "fr" ? "Thème" : "Theme"}
        </p>
      </div>
      <div className="flex" style={{ gap: 8 }}>
        {THEMES.map((t) => {
          const isActive = theme === t.mode;
          const Icon = t.icon;
          return (
            <button
              key={t.mode}
              onClick={() => setTheme(t.mode)}
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
              {locale === "fr" ? t.labelFr : t.label}
            </button>
          );
        })}
      </div>
    </div>
  );
}
