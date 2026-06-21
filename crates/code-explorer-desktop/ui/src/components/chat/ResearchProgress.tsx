import { Cpu, CheckCircle2, Loader2 } from "lucide-react";

export type ResearchStep = {
  id: string;
  tool: string;
  status: "pending" | "running" | "completed" | "failed";
  label: string;
};

export function ResearchProgress({ steps }: { steps: ResearchStep[] }) {
  if (steps.length === 0) return null;

  return (
    <div className="flex flex-col gap-2 p-3 bg-bg-2 rounded-xl border border-surface-border mb-4 fade-in">
      <div className="flex items-center gap-2 mb-1 px-1">
        <Cpu size={14} className="text-purple" />
        <span className="text-[11px] font-semibold text-text-1 uppercase tracking-wider">Research Pipeline</span>
      </div>
      <div className="flex flex-col gap-1.5">
        {steps.map((step, i) => (
          <div key={step.id} className="flex items-center gap-3 group">
            <div className="flex flex-col items-center">
              <div 
                className={`w-5 h-5 rounded-full flex items-center justify-center transition-colors ${
                  step.status === "completed" ? "bg-green/20 text-green" :
                  step.status === "running" ? "bg-purple/20 text-purple animate-pulse" :
                  "bg-bg-3 text-text-4"
                }`}
              >
                {step.status === "completed" ? <CheckCircle2 size={12} /> :
                 step.status === "running" ? <Loader2 size={12} className="animate-spin" /> :
                 <div className="w-1.5 h-1.5 rounded-full bg-current" />}
              </div>
              {i < steps.length - 1 && (
                <div className="w-px h-3 bg-surface-border my-0.5" />
              )}
            </div>
            <div className="flex-1 min-w-0">
              <div className={`text-[12px] font-medium truncate ${
                step.status === "running" ? "text-text-0" : "text-text-3"
              }`}>
                {step.label}
              </div>
              {step.status === "running" && (
                <div className="text-[10px] text-purple/70">Executing {step.tool}...</div>
              )}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
