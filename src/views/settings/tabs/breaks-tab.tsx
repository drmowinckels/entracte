import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import {
  clampCsvToDark,
  hexToRgbCsv,
  normalizeHexInput,
  rgbCsvToHex,
} from "../../../lib/color";
import { Advanced } from "../components/advanced";
import { CheckboxRow, NumberRow } from "../components/rows";
import { InfoTip } from "../components/info-tip";
import {
  MONITOR_PLACEMENTS,
  OVERLAY_THEMES,
  ROTATION_GRADIENT,
} from "../constants";
import type { UseSettings } from "../hooks/use-settings";
import type { MonitorPlacement, SchedulerSettings, SupporterStatus } from "../types";
import { linesToList, listToLines } from "../utils";

export function BreaksTab({
  settings,
  update,
  supporter,
}: {
  settings: SchedulerSettings;
  update: UseSettings["update"];
  supporter: SupporterStatus;
}) {
  const isSupporter = supporter.is_supporter;
  const [microPhysical, setMicroPhysical] = useState(
    listToLines(settings.micro_physical_hints),
  );
  const [microPsychological, setMicroPsychological] = useState(
    listToLines(settings.micro_psychological_hints),
  );
  const [longSolo, setLongSolo] = useState(listToLines(settings.long_hints));
  const [longSocial, setLongSocial] = useState(
    listToLines(settings.long_social_hints),
  );
  const [sleep, setSleep] = useState(listToLines(settings.sleep_hints));

  // Re-seed when the active profile changes underneath us.
  useEffect(() => {
    setMicroPhysical(listToLines(settings.micro_physical_hints));
  }, [settings.micro_physical_hints]);
  useEffect(() => {
    setMicroPsychological(listToLines(settings.micro_psychological_hints));
  }, [settings.micro_psychological_hints]);
  useEffect(() => {
    setLongSolo(listToLines(settings.long_hints));
  }, [settings.long_hints]);
  useEffect(() => {
    setLongSocial(listToLines(settings.long_social_hints));
  }, [settings.long_social_hints]);
  useEffect(() => {
    setSleep(listToLines(settings.sleep_hints));
  }, [settings.sleep_hints]);

  return (
    <>
      <h2>Overlay</h2>
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
              value={Math.round((1 - settings.overlay_opacity) * 100)}
              onChange={(e) =>
                update("overlay_opacity", 1 - Number(e.target.value) / 100)
              }
            />
            <span className="range-value">
              {Math.round((1 - settings.overlay_opacity) * 100)}%
            </span>
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
              value={Math.round(settings.overlay_font_scale * 100)}
              onChange={(e) =>
                update("overlay_font_scale", Number(e.target.value) / 100)
              }
            />
            <span className="range-value">
              {Math.round(settings.overlay_font_scale * 100)}%
            </span>
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
                          : OVERLAY_THEMES.find(
                              (t) => t.id === settings.overlay_color,
                            )?.rgb ?? OVERLAY_THEMES[0].rgb
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
                const disabled =
                  supporterOnly &&
                  !isSupporter &&
                  settings.overlay_color !== t.id;
                return (
                  <option key={t.id} value={t.id} disabled={disabled}>
                    {t.label}
                    {supporterOnly ? " (Supporter)" : ""}
                  </option>
                );
              })}
            </select>
          </span>
        </label>
        {!isSupporter &&
          (settings.overlay_color === "rotate" ||
            settings.overlay_color === "custom") && (
            <p className="placeholder">
              This theme is part of the Supporter pack — see the About tab to
              unlock it.
            </p>
          )}
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

      <h2>Sound volume</h2>
      <section>
        <p className="placeholder">
          Applies to every break sound. Pick which sound each break uses on the
          Breaks tab.
        </p>
        <label className="row">
          <span>Volume</span>
          <span className="range-wrap">
            <input
              type="range"
              min={0}
              max={100}
              step={1}
              value={Math.round(settings.sound_volume * 100)}
              onChange={(e) => update("sound_volume", Number(e.target.value) / 100)}
            />
            <span className="range-value">
              {Math.round(settings.sound_volume * 100)}%
            </span>
          </span>
        </label>
      </section>

      <h2>Skip & postpone</h2>
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
        />
        {settings.postpone_enabled && !settings.strict_mode && (
          <NumberRow
            label="Postpone by (minutes)"
            value={settings.postpone_minutes}
            min={1}
            multiplier={1}
            onChange={(v) => update("postpone_minutes", v)}
          />
        )}
        {settings.postpone_enabled && !settings.strict_mode && (
          <CheckboxRow
            label="Escalate each subsequent postpone of the same break"
            value={settings.postpone_escalation_enabled}
            onChange={(v) => update("postpone_escalation_enabled", v)}
            tip="Each postpone of the same break adds extra delay, making repeated postponing progressively less attractive."
          />
        )}
        {settings.postpone_enabled &&
          !settings.strict_mode &&
          settings.postpone_escalation_enabled && (
            <>
              <NumberRow
                label="Extra delay per postpone (seconds)"
                value={settings.postpone_escalation_step_secs}
                min={0}
                multiplier={1}
                onChange={(v) => update("postpone_escalation_step_secs", v)}
              />
              <NumberRow
                label="Maximum postpones per break"
                value={settings.postpone_max_count}
                min={1}
                multiplier={1}
                onChange={(v) => update("postpone_max_count", v)}
              />
            </>
          )}
        <div className="actions inline">
          <button
            className="secondary"
            onClick={() => invoke("skip_next_break", { kind: "micro" })}
            disabled={settings.strict_mode}
          >
            Skip next micro
          </button>
          <button
            className="secondary"
            onClick={() => invoke("skip_next_break", { kind: "long" })}
            disabled={settings.strict_mode}
          >
            Skip next long
          </button>
        </div>
      </section>

      <h2>Break ideas</h2>
      <section>
        <p className="placeholder">
          One idea per line. Each break picks a random starting idea from the
          pool.
        </p>
        {!isSupporter && (
          <p className="placeholder">
            Editing the hint pools is part of the Supporter pack — see the About
            tab to unlock. The default hints stay available either way.
          </p>
        )}
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
        <h3>Micro breaks</h3>
        <label className="row">
          <span>Mix</span>
          <select
            value={settings.micro_hint_mix}
            onChange={(e) => update("micro_hint_mix", e.target.value)}
            disabled={!isSupporter}
          >
            <option value="both">Both</option>
            <option value="physical">Physical only</option>
            <option value="psychological">Psychological only</option>
          </select>
        </label>
        <label className="row stacked">
          <span>Physical (stretches, eye rest, movement)</span>
          <textarea
            className="textarea"
            rows={6}
            value={microPhysical}
            onChange={(e) => setMicroPhysical(e.target.value)}
            onBlur={() => update("micro_physical_hints", linesToList(microPhysical))}
            readOnly={!isSupporter}
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
              update("micro_psychological_hints", linesToList(microPsychological))
            }
            readOnly={!isSupporter}
          />
        </label>
        <h3>Long breaks</h3>
        <label className="row">
          <span>
            Mix
            <InfoTip text="Solo: things to do on your own (stretch, fresh air, snack). Social: things to do with someone (call, walk together, sit outside)." />
          </span>
          <select
            value={settings.long_hint_mix}
            onChange={(e) => update("long_hint_mix", e.target.value)}
            disabled={!isSupporter}
          >
            <option value="both">Both</option>
            <option value="solo">Solo only</option>
            <option value="social">Social only</option>
          </select>
        </label>
        <label className="row stacked">
          <span>Solo (stretch, fresh air, snack, tidy)</span>
          <textarea
            className="textarea"
            rows={8}
            value={longSolo}
            onChange={(e) => setLongSolo(e.target.value)}
            onBlur={() => update("long_hints", linesToList(longSolo))}
            readOnly={!isSupporter}
          />
        </label>
        <label className="row stacked">
          <span>Social (call, walk together, share a coffee)</span>
          <textarea
            className="textarea"
            rows={6}
            value={longSocial}
            onChange={(e) => setLongSocial(e.target.value)}
            onBlur={() => update("long_social_hints", linesToList(longSocial))}
            readOnly={!isSupporter}
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
            readOnly={!isSupporter}
          />
        </label>
      </section>
    </>
  );
}
