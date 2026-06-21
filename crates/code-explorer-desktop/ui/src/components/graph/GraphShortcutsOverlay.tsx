import { useI18n } from "../../hooks/use-i18n";

interface GraphShortcutsOverlayProps {
  visible: boolean;
}

export function GraphShortcutsOverlay({ visible }: GraphShortcutsOverlayProps) {
  const { t } = useI18n();

  if (!visible) return null;

  const shortcuts: [string, string][] = [
    ["Ctrl+G", t("graph.shortcut.goToSymbol")],
    ["Ctrl+E", t("graph.shortcut.exportPng")],
    ["Ctrl+Shift+S", t("graph.shortcut.screenshot")],
    ["Ctrl+=/\u2212/0", t("graph.shortcut.zoomInOutFit")],
    ["Ctrl+\\", t("graph.shortcut.clearSelection")],
    ["Escape", t("graph.shortcut.clearSelection")],
    ["Double-click", t("graph.shortcut.focusSubgraph")],
    ["?", t("graph.shortcut.toggleHelp")],
  ];

  return (
    <div
      className="absolute z-30 rounded-xl"
      style={{
        top: 60,
        right: 16,
        padding: "16px 20px",
        background: "var(--bg-2)",
        border: "1px solid var(--surface-border)",
        backdropFilter: "blur(12px)",
        boxShadow: "var(--shadow-lg)",
        fontSize: 11,
        color: "var(--text-2)",
        minWidth: 220,
      }}
    >
      <div style={{ fontWeight: 600, color: "var(--text-0)", marginBottom: 8, fontSize: 12 }}>
        {t("graph.keyboardShortcuts")}
      </div>
      {shortcuts.map(([key, desc]) => (
        <div key={key} className="flex justify-between py-1" style={{ gap: 16 }}>
          <kbd
            className="font-mono text-[10px] rounded px-1.5 py-0.5"
            style={{ background: "var(--bg-3)", color: "var(--text-1)" }}
          >
            {key}
          </kbd>
          <span>{desc}</span>
        </div>
      ))}
    </div>
  );
}
