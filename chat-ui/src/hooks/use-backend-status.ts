import { useEffect, useRef, useState } from 'react';
import { mcpClient } from '../api/mcp-client';

/**
 * Backend connection status — polls `GET /health` on the MCP server so the
 * UI can warn the user *before* they type a question into a chat that
 * won't reach anywhere.
 *
 * Three states:
 *   - 'checking' : initial load, or transitioning between probes
 *   - 'online'   : last probe returned a valid health response
 *   - 'offline'  : last probe failed (network error, 5xx, timeout)
 *
 * Probe cadence: every `pollIntervalMs` (default 10s). Cheap because
 * `/health` is a flat handler, no DB hit.
 *
 * Why this matters before the Sample meeting: live demos die when the
 * backend isn't running. A red badge in the header > a vague
 * "Failed to fetch" 30 seconds into a question.
 */

export type BackendStatus = 'checking' | 'online' | 'offline';

export interface BackendHealth {
  status: BackendStatus;
  /** Server's own self-report when reachable. */
  service?: string;
  version?: string;
  /** Short human-readable message, ready to drop in a tooltip. */
  message: string;
  /** Timestamp of the last successful probe. 0 if never. */
  lastSuccessAt: number;
  /** Timestamp of the last attempt (success or failure). */
  lastAttemptAt: number;
  /** When the next probe will fire (ms epoch). */
  nextProbeAt: number;
}

interface UseBackendStatusOptions {
  /** Polling interval in milliseconds. Default 10_000. */
  pollIntervalMs?: number;
  /** Initial probe delay — small offset so the dev tools don't all fire at t=0. */
  initialDelayMs?: number;
}

const DEFAULT_OPTIONS: Required<UseBackendStatusOptions> = {
  pollIntervalMs: 10_000,
  initialDelayMs: 500,
};

/**
 * Subscribe a component to backend health. Returns a stable shape that
 * is safe to use in render conditions.
 *
 * Cleanup: cancels the in-flight timer on unmount, no memory leak.
 */
export function useBackendStatus(
  options: UseBackendStatusOptions = {},
): BackendHealth {
  const { pollIntervalMs, initialDelayMs } = { ...DEFAULT_OPTIONS, ...options };

  const [health, setHealth] = useState<BackendHealth>(() => ({
    status: 'checking',
    message: 'Vérification de la connexion au serveur…',
    lastSuccessAt: 0,
    lastAttemptAt: 0,
    nextProbeAt: Date.now() + initialDelayMs,
  }));

  // Persist the timeout id across renders so we never leak a timer.
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    let cancelled = false;
    let probeRunning = false;
    let probeAgain = false;

    const scheduleProbe = (delayMs: number) => {
      if (timerRef.current !== null) {
        clearTimeout(timerRef.current);
      }
      timerRef.current = setTimeout(probe, delayMs);
    };

    const probe = async () => {
      if (probeRunning) {
        probeAgain = true;
        return;
      }

      probeRunning = true;
      const startedAt = Date.now();
      try {
        const result = await mcpClient.health();
        if (cancelled) return;

        const succeededAt = Date.now();
        setHealth({
          status: 'online',
          service: result.service,
          version: result.version,
          message: `Connecté à ${result.service}@${result.version}`,
          lastSuccessAt: succeededAt,
          lastAttemptAt: succeededAt,
          nextProbeAt: succeededAt + pollIntervalMs,
        });
      } catch (err) {
        if (cancelled) return;

        const failedAt = Date.now();
        const reason = err instanceof Error ? err.message : String(err);
        setHealth((prev) => ({
          status: 'offline',
          service: prev.service,
          version: prev.version,
          // Keep the previous success timestamp so the UI can say
          // "déconnecté depuis Xs" instead of "depuis toujours".
          lastSuccessAt: prev.lastSuccessAt,
          lastAttemptAt: failedAt,
          nextProbeAt: failedAt + pollIntervalMs,
          message: formatBackendOfflineMessage(
            reason,
            prev.lastSuccessAt > 0 ? failedAt - prev.lastSuccessAt : null,
          ),
        }));
      } finally {
        probeRunning = false;
        if (!cancelled) {
          if (probeAgain) {
            probeAgain = false;
            scheduleProbe(0);
          } else {
            // Schedule the next probe — re-using a single chained setTimeout
            // is more reliable than setInterval (no overlapping requests, no
            // drift if /health hangs).
            const elapsed = Date.now() - startedAt;
            const wait = Math.max(0, pollIntervalMs - elapsed);
            scheduleProbe(wait);
          }
        }
      }
    };

    const probeWhenVisible = () => {
      if (document.visibilityState === 'hidden') return;
      scheduleProbe(0);
    };

    scheduleProbe(initialDelayMs);
    window.addEventListener('focus', probeWhenVisible);
    window.addEventListener('online', probeWhenVisible);
    document.addEventListener('visibilitychange', probeWhenVisible);

    return () => {
      cancelled = true;
      window.removeEventListener('focus', probeWhenVisible);
      window.removeEventListener('online', probeWhenVisible);
      document.removeEventListener('visibilitychange', probeWhenVisible);
      if (timerRef.current !== null) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }
    };
  }, [pollIntervalMs, initialDelayMs]);

  return health;
}

function formatAge(ms: number): string {
  if (ms < 1000) return 'moins d\'1s';
  if (ms < 60_000) return `${Math.round(ms / 1000)}s`;
  if (ms < 60 * 60_000) return `${Math.round(ms / 60_000)} min`;
  return `${Math.round(ms / (60 * 60_000))} h`;
}

export function formatBackendOfflineMessage(reason: string, offlineForMs: number | null): string {
  const proxyHint = /\b502\b|bad gateway/i.test(reason)
    ? ' Un 502 indique souvent que le proxy Vite pointe vers un backend arrêté ou vers le mauvais port.'
    : '';

  if (offlineForMs !== null) {
    return `Serveur injoignable depuis ${formatAge(offlineForMs)}.${proxyHint} ${reason}`.trim();
  }

  return `Serveur injoignable.${proxyHint} Lance \`.\\code-explorer.cmd chat -RestartBackend\` depuis le dépôt, ou \`.\\code-explorer.cmd doctor\` pour vérifier les ports. (${reason})`;
}
