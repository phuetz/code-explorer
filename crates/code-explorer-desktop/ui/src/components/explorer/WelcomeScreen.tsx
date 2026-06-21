import { useCallback } from "react";
import { Network, FolderSearch, Sparkles, ArrowRight } from "lucide-react";
import { useRepos } from "../../hooks/use-tauri-query";
import { useAppStore } from "../../stores/app-store";
import { commands } from "../../lib/tauri-commands";
import { useI18n } from "../../hooks/use-i18n";
import { AnimatedCard, StaggerContainer, StaggerItem } from "../shared/motion";
import { isTauri } from "../../lib/tauri-env";
import { toast } from "sonner";

const STEPS = [
  {
    icon: FolderSearch,
    titleKey: "repos.onboarding.step1.title",
    descKey: "repos.onboarding.step1.desc",
    color: "var(--accent)",
  },
  {
    icon: Sparkles,
    titleKey: "repos.onboarding.step2.title",
    descKey: "repos.onboarding.step2.desc",
    color: "var(--green)",
  },
  {
    icon: Network,
    titleKey: "repos.onboarding.step3.title",
    descKey: "repos.onboarding.step3.desc",
    color: "#bb9af7",
  },
];

export function WelcomeScreen() {
  const { t } = useI18n();
  const { data: repos } = useRepos();
  const setActiveRepo = useAppStore((s) => s.setActiveRepo);
  const setMode = useAppStore((s) => s.setMode);

  const handleOpenRepo = useCallback(async (name: string) => {
    try {
      await commands.openRepo(name);
      setActiveRepo(name);
    } catch (e) {
      console.error("Failed to open repo:", e);
    }
  }, [setActiveRepo]);

  const handleAnalyze = useCallback(async () => {
    if (!isTauri()) {
      toast.info(t("welcome.tauriRequired"));
      return;
    }
    setMode("manage");
  }, [setMode, t]);

  return (
    <div
      className="flex flex-col items-center justify-center h-full"
      style={{
        background: "radial-gradient(ellipse at 50% 40%, rgba(106,161,248,0.06) 0%, transparent 60%)",
        overflow: "auto",
        padding: "40px 24px",
      }}
    >
      {/* Hero */}
      <div style={{ textAlign: "center", marginBottom: 48 }}>
        <div
          style={{
            width: 64,
            height: 64,
            borderRadius: 16,
            background: "linear-gradient(135deg, var(--accent) 0%, #bb9af7 100%)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            margin: "0 auto 20px",
            boxShadow: "0 0 40px rgba(106,161,248,0.2)",
          }}
        >
          <Network size={32} style={{ color: "white" }} />
        </div>
        <h2
          style={{
            fontFamily: "var(--font-display)",
            fontSize: 28,
            fontWeight: 700,
            color: "var(--text-0)",
            marginBottom: 8,
          }}
        >
          Code Explorer
        </h2>
        <p
          style={{
            fontSize: 14,
            color: "var(--text-3)",
            maxWidth: 400,
            margin: "0 auto",
            lineHeight: 1.5,
          }}
        >
          {t("a11y.codeIntelligencePlatform")}
        </p>
      </div>

      {/* Steps */}
      <StaggerContainer
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(3, 1fr)",
          gap: 16,
          maxWidth: 720,
          width: "100%",
          marginBottom: 40,
        }}
      >
        {STEPS.map((step) => (
          <StaggerItem key={step.titleKey}>
            <AnimatedCard
              style={{
                padding: "24px 20px",
                borderRadius: "var(--radius-xl)",
                background: "var(--glass-bg)",
                border: "1px solid var(--glass-border)",
                backdropFilter: "blur(var(--glass-blur))",
                textAlign: "center",
                height: "100%",
              }}
            >
              <div
                style={{
                  width: 44,
                  height: 44,
                  borderRadius: 12,
                  background: `${step.color}15`,
                  border: `1px solid ${step.color}30`,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  margin: "0 auto 14px",
                }}
              >
                <step.icon size={22} style={{ color: step.color }} />
              </div>
              <div
                style={{
                  fontFamily: "var(--font-display)",
                  fontSize: 14,
                  fontWeight: 600,
                  color: "var(--text-1)",
                  marginBottom: 6,
                }}
              >
                {t(step.titleKey)}
              </div>
              <div style={{ fontSize: 12, color: "var(--text-3)", lineHeight: 1.5 }}>
                {t(step.descKey)}
              </div>
            </AnimatedCard>
          </StaggerItem>
        ))}
      </StaggerContainer>

      {/* CTA */}
      <button
        onClick={handleAnalyze}
        style={{
          padding: "12px 32px",
          borderRadius: "var(--radius-lg)",
          background: "linear-gradient(135deg, var(--accent) 0%, #bb9af7 100%)",
          color: "white",
          fontFamily: "var(--font-display)",
          fontSize: 14,
          fontWeight: 600,
          border: "none",
          cursor: "pointer",
          boxShadow: "0 4px 24px rgba(106,161,248,0.25)",
          transition: "transform var(--transition-fast), box-shadow var(--transition-fast)",
          marginBottom: 40,
        }}
        onMouseEnter={(e) => {
          e.currentTarget.style.transform = "translateY(-2px)";
          e.currentTarget.style.boxShadow = "0 8px 32px rgba(106,161,248,0.35)";
        }}
        onMouseLeave={(e) => {
          e.currentTarget.style.transform = "translateY(0)";
          e.currentTarget.style.boxShadow = "0 4px 24px rgba(106,161,248,0.25)";
        }}
      >
        {t("analyze.analyzeProject")}
      </button>

      {/* Recent repos */}
      {repos && repos.length > 0 && (
        <div style={{ maxWidth: 480, width: "100%" }}>
          <div
            style={{
              fontSize: 11,
              fontWeight: 600,
              color: "var(--text-3)",
              textTransform: "uppercase",
              letterSpacing: "0.05em",
              marginBottom: 10,
            }}
          >
            {t("repos.title")}
          </div>
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            {repos.slice(0, 5).map((repo) => (
              <button
                key={repo.name}
                onClick={() => handleOpenRepo(repo.name)}
                style={{
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  padding: "10px 14px",
                  borderRadius: "var(--radius-md)",
                  background: "var(--surface)",
                  border: "1px solid var(--surface-border)",
                  cursor: "pointer",
                  transition: "background var(--transition-fast), border-color var(--transition-fast)",
                  textAlign: "left",
                  width: "100%",
                  color: "inherit",
                  fontFamily: "inherit",
                }}
                onMouseEnter={(e) => {
                  e.currentTarget.style.background = "var(--surface-hover)";
                  e.currentTarget.style.borderColor = "var(--surface-border-hover)";
                }}
                onMouseLeave={(e) => {
                  e.currentTarget.style.background = "var(--surface)";
                  e.currentTarget.style.borderColor = "var(--surface-border)";
                }}
              >
                <div>
                  <div style={{ fontSize: 13, fontWeight: 600, color: "var(--text-1)" }}>
                    {repo.name}
                  </div>
                  <div style={{ fontSize: 11, color: "var(--text-3)", marginTop: 2 }}>
                    {repo.nodes ?? 0} {t("status.nodes")} &middot; {repo.files ?? 0} {t("files.title").toLowerCase()}
                  </div>
                </div>
                <ArrowRight size={14} style={{ color: "var(--text-3)" }} />
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
