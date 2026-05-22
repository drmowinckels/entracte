import { useCallback, useEffect, useRef, useState } from "react";
import { z } from "zod";
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import {
  isEnabled as autostartIsEnabled,
  enable as autostartEnable,
  disable as autostartDisable,
} from "@tauri-apps/plugin-autostart";
import { invoke } from "../../../lib/ipc";
import { useTauriListen } from "../../../lib/use-tauri-listen";
import type { SchedulerSettings } from "../types";

const breakSoundSchema = z.object({
  mode: z.enum(["off", "end_chime", "ambient"]),
  sound_id: z.string(),
  custom_path: z.string().optional(),
});

const hookConfigSchema = z.object({
  event: z.enum([
    "break_start",
    "break_end",
    "break_postponed",
    "break_skipped",
    "pause_start",
    "pause_end",
  ]),
  command: z.string(),
  enabled: z.boolean(),
});

export const schedulerSettingsSchema = z.object({
  micro_interval_secs: z.number(),
  micro_duration_secs: z.number(),
  long_interval_secs: z.number(),
  long_duration_secs: z.number(),
  micro_idle_reset_secs: z.number(),
  long_idle_reset_secs: z.number(),
  micro_enabled: z.boolean(),
  long_enabled: z.boolean(),
  micro_enforceable: z.boolean(),
  long_enforceable: z.boolean(),
  pause_during_dnd: z.boolean(),
  pause_during_camera: z.boolean(),
  pause_during_video: z.boolean(),
  work_window_enabled: z.boolean(),
  work_start_minutes: z.number(),
  work_end_minutes: z.number(),
  bedtime_enabled: z.boolean(),
  bedtime_start_minutes: z.number(),
  bedtime_end_minutes: z.number(),
  bedtime_interval_secs: z.number(),
  bedtime_duration_secs: z.number(),
  prebreak_notification_enabled: z.boolean(),
  prebreak_notification_seconds: z.number(),
  overlay_opacity: z.number(),
  overlay_color: z.string(),
  overlay_custom_rgb: z.string(),
  overlay_high_contrast: z.boolean(),
  show_hint: z.boolean(),
  monitor_placement: z.enum(["primary", "active", "all"]),
  strict_mode: z.boolean(),
  postpone_enabled: z.boolean(),
  postpone_minutes: z.number(),
  show_current_time: z.boolean(),
  clock_format: z.enum(["12h", "24h"]),
  micro_manual_finish: z.boolean(),
  long_manual_finish: z.boolean(),
  autostart_enabled: z.boolean(),
  micro_sound: breakSoundSchema,
  long_sound: breakSoundSchema,
  sound_volume: z.number(),
  app_pause_enabled: z.boolean(),
  app_pause_list: z.array(z.string()),
  break_health_enabled: z.boolean(),
  micro_physical_hints: z.array(z.string()),
  micro_psychological_hints: z.array(z.string()),
  micro_hint_mix: z.enum(["both", "physical", "psychological"]),
  long_hints: z.array(z.string()),
  long_social_hints: z.array(z.string()),
  long_hint_mix: z.enum(["both", "solo", "social"]),
  sleep_hints: z.array(z.string()),
  hint_rotate_seconds: z.number(),
  delay_break_if_typing: z.boolean(),
  typing_grace_secs: z.number(),
  typing_max_deferral_secs: z.number(),
  pause_countdown_if_typing: z.boolean(),
  postpone_escalation_enabled: z.boolean(),
  postpone_escalation_step_secs: z.number(),
  postpone_max_count: z.number(),
  overlay_font_scale: z.number(),
  micro_fixed_times: z.array(z.string()),
  long_fixed_times: z.array(z.string()),
  micro_schedule_mode: z.enum(["interval", "fixed", "both"]),
  long_schedule_mode: z.enum(["interval", "fixed", "both"]),
  hooks_enabled: z.boolean(),
  hooks: z.array(hookConfigSchema),
  daily_screen_time_enabled: z.boolean(),
  daily_screen_time_budget_minutes: z.number(),
  daily_screen_time_remind_again_minutes: z.number(),
  tray_countdown_enabled: z.boolean(),
  tray_countdown_target: z.enum(["next", "short", "long"]),
  micro_break_mode: z.enum(["overlay", "windowed", "notification"]),
  long_break_mode: z.enum(["overlay", "windowed", "notification"]),
  custom_css: z.string(),
}) satisfies z.ZodType<SchedulerSettings>;

const PERSIST_DEBOUNCE_MS = 250;

type Updater = (next: SchedulerSettings) => void;

// Persisting hits the disk: `update_settings` rewrites the full profiles
// JSON via an atomic tmpfile + rename. Without debouncing, dragging a slider
// or typing in a number input fires one write per event.
function debouncedPersist(): Updater {
  let pending: number | null = null;
  let latest: SchedulerSettings | null = null;
  const flush = () => {
    pending = null;
    if (latest) {
      const snapshot = latest;
      latest = null;
      tauriInvoke("update_settings", { new: snapshot }).catch((e) =>
        console.error("update_settings failed", e),
      );
    }
  };
  return (next) => {
    latest = next;
    if (pending !== null) window.clearTimeout(pending);
    pending = window.setTimeout(flush, PERSIST_DEBOUNCE_MS);
  };
}

/** Shape returned by {@link useSettings}: live `Settings` plus the
 * helpers used across every Settings tab. */
export type UseSettings = {
  settings: SchedulerSettings | null;
  update: <K extends keyof SchedulerSettings>(
    key: K,
    value: SchedulerSettings[K],
  ) => void;
  updateMany: (patch: Partial<SchedulerSettings>) => void;
  reloadFromActive: () => Promise<SchedulerSettings | null>;
  setAutostart: (enabled: boolean) => Promise<void>;
};

/** Single source of truth for the active profile's settings inside the
 * renderer. Loads on mount, syncs autostart state, debounces every
 * mutation, and subscribes to `profile:changed` so a tray-driven
 * profile switch refreshes the form. */
export function useSettings(): UseSettings {
  const [settings, setSettings] = useState<SchedulerSettings | null>(null);
  const persistRef = useRef<Updater>(debouncedPersist());

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const s = await invoke(
          "get_settings",
          undefined,
          schedulerSettingsSchema,
        );
        let next = s;
        try {
          const actuallyEnabled = await autostartIsEnabled();
          if (actuallyEnabled !== s.autostart_enabled) {
            next = { ...s, autostart_enabled: actuallyEnabled };
            tauriInvoke("update_settings", { new: next }).catch((e) =>
              console.error("autostart sync failed", e),
            );
          }
        } catch {
          // autostart plugin unavailable — keep settings as-is
        }
        if (!cancelled) setSettings(next);
      } catch (e) {
        console.error("get_settings failed", e);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const reloadFromActive = useCallback(async () => {
    try {
      const s = await invoke(
        "get_settings",
        undefined,
        schedulerSettingsSchema,
      );
      setSettings(s);
      return s;
    } catch (e) {
      console.error("reloadFromActive failed", e);
      return null;
    }
  }, []);

  const update = useCallback(
    <K extends keyof SchedulerSettings>(key: K, value: SchedulerSettings[K]) => {
      setSettings((prev) => {
        if (!prev) return prev;
        const next = { ...prev, [key]: value };
        persistRef.current(next);
        return next;
      });
    },
    [],
  );

  const updateMany = useCallback((patch: Partial<SchedulerSettings>) => {
    setSettings((prev) => {
      if (!prev) return prev;
      const next = { ...prev, ...patch };
      persistRef.current(next);
      return next;
    });
  }, []);

  const setAutostart = useCallback(
    async (enabled: boolean) => {
      try {
        if (enabled) {
          await autostartEnable();
        } else {
          await autostartDisable();
        }
        update("autostart_enabled", enabled);
      } catch (e) {
        console.error("autostart toggle failed", e);
      }
    },
    [update],
  );

  // Listen for external profile switches that change the active settings.
  useTauriListen(
    "profile:changed",
    () => {
      reloadFromActive();
    },
    [reloadFromActive],
  );

  return { settings, update, updateMany, reloadFromActive, setAutostart };
}
