import { useCallback, useEffect, useRef } from "react";
import {
  TRAY_COUNTDOWN_TARGETS,
  type TrayCountdownTarget,
} from "../../../lib/tray-countdown";
import { Advanced } from "../components/advanced";
import { CheckboxRow, NumberRow } from "../components/rows";
import { HOOK_EVENTS } from "../constants";
import type { UseHooks } from "../hooks/use-hooks";
import type { UseSettings } from "../hooks/use-settings";
import type {
  ClockFormat,
  HookConfig,
  HookEvent,
  SchedulerSettings,
} from "../types";

function newUiId(): string {
  if (
    typeof crypto !== "undefined" &&
    typeof crypto.randomUUID === "function"
  ) {
    return crypto.randomUUID();
  }
  return `hook-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

export function SystemTab({
  settings,
  update,
  setAutostart,
  hooks,
}: {
  settings: SchedulerSettings;
  update: UseSettings["update"];
  setAutostart: UseSettings["setAutostart"];
  hooks: UseHooks;
}) {
  // Stable IDs for hook rows so React keys survive reordering / mid-list edits.
  // The IDs are local UI state only; they never leave the component.
  const idsRef = useRef<string[]>([]);
  if (idsRef.current.length !== hooks.draft.length) {
    if (idsRef.current.length < hooks.draft.length) {
      const need = hooks.draft.length - idsRef.current.length;
      for (let i = 0; i < need; i += 1) idsRef.current.push(newUiId());
    } else {
      idsRef.current = idsRef.current.slice(0, hooks.draft.length);
    }
  }

  useEffect(() => {
    if (idsRef.current.length !== hooks.draft.length) {
      idsRef.current = hooks.draft.map(() => newUiId());
    }
  }, [hooks.draft]);

  const updateHookAt = useCallback(
    (idx: number, patch: Partial<HookConfig>) => {
      const next = [...hooks.draft];
      next[idx] = { ...next[idx], ...patch };
      hooks.setDraft(next);
    },
    [hooks],
  );

  const removeHookAt = useCallback(
    (idx: number) => {
      idsRef.current = idsRef.current.filter((_, i) => i !== idx);
      hooks.setDraft(hooks.draft.filter((_, i) => i !== idx));
    },
    [hooks],
  );

  const addHook = useCallback(() => {
    idsRef.current = [...idsRef.current, newUiId()];
    hooks.setDraft([
      ...hooks.draft,
      { event: "break_start", command: "", enabled: true },
    ]);
  }, [hooks]);

  return (
    <>
      <h2>Startup</h2>
      <section>
        <CheckboxRow
          label="Start Entracte at login"
          value={settings.autostart_enabled}
          onChange={(v) => setAutostart(v)}
        />
      </section>

      <h2>Display</h2>
      <section>
        <label className="row">
          <span>Time format</span>
          <select
            value={settings.clock_format}
            onChange={(e) =>
              update("clock_format", e.target.value as ClockFormat)
            }
          >
            <option value="24h">24-hour (14:30)</option>
            <option value="12h">12-hour (2:30 PM)</option>
          </select>
        </label>
      </section>

      <h2>Notifications</h2>
      <section>
        <CheckboxRow
          label="Notify before a break starts"
          value={settings.prebreak_notification_enabled}
          onChange={(v) => update("prebreak_notification_enabled", v)}
          tip="Posts a heads-up system notification N seconds before the overlay appears, so a break doesn't catch you mid-thought."
        />
        <NumberRow
          label="Lead time (seconds)"
          value={settings.prebreak_notification_seconds}
          min={5}
          multiplier={1}
          onChange={(v) => update("prebreak_notification_seconds", v)}
        />
      </section>

      <h2>Tray countdown</h2>
      <section>
        <CheckboxRow
          label="Show countdown to next break in the tray"
          value={settings.tray_countdown_enabled}
          onChange={(v) => update("tray_countdown_enabled", v)}
          onlyOn={["macos", "linux"]}
          tip="Shows a live mm:ss next to the tray icon. macOS and Linux only — Windows doesn't support tray text."
        />
        <label
          className={`row${settings.tray_countdown_enabled ? "" : " disabled"}`}
        >
          <span>Count down to</span>
          <select
            value={settings.tray_countdown_target}
            disabled={!settings.tray_countdown_enabled}
            onChange={(e) =>
              update(
                "tray_countdown_target",
                e.target.value as TrayCountdownTarget,
              )
            }
          >
            {TRAY_COUNTDOWN_TARGETS.map((t) => (
              <option key={t.id} value={t.id}>
                {t.label}
              </option>
            ))}
          </select>
        </label>
      </section>

      <Advanced label="Show advanced (hooks)">
        <h3>Event hooks</h3>
        <p className="placeholder">
          Run shell commands when break events fire. Off by default — only
          enable if you understand the security risk of letting arbitrary
          commands run. Changes only apply after you click{" "}
          <strong>Save hooks</strong> and confirm in the system dialog.
        </p>
        <label className="row">
          <span>Run shell commands on break events</span>
          <input
            type="checkbox"
            checked={hooks.draftEnabled}
            onChange={(e) => hooks.setDraftEnabled(e.target.checked)}
          />
        </label>
        {hooks.draftEnabled && (
          <div className="hooks-list">
            {hooks.draft.map((hook, idx) => (
              <div className="hook-row" key={idsRef.current[idx]}>
                <select
                  aria-label="Hook event"
                  value={hook.event}
                  onChange={(e) =>
                    updateHookAt(idx, { event: e.target.value as HookEvent })
                  }
                >
                  {HOOK_EVENTS.map((opt) => (
                    <option key={opt.id} value={opt.id}>
                      {opt.label}
                    </option>
                  ))}
                </select>
                <input
                  type="text"
                  aria-label="Hook command"
                  className="hook-command"
                  placeholder={`e.g. sh -c "osascript -e 'tell app \\"Spotify\\" to pause'"`}
                  value={hook.command}
                  onChange={(e) =>
                    updateHookAt(idx, { command: e.target.value })
                  }
                />
                <label className="hook-toggle">
                  <input
                    type="checkbox"
                    checked={hook.enabled}
                    onChange={(e) =>
                      updateHookAt(idx, { enabled: e.target.checked })
                    }
                  />
                  <span>On</span>
                </label>
                <button
                  className="secondary hook-remove"
                  onClick={() => removeHookAt(idx)}
                >
                  Remove
                </button>
              </div>
            ))}
            <div className="actions inline">
              <button className="secondary" onClick={addHook}>
                Add hook
              </button>
            </div>
          </div>
        )}
        <div className="actions inline">
          <button
            className="primary"
            disabled={hooks.saving || !hooks.isDirty(settings)}
            onClick={hooks.save}
          >
            {hooks.saving ? "Waiting for confirmation…" : "Save hooks"}
          </button>
          <button
            className="secondary"
            disabled={hooks.saving}
            onClick={() => hooks.reset(settings)}
          >
            Reset
          </button>
        </div>
        {hooks.error && <p className="placeholder">{hooks.error}</p>}
      </Advanced>
    </>
  );
}
