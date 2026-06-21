import { memo } from "react";
import { RepoDashboard } from "../repos/RepoDashboard";
import { ActivityTimeline } from "./ActivityTimeline";

export const OverviewView = memo(function OverviewView() {
  return (
    <div className="h-full overflow-auto">
      <div style={{ padding: "16px 24px 0" }}>
        <ActivityTimeline />
      </div>
      <RepoDashboard />
    </div>
  );
});
