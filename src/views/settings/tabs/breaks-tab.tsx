import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  clampCsvToDark,
  hexToRgbCsv,
  normalizeHexInput,
  rgbCsvToHex,
} from "../../../lib/color";
import { useLocalDraft } from "../../../lib/use-local-draft";
import { BREAK_MODE_OPTIONS, type BreakMode } from "../../../lib/break-mode";
import { Advanced } from "../components/advanced";
import { CheckboxRow, NumberRow } from "../components/rows";
import { InfoTip } from "../components/info-tip";
import { WindowedSizeRow } from "../components/windowed-size-row";
import { RoutinePicker } from "../components/routine-picker";
import { SoundControls } from "../components/sound-controls";
import { ContentPacks } from "../components/content-packs";
import {
  MONITOR_PLACEMENTS,
  OVERLAY_THEMES,
  ROTATION_GRADIENT,
} from "../constants";
import type { UseSettings } from "../hooks/use-settings";
import { useRoutines } from "../hooks/use-routines";
import { useChores } from "../hooks/use-chores";
import type {
  MonitorPlacement,
  SchedulerSettings,
  SupporterStatus,
} from "../types";
import { linesToList, listToLines } from "../utils";

// Persist the chore draft this long after the last keystroke (#225). Short
// enough that a list jotted at the morning prompt is cached well before the
// laptop sleeps or shuts down, long enough not to fire a `set_chores` on every
// keystroke.
const CHORES_AUTOSAVE_DELAY_MS = 800;

export function BreaksTab({
  settings,
  update,
  supporter,
  reload,
  focusChoresNonce = 0,
}: {
  settings: SchedulerSettings;
  update: UseSettings["update"];
  supporter: SupporterStatus;
  reload: () => Promise<unknown>;
  /// Bumped by the shell when the morning chore prompt fires, to pull focus
  /// to the chores input. `0` is the initial value and never focuses.
  focusChoresNonce?: number;
}) {
  const isSupporter = supporter.is_supporter;
  const { routines, reload: reloadRoutines } = useRoutines();
  const { chores, save: saveChores } = useChores();
  const [choreLines, setChoreLines] = useLocalDraft(
    () => listToLines(chores?.items ?? []),
    [chores?.items],
  );
  const choresRef = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    if (focusChoresNonce > 0) {
      choresRef.current?.scrollIntoView?.({ block: "center" });
      choresRef.current?.focus();
    }
  }, [focusChoresNonce]);
  // Cache chores as they're typed, not only on blur (#225). The morning prompt
  // focuses this textarea; a user who jots chores then closes the window or
  // sleeps the laptop without clicking away would otherwise lose them — and
  // since the morning prompt already persisted today's `prompted_date`, they'd
  // get no re-prompt the next day either. Persist a short beat after typing
  // stops, gated on a real change so the initial load and a re-seed from the
  // saved (sanitized) list never trigger a redundant save.
  useEffect(() => {
    if (!chores) return;
    const current = linesToList(choreLines);
    const saved = chores.items;
    const unchanged =
      current.length === saved.length &&
      current.every((item, i) => item === saved[i]);
    if (unchanged) return;
    const timer = setTimeout(() => {
      void saveChores(current);
    }, CHORES_AUTOSAVE_DELAY_MS);
    return () => clearTimeout(timer);
  }, [choreLines, chores, saveChores]);
  // Local drafts re-seed when the active profile swaps the setting out.
  const [microPhysical, setMicroPhysical] = useLocalDraft(
    () => listToLines(settings.micro_physical_hints),
    [settings.micro_physical_hints],
  );
  const [microPsychological, setMicroPsychological] = useLocalDraft(
    () => listToLines(settings.micro_psychological_hints),
    [settings.micro_psychological_hints],
  );
  const [longSolo, setLongSolo] = useLocalDraft(
    () => listToLines(settings.long_hints),
    [settings.long_hints],
  );
  const [longSocial, setLongSocial] = useLocalDraft(
    () => listToLines(settings.long_social_hints),
    [settings.long_social_hints],
  );
  const [sleep, setSleep] = useLocalDraft(
    () => listToLines(settings.sleep_hints),
    [settings.sleep_hints],
  );
  const [customCss, setCustomCss] = useLocalDraft(
    () => settings.custom_css,
    [settings.custom_css],
  );

  const transparencyPct = Math.round((1 - settings.overlay_opacity) * 100);
  const fontScalePct = Math.round(settings.overlay_font_scale * 100);
  const soundVolumePct = Math.round(settings.sound_volume * 100);

  const routinePicker = (kind: "micro" | "long") => (
    <RoutinePicker
      kind={kind}
      routineKey={`${kind}_routine`}
      categoriesKey={`${kind}_routine_categories`}
      difficultyKey={`${kind}_routine_max_difficulty`}
      settings={settings}
      update={update}
      routines={routines}
    />
  );

  return (
    <>
      <h2 id="settings-delivery">Delivery</h2>
      <section>
        <p className="placeholder">
          How each break appears. Turn a break on or off, and set its cadence,
          on the Schedule tab.
          <InfoTip text="Full-screen overlay covers the monitor. Windowed shows the same prompt sized to a fraction of the screen, leaving the desktop reachable. System notification only posts a notification and records no skip/postpone metrics." />
        </p>
        <label className={`row${settings.micro_enabled ? "" : " disabled"}`}>
          <span>Micro breaks</span>
          <select
            value={settings.micro_break_mode}
            disabled={!settings.micro_enabled}
            onChange={(e) =>
              update("micro_break_mode", e.target.value as BreakMode)
            }
          >
            {BREAK_MODE_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
        </label>
        <label className={`row${settings.long_enabled ? "" : " disabled"}`}>
          <span>Long breaks</span>
          <select
            value={settings.long_break_mode}
            disabled={!settings.long_enabled}
            onChange={(e) =>
              update("long_break_mode", e.target.value as BreakMode)
            }
          >
            {BREAK_MODE_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
        </label>
        <div className="actions inline">
          <button
            className="secondary"
            disabled={!settings.micro_enabled}
            onClick={() =>
              invoke("trigger_test_break", { kind: "micro", durationSecs: 10 })
            }
          >
            Test micro
          </button>
          <button
            className="secondary"
            disabled={!settings.long_enabled}
            onClick={() =>
              invoke("trigger_test_break", { kind: "long", durationSecs: 15 })
            }
          >
            Test long
          </button>
        </div>
      </section>

      <h2 id="settings-overlay">Overlay</h2>
      <section>
        <label className="row">
          <span>
            Transparency
            <InfoTip text="0% is fully opaque. Higher values let your work show through faintly — useful as a softer prompt." />
          </span>
          <span className="range-wrap">
            <input
              type="range"
              min={0}
              max={20}
              step={1}
              value={transparencyPct}
              onChange={(e) =>
                update("overlay_opacity", 1 - Number(e.target.value) / 100)
              }
            />
            <span className="range-value">{transparencyPct}%</span>
          </span>
        </label>
        <label className="row">
          <span>Text size</span>
          <span className="range-wrap">
            <input
              type="range"
              min={80}
              max={160}
              step={5}
              value={fontScalePct}
              onChange={(e) =>
                update("overlay_font_scale", Number(e.target.value) / 100)
              }
            />
            <span className="range-value">{fontScalePct}%</span>
          </span>
        </label>
        <label className="row">
          <span>
            Theme
            <InfoTip text="Pick a preset, Rotate (different preset every break), or Custom for any colour. Custom colours are auto-darkened so the overlay still dims the screen." />
          </span>
          <span className="theme-wrap">
            <span
              className="theme-swatch"
              style={
                settings.overlay_color === "rotate"
                  ? { background: ROTATION_GRADIENT }
                  : {
                      background: `rgb(${
                        settings.overlay_color === "custom"
                          ? settings.overlay_custom_rgb
                          : (OVERLAY_THEMES.find(
                              (t) => t.id === settings.overlay_color,
                            )?.rgb ?? OVERLAY_THEMES[0].rgb)
                      })`,
                    }
              }
            />
            <select
              value={settings.overlay_color}
              onChange={(e) => update("overlay_color", e.target.value)}
            >
              {OVERLAY_THEMES.map((t) => {
                const supporterOnly = t.id === "rotate" || t.id === "custom";
                if (
                  supporterOnly &&
                  !isSupporter &&
                  settings.overlay_color !== t.id
                ) {
                  return null;
                }
                return (
                  <option key={t.id} value={t.id}>
                    {t.label}
                  </option>
                );
              })}
            </select>
          </span>
        </label>
        {settings.overlay_color === "custom" && (
          <label className="row">
            <span>Custom color</span>
            <span className="color-wrap">
              <input
                type="color"
                value={rgbCsvToHex(settings.overlay_custom_rgb)}
                onChange={(e) => {
                  const csv = hexToRgbCsv(e.target.value);
                  if (!csv) return;
                  update("overlay_custom_rgb", clampCsvToDark(csv) ?? csv);
                }}
              />
              <input
                type="text"
                className="color-hex"
                spellCheck={false}
                defaultValue={rgbCsvToHex(settings.overlay_custom_rgb)}
                key={settings.overlay_custom_rgb}
                placeholder="#1f293a"
                onBlur={(e) => {
                  const normalized = normalizeHexInput(e.target.value);
                  if (!normalized) {
                    e.target.value = rgbCsvToHex(settings.overlay_custom_rgb);
                    return;
                  }
                  const csv = hexToRgbCsv(normalized);
                  if (!csv) return;
                  update("overlay_custom_rgb", clampCsvToDark(csv) ?? csv);
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter") (e.target as HTMLInputElement).blur();
                }}
              />
            </span>
          </label>
        )}
        <CheckboxRow
          label="Show wellness hints"
          value={settings.show_hint}
          onChange={(v) => update("show_hint", v)}
        />
        {settings.show_hint && (
          <>
            <label className="row checkbox-row">
              <span>
                Rotate hints during the break
                <InfoTip text="Off: one idea is picked per break and stays on screen. On: the overlay cycles through the remaining ideas in the pool every N seconds." />
              </span>
              <input
                type="checkbox"
                checked={settings.hint_rotate_seconds > 0}
                onChange={(e) =>
                  update("hint_rotate_seconds", e.target.checked ? 12 : 0)
                }
              />
            </label>
            {settings.hint_rotate_seconds > 0 && (
              <NumberRow
                label="Rotate every (seconds)"
                value={settings.hint_rotate_seconds}
                min={3}
                multiplier={1}
                onChange={(v) => update("hint_rotate_seconds", v)}
              />
            )}
          </>
        )}
        <CheckboxRow
          label="Show current time on overlay"
          value={settings.show_current_time}
          onChange={(v) => update("show_current_time", v)}
        />
        <Advanced label="Show advanced overlay options">
          <label className="row">
            <span>
              Show break on
              <InfoTip text="Primary: always the main display. Under cursor: wherever your mouse is when the break fires. All: a break covers every monitor." />
            </span>
            <select
              value={settings.monitor_placement}
              onChange={(e) =>
                update("monitor_placement", e.target.value as MonitorPlacement)
              }
            >
              {MONITOR_PLACEMENTS.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.label}
                </option>
              ))}
            </select>
          </label>
          <WindowedSizeRow
            label="Windowed break size"
            tip="How much of the screen a windowed-mode break fills. Only applies to breaks set to Windowed delivery on the Schedule tab; full-screen overlays always cover the whole monitor."
            value={settings.windowed_fraction}
            allowInherit={false}
            fallback={settings.windowed_fraction}
            onChange={(v) => update("windowed_fraction", v ?? 0.8)}
          />
          <WindowedSizeRow
            label="Micro break size"
            tip="Override the windowed size for micro breaks only. A quick micro break can be smaller than a long one."
            value={settings.micro_windowed_fraction}
            allowInherit
            fallback={settings.windowed_fraction}
            onChange={(v) => update("micro_windowed_fraction", v)}
          />
          <WindowedSizeRow
            label="Long break size"
            tip="Override the windowed size for long breaks only. Leave on “Same as global” to follow the windowed break size above."
            value={settings.long_windowed_fraction}
            allowInherit
            fallback={settings.windowed_fraction}
            onChange={(v) => update("long_windowed_fraction", v)}
          />
          <CheckboxRow
            label="High contrast (pure black, white text, solid ring)"
            value={settings.overlay_high_contrast}
            onChange={(v) => update("overlay_high_contrast", v)}
            tip={`Overrides theme colour and transparency until turned off. Your OS-level "Increase contrast" preference auto-applies at break time even with this off.`}
          />
          <CheckboxRow
            label="Show vignette when breaks are skipped"
            value={settings.break_health_enabled}
            onChange={(v) => update("break_health_enabled", v)}
            tip="A subtle dark vignette appears on the overlay when you've been skipping breaks, intensifying with each skip."
          />
        </Advanced>
      </section>

      <h2 id="settings-sound">Sound</h2>
      <section>
        <label className="row">
          <span>Volume</span>
          <span className="range-wrap">
            <input
              type="range"
              min={0}
              max={100}
              step={1}
              value={soundVolumePct}
              onChange={(e) =>
                update("sound_volume", Number(e.target.value) / 100)
              }
            />
            <span className="range-value">{soundVolumePct}%</span>
          </span>
        </label>
        <p className="placeholder">
          The volume applies to every break sound. Choose the track for each
          break type below.
        </p>
        <h3>Micro breaks</h3>
        <SoundControls
          sound={settings.micro_sound}
          volume={settings.sound_volume}
          onChange={(next) => update("micro_sound", next)}
          isSupporter={isSupporter}
        />
        <h3>Long breaks</h3>
        <SoundControls
          sound={settings.long_sound}
          volume={settings.sound_volume}
          onChange={(next) => update("long_sound", next)}
          isSupporter={isSupporter}
        />
      </section>

      <h2 id="settings-skip-postpone">Skip & postpone</h2>
      <section>
        <CheckboxRow
          label="Strict mode (all breaks enforced, no skip or postpone)"
          value={settings.strict_mode}
          onChange={(v) => update("strict_mode", v)}
          tip="Disables every escape hatch on the overlay. The postpone and skip controls below are ignored while strict mode is on."
        />
        <CheckboxRow
          label="Allow postponing a break"
          value={settings.postpone_enabled}
          onChange={(v) => update("postpone_enabled", v)}
          disabled={settings.strict_mode}
          tip="Master switch for postponing. Turn it on, then choose below which break types can be postponed."
        />
        <NumberRow
          label="Postpone by (minutes)"
          value={settings.postpone_minutes}
          min={1}
          multiplier={1}
          disabled={!settings.postpone_enabled || settings.strict_mode}
          onChange={(v) => update("postpone_minutes", v)}
        />
        <CheckboxRow
          label="Escalate each subsequent postpone of the same break"
          value={settings.postpone_escalation_enabled}
          onChange={(v) => update("postpone_escalation_enabled", v)}
          disabled={!settings.postpone_enabled || settings.strict_mode}
          tip="Each postpone of the same break adds extra delay, making repeated postponing progressively less attractive."
        />
        <NumberRow
          label="Extra delay per postpone (seconds)"
          value={settings.postpone_escalation_step_secs}
          min={0}
          multiplier={1}
          disabled={
            !settings.postpone_enabled ||
            settings.strict_mode ||
            !settings.postpone_escalation_enabled
          }
          onChange={(v) => update("postpone_escalation_step_secs", v)}
        />
        <NumberRow
          label="Maximum postpones per break"
          value={settings.postpone_max_count}
          min={1}
          multiplier={1}
          disabled={
            !settings.postpone_enabled ||
            settings.strict_mode ||
            !settings.postpone_escalation_enabled
          }
          onChange={(v) => update("postpone_max_count", v)}
        />

        <h3>Per break type</h3>
        <CheckboxRow
          label="Postpone micro breaks"
          value={settings.micro_postpone_enabled}
          onChange={(v) => update("micro_postpone_enabled", v)}
          disabled={!settings.postpone_enabled || settings.strict_mode}
          tip="Shows a Postpone button on the micro break overlay."
        />
        <CheckboxRow
          label="Postpone long breaks"
          value={settings.long_postpone_enabled}
          onChange={(v) => update("long_postpone_enabled", v)}
          disabled={!settings.postpone_enabled || settings.strict_mode}
          tip="Shows a Postpone button on the long break overlay."
        />
        <CheckboxRow
          label="Skip micro breaks"
          value={settings.micro_skip_enabled}
          onChange={(v) => update("micro_skip_enabled", v)}
          disabled={settings.strict_mode}
          tip="When off, the micro break overlay has no Skip button and Esc won't dismiss it."
        />
        <CheckboxRow
          label="Skip long breaks"
          value={settings.long_skip_enabled}
          onChange={(v) => update("long_skip_enabled", v)}
          disabled={settings.strict_mode}
          tip="When off, the long break overlay has no Skip button and Esc won't dismiss it."
        />

        <div className="actions inline">
          <button
            className="secondary"
            onClick={() => invoke("skip_next_break", { kind: "micro" })}
            disabled={settings.strict_mode || !settings.micro_skip_enabled}
          >
            Skip next micro
          </button>
          <button
            className="secondary"
            onClick={() => invoke("skip_next_break", { kind: "long" })}
            disabled={settings.strict_mode || !settings.long_skip_enabled}
          >
            Skip next long
          </button>
        </div>

        <Advanced label="Enforcement">
          <CheckboxRow
            label="Micro: wait for manual finish"
            value={settings.micro_manual_finish}
            onChange={(v) => update("micro_manual_finish", v)}
            tip={`The micro overlay stays up until you press "I'm back", instead of auto-closing when the countdown reaches zero.`}
          />
          <CheckboxRow
            label="Long: wait for manual finish"
            value={settings.long_manual_finish}
            onChange={(v) => update("long_manual_finish", v)}
            tip={`The long overlay stays up until you press "I'm back", instead of auto-closing when the countdown reaches zero.`}
          />
          <CheckboxRow
            label="Micro: cannot be dismissed"
            value={settings.micro_enforceable}
            onChange={(v) => update("micro_enforceable", v)}
            tip="Skip and close controls are hidden during the micro break. Use sparingly."
          />
          <CheckboxRow
            label="Long: cannot be dismissed"
            value={settings.long_enforceable}
            onChange={(v) => update("long_enforceable", v)}
            tip="Skip and close controls are hidden during the long break."
          />
        </Advanced>
      </section>

      <h2 id="settings-break-ideas">Break ideas</h2>
      <section>
        <p className="placeholder">
          Choose which kinds of prompt appear during each break.
          {isSupporter
            ? " Edit the pools below — one idea per line; each break picks a random starting idea."
            : ""}
        </p>
        <h3>Micro breaks</h3>
        <label className="row">
          <span>
            Mix
            <InfoTip text="Physical: stretches, eye rest, movement. Psychological: breathing, awareness, tension release." />
          </span>
          <select
            value={settings.micro_hint_mix}
            onChange={(e) =>
              update(
                "micro_hint_mix",
                e.target.value as typeof settings.micro_hint_mix,
              )
            }
          >
            <option value="both">Both</option>
            <option value="physical">Physical only</option>
            <option value="psychological">Psychological only</option>
          </select>
        </label>
        {routinePicker("micro")}
        {isSupporter && (
          <>
            <label className="row stacked">
              <span>Physical (stretches, eye rest, movement)</span>
              <textarea
                className="textarea"
                rows={6}
                value={microPhysical}
                onChange={(e) => setMicroPhysical(e.target.value)}
                onBlur={() =>
                  update("micro_physical_hints", linesToList(microPhysical))
                }
              />
            </label>
            <label className="row stacked">
              <span>Psychological (breathing, awareness, tension release)</span>
              <textarea
                className="textarea"
                rows={6}
                value={microPsychological}
                onChange={(e) => setMicroPsychological(e.target.value)}
                onBlur={() =>
                  update(
                    "micro_psychological_hints",
                    linesToList(microPsychological),
                  )
                }
              />
            </label>
          </>
        )}
        <h3>Long breaks</h3>
        <label className="row">
          <span>
            Mix
            <InfoTip text="Solo: things to do on your own (stretch, fresh air, snack). Social: things to do with someone (call, walk together, sit outside). Working alone? Pick Solo only to drop the social prompts." />
          </span>
          <select
            value={settings.long_hint_mix}
            onChange={(e) =>
              update(
                "long_hint_mix",
                e.target.value as typeof settings.long_hint_mix,
              )
            }
          >
            <option value="both">Both</option>
            <option value="solo">Solo only</option>
            <option value="social">Social only</option>
          </select>
        </label>
        {routinePicker("long")}
        <CheckboxRow
          label="Spread routine steps across the whole break"
          value={settings.routine_fill}
          onChange={(v) => update("routine_fill", v)}
          tip="When on, a routine's step durations are treated as relative weights and scaled to fill the full break length. When off (default), steps run at their authored durations and the last step holds until the break ends. A routine can override this per-routine with its own pacing field."
        />
        <CheckboxRow
          label="Play plugin sound cues"
          value={settings.allow_plugin_sounds}
          onChange={(v) => update("allow_plugin_sounds", v)}
          tip="When on (default), routines from plugins may play their own short sound cues — a breathing in/out tone, or a chime between exercises. Cues always follow your overall sound volume; turn this off to silence them."
        />
        <h3 id="settings-chores">Today's chores</h3>
        <p className="placeholder">
          Jot down chores you'd like done today — one per line. During a long
          break, Entracte nudges you to knock one out (these take precedence
          over the rotating wellness tips). The list clears each morning.
        </p>
        <label className="row stacked">
          <span>One chore per line</span>
          <textarea
            ref={choresRef}
            className="textarea"
            rows={6}
            value={choreLines}
            placeholder={"Water the plants\nEmpty the dishwasher\nReply to Sam"}
            onChange={(e) => setChoreLines(e.target.value)}
            onBlur={() => saveChores(linesToList(choreLines))}
          />
        </label>
        <CheckboxRow
          label="Prompt me to plan chores each morning"
          value={settings.morning_chore_prompt_enabled}
          onChange={(v) => update("morning_chore_prompt_enabled", v)}
          tip="When on (default), the first time your work window opens each day with an empty list, Entracte opens this Preferences window here so you can jot down the day's chores. Turn it off to never be prompted — you can still fill the list in yourself any time."
        />
        {isSupporter && (
          <>
            <label className="row stacked">
              <span>Solo (stretch, fresh air, snack, tidy)</span>
              <textarea
                className="textarea"
                rows={8}
                value={longSolo}
                onChange={(e) => setLongSolo(e.target.value)}
                onBlur={() => update("long_hints", linesToList(longSolo))}
              />
            </label>
            <label className="row stacked">
              <span>Social (call, walk together, share a coffee)</span>
              <textarea
                className="textarea"
                rows={6}
                value={longSocial}
                onChange={(e) => setLongSocial(e.target.value)}
                onBlur={() =>
                  update("long_social_hints", linesToList(longSocial))
                }
              />
            </label>
            <h3>Bedtime</h3>
            <label className="row stacked">
              <span>One idea per line</span>
              <textarea
                className="textarea"
                rows={6}
                value={sleep}
                onChange={(e) => setSleep(e.target.value)}
                onBlur={() => update("sleep_hints", linesToList(sleep))}
              />
            </label>
          </>
        )}
      </section>

      <h2 id="settings-content-packs">Content packs</h2>
      <section>
        <ContentPacks
          reload={async () => {
            await reload();
            reloadRoutines();
          }}
        />
      </section>

      {isSupporter && (
        <>
          <h2 id="settings-custom-css">Custom CSS</h2>
          <section>
            <p className="placeholder">
              Applied to the settings window and the break overlay. Bad CSS can
              hide controls — clear this field if anything breaks.
            </p>
            <label className="row stacked">
              <span>Stylesheet</span>
              <textarea
                className="textarea"
                rows={12}
                spellCheck={false}
                placeholder=".overlay-card { background: #111; }"
                value={customCss}
                onChange={(e) => setCustomCss(e.target.value)}
                onBlur={() => update("custom_css", customCss)}
              />
            </label>
          </section>
        </>
      )}
    </>
  );
}
