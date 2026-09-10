/**
 * use-share-link — bidirectional sync between app state and the URL hash.
 *
 * What is serialized:
 *   - mode (explorer/analyze/chat/manage)
 *   - analyzeView when mode=analyze
 *   - activeRepo
 *   - selectedNodeId
 *   - activeLens
 *
 * Hash is the simplest carrier — Tauri uses the file:// protocol so query
 * strings are flaky; the hash is reliably round-tripped. Web/serve mode
 * gets the same behavior automatically.
 */

import { useEffect, useRef } from "react";
import { useAppStore } from "../stores/app-store";

interface SharedState {
  mode?: string;
  view?: string;
  repo?: string;
  node?: string;
  lens?: string;
}

function encode(state: SharedState): string {
  const params = new URLSearchParams();
  for (const [k, v] of Object.entries(state)) {
    if (v) params.set(k, v);
  }
  return params.toString();
}

function decode(hash: string): SharedState {
  const params = new URLSearchParams(hash.replace(/^#/, ""));
  return {
    mode: params.get("mode") ?? undefined,
    view: params.get("view") ?? undefined,
    repo: params.get("repo") ?? undefined,
    node: params.get("node") ?? undefined,
    lens: params.get("lens") ?? undefined,
  };
}

export function useShareLink() {
  // ── Restore on mount (one-shot) ─────────────────────────────────
  const restoredRef = useRef(false);
  useEffect(() => {
    if (restoredRef.current) return;
    restoredRef.current = true;
    const s = decode(window.location.hash);
    if (!s.mode && !s.repo && !s.node) return;

    const store = useAppStore.getState();
    if (s.mode && ["explorer", "analyze", "chat", "manage"].includes(s.mode)) {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      store.setMode(s.mode as any);
    }
    if (s.view) {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      store.setAnalyzeView(s.view as any);
    }
    if (s.lens) {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      store.setActiveLens(s.lens as any);
    }
    if (s.repo) {
      store.setActiveRepo(s.repo);
    }
    if (s.node) {
      store.setSelectedNodeId(s.node);
    }
  }, []);

  // ── Sync app → hash on changes ─────────────────────────────────
  const mode = useAppStore((s) => s.mode);
  const analyzeView = useAppStore((s) => s.analyzeView);
  const activeLens = useAppStore((s) => s.activeLens);
  const activeRepo = useAppStore((s) => s.activeRepo);
  const selectedNodeId = useAppStore((s) => s.selectedNodeId);

  useEffect(() => {
    const next: SharedState = {
      mode,
      view: mode === "analyze" ? analyzeView : undefined,
      lens: mode === "explorer" ? activeLens : undefined,
      repo: activeRepo ?? undefined,
      node: selectedNodeId ?? undefined,
    };
    const encoded = encode(next);
    const newHash = encoded ? `#${encoded}` : "";
    if (newHash !== window.location.hash) {
      // replaceState avoids polluting the browser history with every selection.
      window.history.replaceState(null, "", `${window.location.pathname}${newHash}`);
    }
  }, [mode, analyzeView, activeLens, activeRepo, selectedNodeId]);
}

/** Returns the current share URL — used by the Copy Share Link action. */
export function buildShareUrl(): string {
  const s = useAppStore.getState();
  const next: SharedState = {
    mode: s.mode,
    view: s.mode === "analyze" ? s.analyzeView : undefined,
    lens: s.mode === "explorer" ? s.activeLens : undefined,
    repo: s.activeRepo ?? undefined,
    node: s.selectedNodeId ?? undefined,
  };
  const encoded = encode(next);
  return `${window.location.origin}${window.location.pathname}${encoded ? `#${encoded}` : ""}`;
}
