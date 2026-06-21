import {
  MessageSquare,
  Code2,
  Network,
  Skull,
  Zap,
  Flame,
  Share2,
  BookMarked,
  Map,
  Link2,
  ClipboardList,
  Puzzle,
  type LucideIcon,
} from "lucide-react";
import { useI18n } from "../../hooks/use-i18n";
import { StaggerContainer, StaggerItem } from "../shared/motion";
import { useQuery } from "@tanstack/react-query";
import { commands } from "../../lib/tauri-commands";
import { useAppStore } from "../../stores/app-store";
import { toast } from "sonner";

const SUGGESTIONS = [
  {
    icon: Zap,
    textKey: "chat.suggestion.entryPoints",
    fallback: "What are the main entry points of the application?",
    color: "var(--amber)",
  },
  {
    icon: Code2,
    textKey: "chat.suggestion.complex",
    fallback: "Show me the most complex methods",
    color: "var(--accent)",
  },
  {
    icon: Network,
    textKey: "chat.suggestion.architecture",
    fallback: "Explain the overall architecture",
    color: "var(--green)",
  },
  {
    icon: Skull,
    textKey: "chat.suggestion.deadCode",
    fallback: "Is there any dead code?",
    color: "var(--rose)",
  },
];

interface Props {
  onSelect: (question: string) => void;
}

export function ChatSuggestions({ onSelect }: Props) {
  const { t } = useI18n();
  const activeRepo = useAppStore((s) => s.activeRepo);

  const { data: hotspots } = useQuery({
    queryKey: ["git-hotspots", activeRepo],
    queryFn: () => commands.getHotspots(90),
    enabled: !!activeRepo,
  });

  const dynamicSuggestions = [
    ...SUGGESTIONS,
    {
      icon: Share2,
      textKey: "chat.suggestion.diagram",
      fallback: "Generate a Mermaid diagram of the architecture",
      color: "#bb9af7",
    },
    ...(hotspots && hotspots.length > 5 ? [{
      icon: Flame,
      textKey: "chat.suggestion.hotspots",
      fallback: "What are the main hotspots (most changed files) in the code?",
      color: "#ff9e64",
    }] : [])
  ].slice(0, 4); // Keep at most 4 suggestions to fit grid

  // ── MCP prompt recipes (P1.3) ───────────────────────────────────
  // The 6 MCP prompts encode validated tool-chains (e.g. analyze_hotspots →
  // hotspots + coupling + ownership → recommend refactor priorities).
  // Surfaced as one-click recipes so the chat reuses the orchestration
  // instead of the LLM re-inventing it on every conversation.
  const { data: promptList } = useQuery({
    queryKey: ["chat-mcp-prompts"],
    queryFn: () => commands.listChatPrompts(),
    staleTime: Infinity, // prompt list is static, no need to re-fetch
  });

  const handleRecipe = async (
    name: string,
    args: Array<{ name: string; required: boolean; description: string }>,
  ) => {
    const filled: Record<string, string> = {};
    for (const arg of args) {
      if (!arg.required) continue;
      // Lightweight prompt — full forms can come later if a recipe needs more.
      const value = window.prompt(`${arg.name}: ${arg.description}`);
      if (!value || !value.trim()) {
        toast.info(`Recipe cancelled (missing '${arg.name}')`);
        return;
      }
      filled[arg.name] = value.trim();
    }
    try {
      const text = await commands.getChatPrompt(name, filled);
      onSelect(text);
    } catch (err) {
      toast.error(`Failed to load recipe '${name}': ${String(err)}`);
    }
  };

  const recipeIcons: Record<string, LucideIcon> = {
    detect_impact: Network,
    generate_map: Map,
    analyze_hotspots: Flame,
    find_dead_code: Skull,
    trace_dependencies: Link2,
    describe_process: ClipboardList,
  };

  return (
    <div
      className="flex flex-col items-center justify-center h-full"
      style={{
        padding: "40px 24px",
        background: "var(--bg-0)",
      }}
    >
      <div
        style={{
          width: 56,
          height: 56,
          borderRadius: 14,
          background: "var(--accent-subtle)",
          border: "1px solid var(--accent-border)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          marginBottom: 16,
        }}
      >
        <MessageSquare size={26} style={{ color: "var(--accent)" }} />
      </div>
      <h3
        style={{
          fontFamily: "var(--font-display)",
          fontSize: 18,
          fontWeight: 600,
          color: "var(--text-1)",
          marginBottom: 6,
        }}
      >
        {t("chat.welcomeTitle") || "Ask about your code"}
      </h3>
      <p
        style={{
          fontSize: 13,
          color: "var(--text-3)",
          marginBottom: 28,
          textAlign: "center",
          maxWidth: 360,
        }}
      >
        {t("chat.welcomeDesc") || "Ask questions about architecture, dependencies, or code quality."}
      </p>
      <p
        style={{
          fontSize: 12,
          color: "var(--accent)",
          marginBottom: 28,
          textAlign: "center",
          maxWidth: 380,
          lineHeight: 1.5,
        }}
      >
        {t("chat.welcomeValue") ||
          "Code Explorer gives this chat your whole codebase as a graph — so answers are precise and cost a fraction of the context."}
      </p>

      <StaggerContainer
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fit, minmax(200px, 1fr))",
          gap: 10,
          maxWidth: 500,
          width: "100%",
        }}
      >
        {dynamicSuggestions.map((s) => {
          const text = t(s.textKey) || s.fallback;
          return (
            <StaggerItem key={s.textKey}>
              <button
                onClick={() => onSelect(text)}
                className="flex items-start gap-3 w-full rounded-xl transition-all"
                style={{
                  padding: "14px 16px",
                  background: "var(--surface)",
                  border: "1px solid var(--surface-border)",
                  cursor: "pointer",
                  textAlign: "left",
                  color: "inherit",
                  fontFamily: "inherit",
                }}
                onMouseEnter={(e) => {
                  e.currentTarget.style.background = "var(--surface-hover)";
                  e.currentTarget.style.borderColor = "var(--surface-border-hover)";
                  e.currentTarget.style.transform = "translateY(-1px)";
                }}
                onMouseLeave={(e) => {
                  e.currentTarget.style.background = "var(--surface)";
                  e.currentTarget.style.borderColor = "var(--surface-border)";
                  e.currentTarget.style.transform = "translateY(0)";
                }}
              >
                <s.icon size={18} style={{ color: s.color, flexShrink: 0, marginTop: 1 }} />
                <span style={{ fontSize: 12, color: "var(--text-2)", lineHeight: 1.4 }}>
                  {text}
                </span>
              </button>
            </StaggerItem>
          );
        })}
      </StaggerContainer>

      {promptList && promptList.prompts && promptList.prompts.length > 0 && (
        <>
          <div
            style={{
              marginTop: 28,
              marginBottom: 12,
              display: "flex",
              alignItems: "center",
              gap: 8,
              color: "var(--text-3)",
              fontSize: 11,
              textTransform: "uppercase",
              letterSpacing: "0.05em",
            }}
          >
            <BookMarked size={12} />
            <span>{t("chat.mcpRecipes") || "Recettes éprouvées (MCP)"}</span>
          </div>
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fit, minmax(160px, 1fr))",
              gap: 8,
              maxWidth: 500,
              width: "100%",
            }}
          >
            {promptList.prompts.map((p) => {
              const Icon = recipeIcons[p.name] ?? Puzzle;
              return (
                <button
                  key={p.name}
                  onClick={() => handleRecipe(p.name, p.arguments)}
                  title={p.description}
                  className="flex items-center gap-2 rounded-lg transition-all"
                  style={{
                    padding: "8px 10px",
                    background: "var(--surface)",
                    border: "1px solid var(--surface-border)",
                    cursor: "pointer",
                    textAlign: "left",
                    fontSize: 11,
                    color: "var(--text-2)",
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
                  <Icon size={13} style={{ color: "var(--accent)", flexShrink: 0 }} />
                  <span style={{ fontFamily: "var(--font-mono)", fontSize: 10.5 }}>
                    {p.name}
                  </span>
                </button>
              );
            })}
          </div>
        </>
      )}
    </div>
  );
}
