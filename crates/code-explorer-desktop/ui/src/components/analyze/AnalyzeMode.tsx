import { lazy, Suspense } from "react";
import { useQuery } from "@tanstack/react-query";
import { AnimatePresence } from "framer-motion";
import { useAppStore } from "../../stores/app-store";
import { useI18n } from "../../hooks/use-i18n";
import { commands } from "../../lib/tauri-commands";
import { AnimatedPage } from "../shared/motion";
import { AnalyzeNav } from "./AnalyzeNav";
import { LoadingOrbs } from "../shared/LoadingOrbs";

import { AlertCircle } from "lucide-react";

const OverviewView = lazy(() =>
  import("./OverviewView").then((m) => ({ default: m.OverviewView })),
);
const ProcessFlowsView = lazy(() =>
  import("./ProcessFlowsView").then((m) => ({ default: m.ProcessFlowsView })),
);
const HotspotsView = lazy(() =>
  import("../git/HotspotsView").then((m) => ({ default: m.HotspotsView })),
);
const CouplingView = lazy(() =>
  import("../git/CouplingView").then((m) => ({ default: m.CouplingView })),
);
const OwnershipView = lazy(() =>
  import("../git/OwnershipView").then((m) => ({ default: m.OwnershipView })),
);
const CoverageView = lazy(() =>
  import("../coverage/CoverageView").then((m) => ({ default: m.CoverageView })),
);
const DiagramView = lazy(() =>
  import("../diagram/DiagramView").then((m) => ({ default: m.DiagramView })),
);
const ReportView = lazy(() =>
  import("../report/ReportView").then((m) => ({ default: m.ReportView })),
);
const CodeHealthCard = lazy(() =>
  import("../health/CodeHealthCard").then((m) => ({ default: m.CodeHealthCard })),
);
const SnapshotsPanel = lazy(() =>
  import("./SnapshotsPanel").then((m) => ({ default: m.SnapshotsPanel })),
);
const CyclesView = lazy(() =>
  import("./CyclesView").then((m) => ({ default: m.CyclesView })),
);
const ClonesView = lazy(() =>
  import("./ClonesView").then((m) => ({ default: m.ClonesView })),
);
const TodosView = lazy(() =>
  import("./TodosView").then((m) => ({ default: m.TodosView })),
);
const ComplexityView = lazy(() =>
  import("./ComplexityView").then((m) => ({ default: m.ComplexityView })),
);
const EndpointsView = lazy(() =>
  import("./EndpointsView").then((m) => ({ default: m.EndpointsView })),
);
const SchemaView = lazy(() =>
  import("./SchemaView").then((m) => ({ default: m.SchemaView })),
);
const EnvVarsView = lazy(() =>
  import("./EnvVarsView").then((m) => ({ default: m.EnvVarsView })),
);

const analyzeFallback = (
  <div className="flex items-center justify-center h-full">
    <LoadingOrbs />
  </div>
);

// Shared error state for analysis views
function AnalyzeError({ error }: { error: unknown }) {
  const { t } = useI18n();
  return (
    <div className="flex flex-col items-center justify-center h-full p-8 text-center">
      <AlertCircle size={40} style={{ color: "var(--rose)", marginBottom: 16 }} />
      <h3 style={{ fontFamily: "var(--font-display)", fontSize: 18, fontWeight: 600, color: "var(--text-0)", marginBottom: 8 }}>
        {t("analyze.errorTitle")}
      </h3>
      <p style={{ fontSize: 13, color: "var(--text-3)", maxWidth: 400, lineHeight: 1.5 }}>
        {String(error)}
      </p>
    </div>
  );
}

// Wrapper views that own the data fetching for git analytics.
// Query keys MUST include `activeRepo` so switching repos doesn't show stale
// data from the previously active repo. Without it, TanStack Query treats the
// query as the same across repos and serves cached data within `staleTime`.
function HotspotsWrapper({ activeRepo }: { activeRepo: string | null }) {
  const { data, isLoading, error } = useQuery({
    queryKey: ["git-hotspots", activeRepo],
    queryFn: () => commands.getHotspots(90),
    staleTime: 60_000,
  });
  if (isLoading) return analyzeFallback;
  if (error) return <AnalyzeError error={error} />;
  return <HotspotsView data={data ?? []} loading={false} />;
}

function CouplingWrapper({ activeRepo }: { activeRepo: string | null }) {
  const { data, isLoading, error } = useQuery({
    queryKey: ["git-coupling", activeRepo],
    queryFn: () => commands.getCoupling(3),
    staleTime: 60_000,
  });
  if (isLoading) return analyzeFallback;
  if (error) return <AnalyzeError error={error} />;
  return <CouplingView data={data ?? []} loading={false} />;
}

function OwnershipWrapper({ activeRepo }: { activeRepo: string | null }) {
  const { data, isLoading, error } = useQuery({
    queryKey: ["git-ownership", activeRepo],
    queryFn: () => commands.getOwnership(),
    staleTime: 60_000,
  });
  if (isLoading) return analyzeFallback;
  if (error) return <AnalyzeError error={error} />;
  return <OwnershipView data={data ?? []} loading={false} />;
}

export function AnalyzeMode() {
  const { t } = useI18n();
  const analyzeView = useAppStore((s) => s.analyzeView);
  const activeRepo = useAppStore((s) => s.activeRepo);

  if (!activeRepo) {
    return (
      <div className="flex items-center justify-center h-full" style={{ color: "var(--text-2)" }}>
        <p style={{ fontFamily: "var(--font-display)", fontSize: 18 }}>{t("analyze.openRepo")}</p>
      </div>
    );
  }

  const renderView = () => {
    switch (analyzeView) {
      case "overview":   return <OverviewView />;
      case "processes":  return <ProcessFlowsView />;
      case "hotspots":   return <HotspotsWrapper activeRepo={activeRepo} />;
      case "coupling":   return <CouplingWrapper activeRepo={activeRepo} />;
      case "ownership":  return <OwnershipWrapper activeRepo={activeRepo} />;
      case "coverage":   return <CoverageView />;
      case "diagram":    return <DiagramView />;
      case "report":     return <ReportView />;
      case "snapshots":  return <SnapshotsPanel />;
      case "cycles":     return <CyclesView />;
      case "clones":     return <ClonesView />;
      case "todos":      return <TodosView />;
      case "complexity": return <ComplexityView />;
      case "endpoints":  return <EndpointsView />;
      case "schema":     return <SchemaView />;
      case "env_vars":   return <EnvVarsView />;
      case "health":
        return (
          <div className="p-6" style={{ maxWidth: 800, margin: "0 auto" }}>
            <h2 style={{ fontFamily: "var(--font-display)", fontSize: 20, fontWeight: 600, color: "var(--text-0)", marginBottom: 16 }}>
              {t("analyze.codeHealth")}
            </h2>
            <CodeHealthCard />
          </div>
        );
      default: return <OverviewView />;
    }
  };

  return (
    <div className="flex h-full">
      <AnalyzeNav />
      <div className="flex-1 min-w-0 overflow-auto">
        <AnimatePresence mode="wait">
          <AnimatedPage key={analyzeView}>
            <Suspense fallback={analyzeFallback}>
              {renderView()}
            </Suspense>
          </AnimatedPage>
        </AnimatePresence>
      </div>
    </div>
  );
}
