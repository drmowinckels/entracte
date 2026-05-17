import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { formatClockList, parseClockList } from "../../../lib/clock-list";
import {
  formatScreenTime,
  progressPercent,
} from "../../../lib/screen-time";
import { Advanced } from "../components/advanced";
import { BreakModeRow } from "../components/break-mode-row";
import { CheckboxRow, NumberRow, TimeRow } from "../components/rows";
import { SoundControls } from "../components/sound-controls";
import type { UseSettings } from "../hooks/use-settings";
import { useScreenTime } from "../hooks/use-screen-time";
import type { SchedulerSettings } from "../types";

export function ScheduleTab({
  settings,
  update,
  updateMany,
}: {
  settings: SchedulerSettings;
  update: UseSettings["update"];
  updateMany: UseSettings["updateMany"];
}) {
  const [microFixedTimesText, setMicroFixedTimesText] = useState(
    formatClockList(settings.micro_fixed_times),
  );
  const [longFixedTimesText, setLongFixedTimesText] = useState(
    formatClockList(settings.long_fixed_times),
  );

  useEffect(() => {
    setMicroFixedTimesText(formatClockList(settings.micro_fixed_times));
  }, [settings.micro_fixed_times]);
  useEffect(() => {
    setLongFixedTimesText(formatClockList(settings.long_fixed_times));
  }, [settings.long_fixed_times]);

  const screenTime = useScreenTime(settings.daily_screen_time_enabled);

  return (
    <>
      <h2>Active hours</h2>
      <section>
        <CheckboxRow
          label="Only fire breaks within set hours"
          value={settings.work_window_enabled}
          onChange={(v) => update("work_window_enabled", v)}
          tip="Outside this window, breaks won't fire. Bedtime reminders ignore this and use their own window below."
        />
        <TimeRow
          label="Start"
          value={settings.work_start_minutes}
          onChange={(v) => update("work_start_minutes", v)}
          disabled={!settings.work_window_enabled}
        />
        <TimeRow
          label="End"
          value={settings.work_end_minutes}
          onChange={(v) => update("work_end_minutes", v)}
          disabled={!settings.work_window_enabled}
        />
      </section>

      <h2>Micro breaks</h2>
      <section>
        <BreakModeRow
          label="Mode"
          enabled={settings.micro_enabled}
          mode={settings.micro_break_mode}
          enabledKey="micro_enabled"
          modeKey="micro_break_mode"
          onChange={(patch) => updateMany(patch as Partial<SchedulerSettings>)}
          tip="Overlay = full-screen prompt. Windowed = the same prompt sized to 80% of the monitor, leaving the desktop reachable. Notification = system notification only (skip/postpone metrics aren't recorded in this mode)."
        />
        {settings.micro_enabled && (
          <>
            <label className="row">
              <span>Schedule</span>
              <select
                value={settings.micro_schedule_mode}
                onChange={(e) => update("micro_schedule_mode", e.target.value)}
              >
                <option value="interval">Interval</option>
                <option value="fixed">Fixed times</option>
                <option value="both">Both</option>
              </select>
            </label>
            {(settings.micro_schedule_mode === "interval" ||
              settings.micro_schedule_mode === "both") && (
              <NumberRow
                label="Interval (minutes)"
                value={settings.micro_interval_secs}
                min={1}
                multiplier={60}
                onChange={(v) => update("micro_interval_secs", v)}
              />
            )}
            {(settings.micro_schedule_mode === "fixed" ||
              settings.micro_schedule_mode === "both") && (
              <label className="row">
                <span>Fixed times (comma-separated, hh:mm)</span>
                <input
                  type="text"
                  value={microFixedTimesText}
                  onChange={(e) => setMicroFixedTimesText(e.target.value)}
                  onBlur={() => {
                    const parsed = parseClockList(microFixedTimesText);
                    setMicroFixedTimesText(formatClockList(parsed));
                    update("micro_fixed_times", parsed);
                  }}
                />
              </label>
            )}
            <NumberRow
              label="Duration (seconds)"
              value={settings.micro_duration_secs}
              min={5}
              multiplier={1}
              onChange={(v) => update("micro_duration_secs", v)}
            />
            <SoundControls
              sound={settings.micro_sound}
              volume={settings.sound_volume}
              onChange={(next) => update("micro_sound", next)}
            />
            <div className="actions inline">
              <button
                onClick={() =>
                  invoke("trigger_test_break", { kind: "micro", durationSecs: 10 })
                }
              >
                Test now (10s)
              </button>
            </div>
            <Advanced>
              <NumberRow
                label="Idle reset threshold (minutes)"
                value={settings.micro_idle_reset_secs}
                min={1}
                multiplier={60}
                onChange={(v) => update("micro_idle_reset_secs", v)}
                tip="If you've been idle longer than this, the next-break timer resets when you come back — Entracte assumes you already took a break."
              />
              <CheckboxRow
                label="Cannot be dismissed"
                value={settings.micro_enforceable}
                onChange={(v) => update("micro_enforceable", v)}
                tip="Skip and close controls are hidden during the break. Use sparingly."
              />
              <CheckboxRow
                label="Wait for manual finish"
                value={settings.micro_manual_finish}
                onChange={(v) => update("micro_manual_finish", v)}
                tip={`The overlay stays up until you press "I'm back", instead of auto-closing when the countdown reaches zero.`}
              />
            </Advanced>
          </>
        )}
      </section>

      <h2>Long breaks</h2>
      <section>
        <BreakModeRow
          label="Mode"
          enabled={settings.long_enabled}
          mode={settings.long_break_mode}
          enabledKey="long_enabled"
          modeKey="long_break_mode"
          onChange={(patch) => updateMany(patch as Partial<SchedulerSettings>)}
          tip="Overlay = full-screen prompt. Windowed = the same prompt sized to 80% of the monitor, leaving the desktop reachable. Notification = system notification only (skip/postpone metrics aren't recorded in this mode)."
        />
        {settings.long_enabled && (
          <>
            <label className="row">
              <span>Schedule</span>
              <select
                value={settings.long_schedule_mode}
                onChange={(e) => update("long_schedule_mode", e.target.value)}
              >
                <option value="interval">Interval</option>
                <option value="fixed">Fixed times</option>
                <option value="both">Both</option>
              </select>
            </label>
            {(settings.long_schedule_mode === "interval" ||
              settings.long_schedule_mode === "both") && (
              <NumberRow
                label="Interval (minutes)"
                value={settings.long_interval_secs}
                min={5}
                multiplier={60}
                onChange={(v) => update("long_interval_secs", v)}
              />
            )}
            {(settings.long_schedule_mode === "fixed" ||
              settings.long_schedule_mode === "both") && (
              <label className="row">
                <span>Fixed times (comma-separated, hh:mm)</span>
                <input
                  type="text"
                  value={longFixedTimesText}
                  onChange={(e) => setLongFixedTimesText(e.target.value)}
                  onBlur={() => {
                    const parsed = parseClockList(longFixedTimesText);
                    setLongFixedTimesText(formatClockList(parsed));
                    update("long_fixed_times", parsed);
                  }}
                />
              </label>
            )}
            <NumberRow
              label="Duration (minutes)"
              value={settings.long_duration_secs}
              min={1}
              multiplier={60}
              onChange={(v) => update("long_duration_secs", v)}
            />
            <SoundControls
              sound={settings.long_sound}
              volume={settings.sound_volume}
              onChange={(next) => update("long_sound", next)}
            />
            <div className="actions inline">
              <button
                onClick={() =>
                  invoke("trigger_test_break", { kind: "long", durationSecs: 15 })
                }
              >
                Test now (15s)
              </button>
            </div>
            <Advanced>
              <NumberRow
                label="Idle reset threshold (minutes)"
                value={settings.long_idle_reset_secs}
                min={1}
                multiplier={60}
                onChange={(v) => update("long_idle_reset_secs", v)}
                tip="If you've been idle longer than this, the next-break timer resets when you come back — Entracte assumes you already took a break."
              />
              <CheckboxRow
                label="Cannot be dismissed"
                value={settings.long_enforceable}
                onChange={(v) => update("long_enforceable", v)}
                tip="Skip and close controls are hidden during the break."
              />
              <CheckboxRow
                label="Wait for manual finish"
                value={settings.long_manual_finish}
                onChange={(v) => update("long_manual_finish", v)}
                tip={`The overlay stays up until you press "I'm back", instead of auto-closing when the countdown reaches zero.`}
              />
            </Advanced>
          </>
        )}
      </section>

      <h2>Bedtime</h2>
      <section>
        <CheckboxRow
          label="Persistent sleep reminders within window"
          value={settings.bedtime_enabled}
          onChange={(v) => update("bedtime_enabled", v)}
          tip="Inside the bedtime window, Entracte fires a Sleep prompt instead of micro or long breaks. Sleep prompts always show — they ignore DnD and camera-in-use."
        />
        {settings.bedtime_enabled && (
          <>
            <TimeRow
              label="Start"
              value={settings.bedtime_start_minutes}
              onChange={(v) => update("bedtime_start_minutes", v)}
            />
            <TimeRow
              label="End"
              value={settings.bedtime_end_minutes}
              onChange={(v) => update("bedtime_end_minutes", v)}
            />
            <NumberRow
              label="Reminder interval (minutes)"
              value={settings.bedtime_interval_secs}
              min={1}
              multiplier={60}
              onChange={(v) => update("bedtime_interval_secs", v)}
            />
            <NumberRow
              label="Reminder duration (seconds)"
              value={settings.bedtime_duration_secs}
              min={5}
              multiplier={1}
              onChange={(v) => update("bedtime_duration_secs", v)}
            />
            <div className="actions inline">
              <button
                onClick={() =>
                  invoke("trigger_test_break", { kind: "sleep", durationSecs: 15 })
                }
              >
                Test now (15s)
              </button>
            </div>
          </>
        )}
      </section>

      <h2>Daily screen time</h2>
      <section>
        <CheckboxRow
          label="Remind me to wrap up after a daily budget"
          value={settings.daily_screen_time_enabled}
          onChange={(v) => update("daily_screen_time_enabled", v)}
          tip="Counts only active typing/clicking. Resets at local midnight. The reminder is a system notification, not a forced break."
        />
        {settings.daily_screen_time_enabled && (
          <>
            <NumberRow
              label="Daily budget (hours)"
              value={settings.daily_screen_time_budget_minutes}
              min={1}
              multiplier={60}
              onChange={(v) => update("daily_screen_time_budget_minutes", v)}
            />
            <NumberRow
              label="Remind again after (minutes, 0 = once per day)"
              value={settings.daily_screen_time_remind_again_minutes}
              min={0}
              multiplier={1}
              onChange={(v) =>
                update("daily_screen_time_remind_again_minutes", v)
              }
            />
            <div className="screen-time-progress">
              <span className="screen-time-label">Today</span>
              <span className="screen-time-value">
                {formatScreenTime(screenTime?.seconds ?? 0)} /{" "}
                {formatScreenTime(settings.daily_screen_time_budget_minutes * 60)}
              </span>
              <div
                className="screen-time-bar"
                role="progressbar"
                aria-label="Daily screen time progress"
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={progressPercent(
                  screenTime?.seconds ?? 0,
                  settings.daily_screen_time_budget_minutes,
                )}
              >
                <span
                  ref={(el) => {
                    el?.style.setProperty(
                      "--screen-time-progress",
                      `${progressPercent(
                        screenTime?.seconds ?? 0,
                        settings.daily_screen_time_budget_minutes,
                      )}%`,
                    );
                  }}
                />
              </div>
            </div>
          </>
        )}
      </section>

      <Advanced label="Show advanced scheduling">
        <h3>Input-aware scheduling</h3>
        <CheckboxRow
          label="Delay break if I'm typing"
          value={settings.delay_break_if_typing}
          onChange={(v) => update("delay_break_if_typing", v)}
          tip="If you're mid-keystroke when a break is due, it waits until you pause typing — up to the maximum deferral below."
        />
        {settings.delay_break_if_typing && (
          <>
            <NumberRow
              label="Treat input within (seconds) as typing"
              value={settings.typing_grace_secs}
              min={1}
              multiplier={1}
              onChange={(v) => update("typing_grace_secs", v)}
            />
            <NumberRow
              label="Maximum deferral (seconds)"
              value={settings.typing_max_deferral_secs}
              min={1}
              multiplier={1}
              onChange={(v) => update("typing_max_deferral_secs", v)}
            />
          </>
        )}
        <CheckboxRow
          label="Pause break countdown while I'm typing"
          value={settings.pause_countdown_if_typing}
          onChange={(v) => update("pause_countdown_if_typing", v)}
          tip={`During a break, the countdown only ticks while you're not typing. Useful with "Wait for manual finish" off.`}
        />
      </Advanced>
    </>
  );
}
