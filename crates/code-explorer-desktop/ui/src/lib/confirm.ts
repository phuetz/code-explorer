/**
 * Stylized confirm dialog — drop-in replacement for `window.confirm()`.
 *
 * Usage:
 *   import { confirm } from "../../lib/confirm";
 *   if (await confirm({ title: "...", message: "...", danger: true })) {
 *     // user confirmed
 *   }
 *
 * The actual rendering is done by <ConfirmDialogProvider /> mounted at the
 * root of App.tsx. This module only exposes the imperative `confirm()`
 * helper and the store the provider reads from.
 */

import { create } from "zustand";

export type ConfirmOptions = {
  title: string;
  message: string;
  /** Defaults to "Confirm" / "Confirmer" depending on locale. */
  confirmLabel?: string;
  /** Defaults to "Cancel" / "Annuler". */
  cancelLabel?: string;
  /** When true, styles the confirm button in the danger color (red). */
  danger?: boolean;
};

type PendingRequest = ConfirmOptions & { resolve: (ok: boolean) => void };

interface ConfirmStore {
  pending: PendingRequest | null;
  request: (opts: ConfirmOptions) => Promise<boolean>;
  resolve: (ok: boolean) => void;
}

export const useConfirmStore = create<ConfirmStore>((set, get) => ({
  pending: null,
  request: (opts) =>
    new Promise<boolean>((resolve) => {
      const current = get().pending;
      // If a previous confirm is still open, auto-cancel it. Stacking two
      // modals on top of each other is confusing and we don't want to keep
      // promises dangling forever.
      if (current) current.resolve(false);
      set({ pending: { ...opts, resolve } });
    }),
  resolve: (ok) => {
    const current = get().pending;
    if (!current) return;
    current.resolve(ok);
    set({ pending: null });
  },
}));

/** Imperative API — `await confirm({ title, message })`. */
export function confirm(opts: ConfirmOptions): Promise<boolean> {
  return useConfirmStore.getState().request(opts);
}
