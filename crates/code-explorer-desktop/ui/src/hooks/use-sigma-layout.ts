import { useCallback, useRef } from "react";
import type { AbstractGraph } from "graphology-types";
import type Sigma from "sigma";

export function useSigmaLayout(
  graphRef: React.MutableRefObject<AbstractGraph | null>,
  sigmaRef: React.MutableRefObject<Sigma | null>,
  setIsLayoutRunning: (running: boolean) => void,
) {
  const layoutRef = useRef<{ kill(): void; start(): void } | null>(null);
  const layoutTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const loadLayoutRuntime = async () => {
    const [fa2Worker, forceAtlas2, noverlap] = await Promise.all([
      import("graphology-layout-forceatlas2/worker"),
      import("graphology-layout-forceatlas2"),
      import("graphology-layout-noverlap"),
    ]);
    return {
      FA2Layout: fa2Worker.default,
      forceAtlas2: forceAtlas2.default,
      noverlap: noverlap.default,
    };
  };

  const runLayout = useCallback(() => {
    const graph = graphRef.current;
    if (!graph || graph.order === 0) return;

    void (async () => {
      const runtime = await loadLayoutRuntime();

      if (layoutRef.current) {
        layoutRef.current.kill();
        layoutRef.current = null;
      }
      if (layoutTimerRef.current) {
        clearTimeout(layoutTimerRef.current);
        layoutTimerRef.current = null;
      }

      const nodeCount = graph.order;
      const settings = {
        ...runtime.forceAtlas2.inferSettings(graph),
        gravity: nodeCount < 500 ? 0.8 : nodeCount < 2000 ? 0.5 : 0.3,
        scalingRatio: nodeCount < 500 ? 15 : nodeCount < 2000 ? 30 : 60,
        slowDown: nodeCount < 500 ? 1 : 3,
        barnesHutOptimize: nodeCount > 200,
      };

      const layout = new runtime.FA2Layout(graph, { settings });
      layoutRef.current = layout;
      layout.start();
      setIsLayoutRunning(true);

      const duration =
        nodeCount > 2000 ? 15000 : nodeCount > 500 ? 8000 : nodeCount > 50 ? 4000 : 2000;
      layoutTimerRef.current = setTimeout(() => {
        if (layoutRef.current) {
          layoutRef.current.kill();
          layoutRef.current = null;
          runtime.noverlap.assign(graph, {
            maxIterations: 20,
            settings: { ratio: 1.1, margin: 5 },
          });
          sigmaRef.current?.refresh();
          requestAnimationFrame(() => {
            const camera = sigmaRef.current?.getCamera();
            if (camera) camera.animatedReset({ duration: 300 });
          });
          setIsLayoutRunning(false);
        }
      }, duration);
    })();
  }, [graphRef, sigmaRef, setIsLayoutRunning]);

  const stopLayout = useCallback(() => {
    if (layoutRef.current) {
      layoutRef.current.kill();
      layoutRef.current = null;
    }
    if (layoutTimerRef.current) {
      clearTimeout(layoutTimerRef.current);
      layoutTimerRef.current = null;
    }
    setIsLayoutRunning(false);
  }, [setIsLayoutRunning]);

  return { runLayout, stopLayout };
}
