import { useEffect, useRef } from "react";
import { Copy, Sparkles, Hammer, ShieldCheck, Skull, Replace } from "lucide-react";
import { Tooltip } from "../shared/Tooltip";
import { useI18n } from "../../hooks/use-i18n";
import { useAppStore } from "../../stores/app-store";

export interface ContextMenuData {
  x: number;
  y: number;
  nodeId: string;
  name: string;
  filePath: string;
}

/**
 * Identifier of the AI action invoked from the context menu. The graph host
 * routes these to the chat panel: most switch the chat to a specific mode
 * and seed the input with a node-scoped question.
 */
export type AiAction = "explain" | "feature_dev" | "code_review" | "dead_check";

interface GraphContextMenuProps {
  contextMenu: ContextMenuData | null;
  onClose: () => void;
  onGoToDefinition: (filePath: string, name: string) => void;
  onFindReferences: (name: string) => void;
  onViewImpact: (nodeId: string, name: string) => void;
  onExpandNeighbors: () => void;
  onHideNode: (nodeId: string) => void;
  onCopyName: (name: string) => void;
  onCopyFilePath: (filePath: string) => void;
  /** Triggered when the user picks an AI action — the host dispatches it. */
  onAiAction?: (action: AiAction, ctx: ContextMenuData) => void;
}

export function GraphContextMenu({
  contextMenu,
  onClose,
  onGoToDefinition,
  onFindReferences,
  onViewImpact,
  onExpandNeighbors,
  onHideNode,
  onCopyName,
  onCopyFilePath,
  onAiAction,
}: GraphContextMenuProps) {
  const { t, tt } = useI18n();
  const menuRef = useRef<HTMLDivElement>(null);
  const firstItemRef = useRef<HTMLButtonElement>(null);

  // Auto-focus first item on open
  useEffect(() => {
    if (contextMenu) {
      firstItemRef.current?.focus();
    }
  }, [contextMenu]);

  // Close on click outside, scroll, or resize
  useEffect(() => {
    if (!contextMenu) return;
    const handleOutside = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        onClose();
      }
    };
    const handleDismiss = () => onClose();
    document.addEventListener("mousedown", handleOutside);
    window.addEventListener("scroll", handleDismiss, true);
    window.addEventListener("resize", handleDismiss);
    return () => {
      document.removeEventListener("mousedown", handleOutside);
      window.removeEventListener("scroll", handleDismiss, true);
      window.removeEventListener("resize", handleDismiss);
    };
  }, [contextMenu, onClose]);

  // Arrow key navigation
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (!menuRef.current) return;
    const items = Array.from(
      menuRef.current.querySelectorAll<HTMLButtonElement>('button[role="menuitem"]'),
    );
    const idx = items.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === "ArrowDown") {
      e.preventDefault();
      items[(idx + 1) % items.length]?.focus();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      items[(idx - 1 + items.length) % items.length]?.focus();
    } else if (e.key === "Escape") {
      onClose();
    }
  };

  if (!contextMenu) return null;

  // Clamp to viewport
  const menuW = 220;
  const menuH = 300;
  const clampedX = Math.min(contextMenu.x, window.innerWidth - menuW - 8);
  const clampedY = Math.min(contextMenu.y, window.innerHeight - menuH - 8);

  return (
    <div
      ref={menuRef}
      role="menu"
      aria-label="Node context menu"
      className="absolute z-50 pointer-events-auto rounded-lg text-xs"
      style={{
        left: Math.max(8, clampedX),
        top: Math.max(8, clampedY),
        backgroundColor: "var(--bg-3)",
        border: "1px solid var(--surface-border-hover)",
        boxShadow: "var(--shadow-lg)",
        minWidth: "200px",
        overflow: "hidden",
      }}
      onKeyDown={handleKeyDown}
    >
      <Tooltip content={tt("graph.contextMenu.goToDefinition").tip}>
        <ContextMenuButton
          ref={firstItemRef}
          onClick={() => {
            onGoToDefinition(contextMenu.filePath, contextMenu.name);
            onClose();
          }}
        >
          {tt("graph.contextMenu.goToDefinition").label}
        </ContextMenuButton>
      </Tooltip>
      <Tooltip content={tt("graph.contextMenu.findReferences").tip}>
        <ContextMenuButton
          onClick={() => {
            onFindReferences(contextMenu.name);
            onClose();
          }}
        >
          {tt("graph.contextMenu.findReferences").label}
        </ContextMenuButton>
      </Tooltip>
      <div
        style={{ borderTop: "1px solid var(--surface-border)", margin: "4px 0" }}
      />
      <ContextMenuButton
        onClick={() => {
          onViewImpact(contextMenu.nodeId, contextMenu.name);
          onClose();
        }}
      >
        {t("graph.viewImpact")}
      </ContextMenuButton>
      <Tooltip content={tt("graph.contextMenu.expandNeighbors").tip}>
        <ContextMenuButton onClick={() => { onExpandNeighbors(); onClose(); }}>
          {tt("graph.contextMenu.expandNeighbors").label}
        </ContextMenuButton>
      </Tooltip>
      <Tooltip content={tt("graph.contextMenu.hideNode").tip}>
        <ContextMenuButton
          onClick={() => {
            onHideNode(contextMenu.nodeId);
            onClose();
          }}
        >
          {tt("graph.contextMenu.hideNode").label}
        </ContextMenuButton>
      </Tooltip>
      {onAiAction && (
        <>
          <div
            style={{
              borderTop: "1px solid var(--surface-border)",
              margin: "4px 0",
            }}
          />
          <div
            style={{
              padding: "4px 16px 2px",
              fontSize: 9,
              fontWeight: 700,
              textTransform: "uppercase",
              color: "var(--text-3)",
              letterSpacing: 0.5,
            }}
          >
            AI Inspector
          </div>
          <ContextMenuButton
            onClick={() => {
              onAiAction("explain", contextMenu);
              onClose();
            }}
          >
            <Sparkles size={14} style={{ marginRight: 8, color: "var(--accent)" }} />
            Explain this symbol
          </ContextMenuButton>
          <ContextMenuButton
            onClick={() => {
              onAiAction("feature_dev", contextMenu);
              onClose();
            }}
          >
            <Hammer size={14} style={{ marginRight: 8, color: "var(--amber)" }} />
            Design changes around this
          </ContextMenuButton>
          <ContextMenuButton
            onClick={() => {
              onAiAction("code_review", contextMenu);
              onClose();
            }}
          >
            <ShieldCheck size={14} style={{ marginRight: 8, color: "var(--green)" }} />
            Review impact of changes
          </ContextMenuButton>
          <ContextMenuButton
            onClick={() => {
              onAiAction("dead_check", contextMenu);
              onClose();
            }}
          >
            <Skull size={14} style={{ marginRight: 8, color: "var(--rose)" }} />
            Dead-code check
          </ContextMenuButton>
          <ContextMenuButton
            onClick={() => {
              useAppStore.getState().openRenameModal(contextMenu.name);
              onClose();
            }}
          >
            <Replace size={14} style={{ marginRight: 8, color: "var(--accent)" }} />
            Rename refactor…
          </ContextMenuButton>
        </>
      )}
      <div
        style={{ borderTop: "1px solid var(--surface-border)", margin: "4px 0" }}
      />
      <Tooltip content={tt("graph.contextMenu.copyName").tip}>
        <ContextMenuButton
          onClick={() => {
            onCopyName(contextMenu.name);
            onClose();
          }}
        >
          <Copy size={14} style={{ marginRight: "8px" }} />
          {tt("graph.contextMenu.copyName").label}
        </ContextMenuButton>
      </Tooltip>
      <Tooltip content={tt("graph.contextMenu.copyFilePath").tip}>
        <ContextMenuButton
          onClick={() => {
            onCopyFilePath(contextMenu.filePath);
            onClose();
          }}
        >
          <Copy size={14} style={{ marginRight: "8px" }} />
          {tt("graph.contextMenu.copyFilePath").label}
        </ContextMenuButton>
      </Tooltip>
    </div>
  );
}

// ─── Context menu button helper ───────────────────────────────────

const ContextMenuButton = ({
  onClick,
  children,
  ref,
}: {
  onClick: () => void;
  children: React.ReactNode;
  ref?: React.Ref<HTMLButtonElement>;
}) => (
  <button
    ref={ref}
    role="menuitem"
    onClick={onClick}
    className="w-full text-left transition-colors flex items-center hover:brightness-125"
    style={{
      padding: "8px 16px",
      color: "var(--text-2)",
      backgroundColor: "var(--bg-3)",
    }}
  >
    {children}
  </button>
);
