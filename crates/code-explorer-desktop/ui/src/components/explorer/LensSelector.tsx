import { memo } from "react";
import { Eye } from "lucide-react";
import { useAppStore } from "../../stores/app-store";
import type { LensType } from "../../stores/app-store";
import { useI18n } from "../../hooks/use-i18n";

const LENS_OPTIONS: { value: LensType; i18nKey: string }[] = [
  { value: "all", i18nKey: "lens.all" },
  { value: "calls", i18nKey: "lens.calls" },
  { value: "structure", i18nKey: "lens.structure" },
  { value: "heritage", i18nKey: "lens.heritage" },
  { value: "impact", i18nKey: "lens.impact" },
  { value: "dead-code", i18nKey: "lens.deadCode" },
  { value: "tracing", i18nKey: "lens.tracing" },
  { value: "hotspots", i18nKey: "lens.hotspots" },
  { value: "risk", i18nKey: "lens.risk" },
];

export const LensSelector = memo(function LensSelector() {
  const { t } = useI18n();
  const activeLens = useAppStore((s) => s.activeLens);
  const setActiveLens = useAppStore((s) => s.setActiveLens);

  return (
    <div className="flex items-center gap-1.5">
      <Eye size={14} style={{ color: "var(--text-3)" }} />
      <select
        value={activeLens}
        onChange={(e) => {
          const lens = e.target.value as LensType;
          setActiveLens(lens);
        }}
        aria-label={t("lens.ariaLabel")}
        className="text-xs rounded px-1.5 py-1 border-none cursor-pointer"
        style={{
          background: "var(--bg-3)",
          color: "var(--text-1)",
          fontFamily: "var(--font-body)",
        }}
      >
        {LENS_OPTIONS.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {t(opt.i18nKey)}
          </option>
        ))}
      </select>
    </div>
  );
});
