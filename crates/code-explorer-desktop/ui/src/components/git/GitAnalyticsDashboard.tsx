/**
 * Git Analytics Dashboard — Hotspots, Coupling, and Ownership views.
 */

import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Flame, Link2, Users } from "lucide-react";
import { commands } from "../../lib/tauri-commands";
import { useI18n } from "../../hooks/use-i18n";
import { useAppStore } from "../../stores/app-store";
import { AnimatedPage } from "../shared/motion";
import { HotspotsView } from "./HotspotsView";
import { CouplingView } from "./CouplingView";
import { OwnershipView } from "./OwnershipView";

type Tab = "hotspots" | "coupling" | "ownership";

const TABS: { id: Tab; i18nKey: string; icon: typeof Flame }[] = [
  { id: "hotspots", i18nKey: "git.hotspots", icon: Flame },
  { id: "coupling", i18nKey: "git.coupling", icon: Link2 },
  { id: "ownership", i18nKey: "git.ownership", icon: Users },
];

export function GitAnalyticsDashboard() {
  const { t } = useI18n();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const [tab, setTab] = useState<Tab>("hotspots");

  // All git analytics queries scope on `activeRepo` to avoid serving stale
  // cached results from the previously-active repo on switch.
  const { data: hotspots, isLoading: loadingH } = useQuery({
    queryKey: ["git-hotspots", activeRepo],
    queryFn: () => commands.getHotspots(90),
    staleTime: 60_000,
  });

  const { data: coupling, isLoading: loadingC } = useQuery({
    queryKey: ["git-coupling", activeRepo],
    queryFn: () => commands.getCoupling(3),
    staleTime: 60_000,
  });

  const { data: ownership, isLoading: loadingO } = useQuery({
    queryKey: ["git-ownership", activeRepo],
    queryFn: () => commands.getOwnership(),
    staleTime: 60_000,
  });

  return (
    <AnimatedPage className="h-full flex flex-col">
      {/* Tab bar */}
      <div
        className="flex items-center gap-1 px-4 py-2 border-b"
        style={{
          backgroundColor: "var(--bg-2)",
          borderColor: "var(--border)",
        }}
      >
        {TABS.map((tb) => (
          <button
            key={tb.id}
            onClick={() => setTab(tb.id)}
            className="flex items-center gap-2 px-3 py-1.5 rounded-md text-xs font-medium transition-all"
            style={{
              background: tab === tb.id ? "var(--accent-subtle)" : "transparent",
              color: tab === tb.id ? "var(--accent)" : "var(--text-2)",
              border: "none",
              cursor: "pointer",
            }}
          >
            <tb.icon size={14} />
            {t(tb.i18nKey)}
            {tb.id === "hotspots" && hotspots && (
              <span
                className="px-1.5 rounded-full text-[10px]"
                style={{ background: "var(--bg-3)", color: "var(--text-3)" }}
              >
                {hotspots.length}
              </span>
            )}
            {tb.id === "coupling" && coupling && (
              <span
                className="px-1.5 rounded-full text-[10px]"
                style={{ background: "var(--bg-3)", color: "var(--text-3)" }}
              >
                {coupling.length}
              </span>
            )}
          </button>
        ))}
      </div>

      {/* Content */}
      <div className="flex-1 overflow-auto">
        {tab === "hotspots" && (
          <HotspotsView data={hotspots || []} loading={loadingH} />
        )}
        {tab === "coupling" && (
          <CouplingView data={coupling || []} loading={loadingC} />
        )}
        {tab === "ownership" && (
          <OwnershipView data={ownership || []} loading={loadingO} />
        )}
      </div>
    </AnimatedPage>
  );
}
