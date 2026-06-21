import { useSyncExternalStore } from "react";
import { getLocale, setLocale, subscribe, t, tt, type Locale } from "../lib/i18n";

/** Hook that re-renders when the locale changes. Returns translation helpers + locale controls. */
export function useI18n() {
  const locale = useSyncExternalStore(subscribe, getLocale, getLocale);
  return { locale, setLocale, t, tt } as const;
}

export type { Locale };
