import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { HookConfig, SchedulerSettings } from "../types";

/** Draft state + actions for the hooks editor. `draft` is the local
 * edit buffer; it diverges from the persisted hooks until `save` lands. */
export type UseHooks = {
  draft: HookConfig[];
  draftEnabled: boolean;
  saving: boolean;
  error: string;
  setDraft: (next: HookConfig[]) => void;
  setDraftEnabled: (enabled: boolean) => void;
  syncFromSettings: (s: SchedulerSettings) => void;
  isDirty: (s: SchedulerSettings) => boolean;
  save: () => Promise<void>;
  reset: (s: SchedulerSettings) => void;
};

function hooksEqual(a: HookConfig[], b: HookConfig[]): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i += 1) {
    if (a[i].event !== b[i].event) return false;
    if (a[i].command !== b[i].command) return false;
    if (a[i].enabled !== b[i].enabled) return false;
  }
  return true;
}

/** Buffer hook edits locally and flush them via `set_hooks` (which
 * shows the user-confirmation dialog) on `save`. Seeds from `settings`
 * on mount and re-seeds whenever the active profile changes. */
export function useHooks(
  settings: SchedulerSettings | null,
  reloadSettings: () => Promise<SchedulerSettings | null>,
): UseHooks {
  const [draft, setDraft] = useState<HookConfig[]>([]);
  const [draftEnabled, setDraftEnabled] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  const syncFromSettings = useCallback((s: SchedulerSettings) => {
    setDraft(s.hooks.map((h) => ({ ...h })));
    setDraftEnabled(s.hooks_enabled);
    setError("");
  }, []);

  // Seed the draft the first time settings arrive — and whenever the active
  // profile changes (the listener in useSettings replaces `settings`).
  useEffect(() => {
    if (settings) syncFromSettings(settings);
  }, [settings, syncFromSettings]);

  const isDirty = useCallback(
    (s: SchedulerSettings) =>
      draftEnabled !== s.hooks_enabled || !hooksEqual(draft, s.hooks),
    [draft, draftEnabled],
  );

  const save = useCallback(async () => {
    setSaving(true);
    setError("");
    try {
      await invoke("set_hooks", {
        hooksEnabled: draftEnabled,
        hooks: draft,
      });
      await reloadSettings();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }, [draft, draftEnabled, reloadSettings]);

  const reset = useCallback(
    (s: SchedulerSettings) => {
      syncFromSettings(s);
    },
    [syncFromSettings],
  );

  return {
    draft,
    draftEnabled,
    saving,
    error,
    setDraft,
    setDraftEnabled,
    syncFromSettings,
    isDirty,
    save,
    reset,
  };
}
