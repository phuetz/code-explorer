import { useState, useEffect } from "react";
import { isTauri } from "../lib/tauri-env";
import type { FeatureDevArtifact, FeatureDevPhase, FeatureDevPhaseEvent, FeatureDevSection, FeatureDevSectionEvent } from "../lib/tauri-commands";

export type ResearchStep = {
  id: string;
  tool: string;
  status: "pending" | "running" | "completed" | "failed";
  label: string;
};

export function useChatStream() {
  const [streamingText, setStreamingText] = useState("");
  const [isStreaming, setIsStreaming] = useState(false);
  const [activeTools, setActiveTools] = useState<string[]>([]);
  const [toolHistory, setToolHistory] = useState<ResearchStep[]>([]);
  
  const [liveArtifact, setLiveArtifact] = useState<FeatureDevArtifact | null>(null);
  const [activePhase, setActivePhase] = useState<FeatureDevPhase | null>(null);

  // ── Listen for SSE stream chunks from the backend ───────────
  useEffect(() => {
    if (!isTauri()) return;

    let cancelled = false;
    let chunkUnlisten: (() => void) | null = null;
    let doneUnlisten: (() => void) | null = null;
    let toolStartUnlisten: (() => void) | null = null;
    let toolEndUnlisten: (() => void) | null = null;

    import("@tauri-apps/api/event").then((mod) => {
      mod.listen<string>("chat-stream-chunk", (event) => {
        if (cancelled) return;
        setStreamingText((prev: string) => prev + event.payload);
      }).then((fn) => {
        if (cancelled) fn(); else chunkUnlisten = fn;
      });

      mod.listen<string>("tool_execution_start", (event) => {
        if (cancelled) return;
        setActiveTools((prev: string[]) => [...prev, event.payload]);
        setToolHistory((prev) => [
          ...prev,
          { 
            id: `${event.payload}-${Date.now()}`, 
            tool: event.payload, 
            status: "running", 
            label: `Using ${event.payload.replace(/_/g, ' ')}` 
          }
        ]);
      }).then((fn) => {
        if (cancelled) fn(); else toolStartUnlisten = fn;
      });

      mod.listen<string>("tool_execution_end", (event) => {
        if (cancelled) return;
        setActiveTools((prev: string[]) => prev.filter((t: string) => t !== event.payload));
        setToolHistory((prev) => 
          prev.map(step => 
            step.tool === event.payload && step.status === "running" 
              ? { ...step, status: "completed" } 
              : step
          )
        );
      }).then((fn) => {
        if (cancelled) fn(); else toolEndUnlisten = fn;
      });

      mod.listen<void>("chat-stream-done", () => {
        if (cancelled) return;
        setActiveTools([]);
        setIsStreaming(false);
      }).then((fn) => {
        if (cancelled) fn(); else doneUnlisten = fn;
      });
    });

    return () => {
      cancelled = true;
      chunkUnlisten?.();
      doneUnlisten?.();
      toolStartUnlisten?.();
      toolEndUnlisten?.();
    };
  }, []);

  // ── Listen for feature-dev phase / section events ────────────
  useEffect(() => {
    if (!isTauri()) return;

    let cancelled = false;
    let phaseUnlisten: (() => void) | null = null;
    let sectionUnlisten: (() => void) | null = null;

    import("@tauri-apps/api/event").then((mod) => {
      mod
        .listen<FeatureDevPhaseEvent>("feature-dev-phase", (event) => {
          if (cancelled) return;
          const { phase, status } = event.payload;
          if (status === "running") setActivePhase(phase);
          else if (status === "completed" || status === "failed") setActivePhase(null);
        })
        .then((fn) => {
          if (cancelled) fn();
          else phaseUnlisten = fn;
        });

      mod
        .listen<FeatureDevSectionEvent>("feature-dev-section", (event) => {
          if (cancelled) return;
          setLiveArtifact((prev: FeatureDevArtifact | null) => {
            if (!prev || prev.id !== event.payload.artifactId) return prev;
            // Replace-or-append by phase so re-runs of a phase don't duplicate.
            const others = prev.sections.filter(
              (s: FeatureDevSection) => s.phase !== event.payload.section.phase,
            );
            return {
              ...prev,
              sections: [...others, event.payload.section],
            };
          });
        })
        .then((fn) => {
          if (cancelled) fn();
          else sectionUnlisten = fn;
        });
    });

    return () => {
      cancelled = true;
      phaseUnlisten?.();
      sectionUnlisten?.();
    };
  }, []);

  return {
    streamingText,
    setStreamingText,
    isStreaming,
    setIsStreaming,
    activeTools,
    setActiveTools,
    toolHistory,
    setToolHistory,
    liveArtifact,
    setLiveArtifact,
    activePhase,
    setActivePhase
  };
}

