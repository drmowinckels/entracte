import {
  HOTKEY_ACTIONS,
  acceleratorFor,
  conflictingAccelerators,
  isValidAccelerator,
  normalizeAccelerator,
  setAccelerator,
} from "../../../lib/hotkeys";
import type { UseSettings } from "../hooks/use-settings";
import type { SchedulerSettings } from "../types";
import { CheckboxRow } from "./rows";
import { InfoTip } from "./info-tip";

// Global-hotkey configuration. Each action gets a free-text accelerator
// field (tauri-plugin-global-shortcut syntax, e.g. "CmdOrCtrl+Alt+P");
// clearing it unbinds the action. Conflicts (one chord on two actions) are
// flagged inline. Registration happens on the backend, so the shortcuts fire
// whether or not this window is focused.
export function HotkeysSection({
  settings,
  update,
}: {
  settings: SchedulerSettings;
  update: UseSettings["update"];
}) {
  const conflicts = conflictingAccelerators(settings.hotkeys);

  return (
    <>
      <CheckboxRow
        label="Enable global hotkeys"
        value={settings.hotkeys_enabled}
        onChange={(v) => update("hotkeys_enabled", v)}
        tip="Register OS-level keyboard shortcuts for the actions below. They fire whether or not this window is focused. Format example: CmdOrCtrl+Alt+P."
      />
      {settings.hotkeys_enabled && (
        <div className="hotkeys-list">
          {HOTKEY_ACTIONS.map(({ action, label }) => {
            const accelerator = acceleratorFor(settings.hotkeys, action);
            const hasValue = accelerator.trim().length > 0;
            const invalid = hasValue && !isValidAccelerator(accelerator);
            const conflicting =
              hasValue && conflicts.has(normalizeAccelerator(accelerator));
            // Invalid syntax is the more fundamental problem, so it wins the
            // message when both would apply.
            const problemText = invalid
              ? "Not a recognised shortcut. Use one or more modifiers (CmdOrCtrl, Alt, Shift) plus a single key, e.g. CmdOrCtrl+Alt+P."
              : "This shortcut is also bound to another action. Give each action a unique chord.";
            const problem = invalid || conflicting;
            return (
              // A plain div, not a <label>: the row holds two controls (the
              // accelerator field and Clear), so a wrapping label would
              // ambiguously associate with both. The input carries its own
              // aria-label instead.
              <div className="row" key={action}>
                <span>
                  {label}
                  {problem && <InfoTip text={problemText} warn />}
                </span>
                <span className="hotkey-input">
                  <input
                    type="text"
                    aria-label={label}
                    spellCheck={false}
                    placeholder="e.g. CmdOrCtrl+Alt+P"
                    value={accelerator}
                    aria-invalid={problem || undefined}
                    onChange={(e) =>
                      update(
                        "hotkeys",
                        setAccelerator(
                          settings.hotkeys,
                          action,
                          e.target.value,
                        ),
                      )
                    }
                  />
                  <button
                    type="button"
                    className="secondary"
                    aria-label={`Clear ${label} shortcut`}
                    disabled={accelerator.length === 0}
                    onClick={() =>
                      update(
                        "hotkeys",
                        setAccelerator(settings.hotkeys, action, ""),
                      )
                    }
                  >
                    Clear
                  </button>
                </span>
              </div>
            );
          })}
        </div>
      )}
    </>
  );
}
