import { useCallback, useState } from 'react';
import { createPortal } from 'react-dom';
import { useBackendStatus, type BackendStatus as Status } from '../../hooks/use-backend-status';
import { useAnchoredPopover } from '../../hooks/use-anchored-popover';

/**
 * Header badge that shows whether the MCP backend is reachable. Click
 * the badge to expand a tooltip with the full message + version + a
 * one-shot retry button.
 *
 * Three visual states:
 *   - online   : green dot, service@version label
 *   - checking : amber dot pulsing, "Vérification…"
 *   - offline  : red dot, "Hors ligne" + click for details
 *
 * Designed to live in `ChatPanel`'s header next to `ProjectSelector`.
 *
 * No accessibility shortcuts beyond the tooltip — this is read-only
 * status, not a primary action. The `aria-label` describes the current
 * state for screen readers.
 */
export function BackendStatus() {
  const health = useBackendStatus();
  const [expanded, setExpanded] = useState(false);
  const closePopover = useCallback(() => setExpanded(false), []);
  const { anchorRef, popoverRef, position, updatePosition } = useAnchoredPopover<
    HTMLButtonElement,
    HTMLDivElement
  >({
    maxWidth: 320,
    minWidth: 288,
    onClose: closePopover,
    open: expanded,
  });

  const toggle = () => {
    const next = !expanded;
    if (next) updatePosition();
    setExpanded(next);
  };

  const popover =
    expanded && position
      ? createPortal(
          <div
            ref={popoverRef}
            role="dialog"
            aria-label="Détails de la connexion au serveur MCP"
            className="popover-panel fixed z-[100] overflow-y-auto rounded-lg border p-3 text-xs shadow-2xl"
            style={position}
          >
            <div className="mb-1 flex items-center gap-2 font-medium text-neutral-100">
              <StatusDot status={health.status} />
              {longLabel(health.status)}
            </div>
            <p className="mb-2 leading-relaxed text-neutral-400">{health.message}</p>
            {health.lastSuccessAt > 0 && health.status !== 'online' && (
              <p className="text-neutral-500">
                Dernière connexion réussie : {new Date(health.lastSuccessAt).toLocaleTimeString()}
              </p>
            )}
            <div className="mt-2 flex justify-end">
              <button
                type="button"
                onClick={closePopover}
                className="control-button rounded border px-2 py-1 text-xs"
              >
                Fermer
              </button>
            </div>
          </div>,
          document.body
        )
      : null;

  return (
    <div className="relative">
      <button
        ref={anchorRef}
        type="button"
        onClick={toggle}
        className="control-button flex items-center gap-1.5 rounded-md border px-2 py-1 text-xs"
        aria-label={ariaLabel(health.status, health.message)}
        aria-expanded={expanded}
      >
        <StatusDot status={health.status} />
        <span className="hidden sm:inline">{shortLabel(health)}</span>
      </button>
      {popover}
    </div>
  );
}

function StatusDot({ status }: { status: Status }) {
  const className =
    status === 'online'
      ? 'h-2 w-2 rounded-full bg-emerald-500'
      : status === 'checking'
        ? 'h-2 w-2 rounded-full bg-amber-400 animate-pulse'
        : 'h-2 w-2 rounded-full bg-red-500';
  return <span className={className} aria-hidden="true" />;
}

function shortLabel(health: ReturnType<typeof useBackendStatus>): string {
  if (health.status === 'online') {
    return health.service ? `${health.service}` : 'Connecté';
  }
  if (health.status === 'checking') return 'Vérification…';
  return 'Hors ligne';
}

function longLabel(status: Status): string {
  if (status === 'online') return 'Serveur connecté';
  if (status === 'checking') return 'Vérification en cours';
  return 'Serveur injoignable';
}

function ariaLabel(status: Status, message: string): string {
  if (status === 'online') return `Serveur connecté. ${message}. Cliquer pour les détails.`;
  if (status === 'checking') return 'Vérification de la connexion au serveur en cours.';
  return `Serveur injoignable. ${message}. Cliquer pour les détails.`;
}
