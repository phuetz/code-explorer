import { memo } from "react";
import { Compass, BarChart3, MessageSquare, Settings, Search, ChevronsLeft, ChevronsRight } from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { useAppStore } from "../../stores/app-store";
import type { AppMode } from "../../stores/app-store";
import { commands } from "../../lib/tauri-commands";
import { Tooltip } from "../shared/Tooltip";
import { useI18n } from "../../hooks/use-i18n";

const modes: { mode: AppMode; icon: typeof Compass; i18nKey: string; shortcut: string }[] = [
  { mode: "explorer", icon: Compass, i18nKey: "mode.explorer", shortcut: "Ctrl+1" },
  { mode: "analyze", icon: BarChart3, i18nKey: "mode.analyze", shortcut: "Ctrl+2" },
  { mode: "chat", icon: MessageSquare, i18nKey: "mode.chat", shortcut: "Ctrl+3" },
  { mode: "manage", icon: Settings, i18nKey: "mode.manage", shortcut: "Ctrl+4" },
];

function Badge({ value, color }: { value: string; color: string }) {
  return (
    <span
      className="text-[9px] font-bold px-1.5 py-0.5 rounded-lg leading-[14px] shrink-0"
      style={{
        background: `${color}20`,
        color,
      }}
    >
      {value}
    </span>
  );
}

export const ModeBar = memo(function ModeBar() {
  const { t } = useI18n();
  const mode = useAppStore((s) => s.mode);
  const setMode = useAppStore((s) => s.setMode);
  const setCommandPaletteOpen = useAppStore((s) => s.setCommandPaletteOpen);
  const expanded = useAppStore((s) => s.sidebarExpanded);
  const toggleSidebar = useAppStore((s) => s.toggleSidebar);
  const activeRepo = useAppStore((s) => s.activeRepo);

  // Fetch health score for badge.
  // Scope by `activeRepo` so the badge refetches after a repo switch instead
  // of flashing the stale grade of the previously-open repo for up to 60 s.
  const { data: health } = useQuery({
    queryKey: ["code-health-badge", activeRepo],
    queryFn: () => commands.getCodeHealth(),
    staleTime: 60_000,
    enabled: !!activeRepo,
  });

  // Fetch LLM config for chat badge
  const { data: chatConfig } = useQuery({
    queryKey: ["chat-config"],
    queryFn: () => commands.chatGetConfig(),
    staleTime: 30_000,
  });

  const llmConnected = chatConfig?.apiKey
    ? chatConfig.apiKey.length > 0
    : chatConfig?.provider === "ollama";

  const healthGrade = health?.grade;

  return (
    <div
      className={`flex flex-col items-start py-2 h-full shrink-0 bg-bg-0 border-r border-surface-border transition-[width] duration-280 ease-out overflow-hidden ${
        expanded ? "w-[180px]" : "w-12"
      }`}
    >
      {/* Mode buttons */}
      {modes.map(({ mode: m, icon: Icon, i18nKey, shortcut }) => {
        const active = mode === m;
        const label = t(i18nKey);

        // Per-mode badge
        let badge = null;
        if (m === "analyze" && healthGrade) {
          const color = healthGrade.startsWith("A") ? "#4ade80" : healthGrade.startsWith("B") ? "#fbbf24" : "#fb7185";
          badge = <Badge value={healthGrade} color={color} />;
        }
        if (m === "chat") {
          badge = (
            <span
              className={`w-[7px] h-[7px] rounded-full shrink-0 ${
                llmConnected ? "bg-green" : "bg-rose"
              }`}
            />
          );
        }

        const button = (
          <button
            key={m}
            onClick={() => setMode(m)}
            className={`relative flex items-center w-full mb-0.5 rounded-lg transition-colors focus-visible:ring-2 focus-visible:ring-accent focus-visible:outline-none h-10 gap-2.5 mx-1 border-none cursor-pointer ${
              active ? "text-accent bg-accent-subtle shadow-[var(--glow-accent)]" : "text-text-3 bg-transparent"
            } ${expanded ? "pl-3.5 pr-2.5 justify-start" : "justify-center"}`}
            aria-label={`${label}${shortcut ? ` (${shortcut})` : ""}`}
            aria-current={active ? "page" : undefined}
          >
            {active && (
              <div className="absolute left-0 top-1/2 -translate-y-1/2 rounded-r w-[3px] h-5 bg-accent" />
            )}
            <Icon size={20} className="shrink-0" />
            {expanded && (
              <>
                <span className={`text-[13px] font-body whitespace-nowrap overflow-hidden flex-1 text-left ${active ? "font-semibold" : "font-medium"}`}>
                  {label}
                </span>
                {badge}
              </>
            )}
          </button>
        );

        return expanded ? (
          <div key={m} className="w-full">{button}</div>
        ) : (
          <Tooltip key={m} content={`${label} (${shortcut})`}>
            <div className="relative w-full">
              {button}
              {/* Collapsed badge overlay */}
              {badge && (
                <div className="absolute top-1 right-1 pointer-events-none">
                  {badge}
                </div>
              )}
            </div>
          </Tooltip>
        );
      })}

      <div className="flex-1" />

      {/* Command palette */}
      <Tooltip content={`${t("mode.commandPalette")} (Ctrl+K)`} placement="right">
        <button
          onClick={() => setCommandPaletteOpen(true)}
          className={`flex items-center w-full rounded-lg transition-colors h-9 gap-2.5 mx-1 mb-0.5 border-none cursor-pointer text-text-3 bg-transparent ${
            expanded ? "pl-3.5 pr-2.5 justify-start" : "justify-center"
          }`}
          aria-label={t("mode.commandPalette")}
        >
          <Search size={18} className="shrink-0" />
          {expanded && (
            <span className="text-[12px] whitespace-nowrap text-text-3">
              Ctrl+K
            </span>
          )}
        </button>
      </Tooltip>

      {/* Toggle expand/collapse — always wrap in Tooltip so the collapsed
          icon-only state has discoverable meaning. */}
      <Tooltip content={expanded ? t("mode.collapse") : t("mode.expand")} placement="right">
        <button
          onClick={toggleSidebar}
          className={`flex items-center w-full rounded-lg transition-colors h-8 gap-2 mx-1 mb-1 border-none cursor-pointer text-text-4 bg-transparent ${
            expanded ? "pl-3.5 justify-start" : "justify-center"
          }`}
          aria-label={expanded ? t("mode.collapse") : t("mode.expand")}
        >
          {expanded ? <ChevronsLeft size={16} /> : <ChevronsRight size={16} />}
          {expanded && (
            <span className="text-[11px] text-text-4 whitespace-nowrap">
              {t("mode.collapse")}
            </span>
          )}
        </button>
      </Tooltip>
    </div>
  );
});
