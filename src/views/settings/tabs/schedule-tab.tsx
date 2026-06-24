import { invoke } from "@tauri-apps/api/core";
import { formatClockList, parseClockList } from "../../../lib/clock-list";
import { useLocalDraft } from "../../../lib/use-local-draft";
import { formatScreenTime, progressPercent } from "../../../lib/screen-time";
import { Advanced } from "../components/advanced";
import { CheckboxRow, NumberRow, TimeRow } from "../components/rows";
import { WeekdayToggle } from "../components/weekday-toggle";
import type { UseSettings } from "../hooks/use-settings";
import { useScreenTime } from "../hooks/use-screen-time";
import type { SchedulerSettings } from "../types";

export function ScheduleTab({
  settings,
  update,
}: {
  settings: SchedulerSettings;
  update: UseSettings["update"];
}) {
  const [microFixedTimesText, setMicroFixedTimesText] = useLocalDraft(
    () => formatClockList(settings.micro_fixed_times, settings.clock_format),
    [settings.micro_fixed_times, settings.clock_format],
  );
  const [longFixedTimesText, setLongFixedTimesText] = useLocalDraft(
    () => formatClockList(settings.long_fixed_times, settings.clock_format),
    [settings.long_fixed_times, settings.clock_format],
  );

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
          format={settings.clock_format}
        />
        <TimeRow
          label="End"
          value={settings.work_end_minutes}
          onChange={(v) => update("work_end_minutes", v)}
          disabled={!settings.work_window_enabled}
          format={settings.clock_format}
        />
        <WeekdayToggle
          label="On these days"
          mask={settings.work_days_mask}
          onChange={(v) => update("work_days_mask", v)}
          disabled={!settings.work_window_enabled}
          tip="Breaks only fire within the hours above on the selected days — turn off weekends so Entracte stays quiet while you game or relax. A window that runs past midnight (e.g. 22:00–06:00) counts the early-morning hours as part of the day it started."
        />
      </section>

      <h2>Micro breaks</h2>
      <section>
        <CheckboxRow
          label="Enable micro breaks"
          value={settings.micro_enabled}
          onChange={(v) => update("micro_enabled", v)}
          tip="Short, frequent breaks. Set how they're delivered (overlay, windowed, or notification) on the Breaks tab."
        />
        {settings.micro_enabled && (
          <>
            <label className="row">
              <span>Schedule</span>
              <select
                value={settings.micro_schedule_mode}
                onChange={(e) =>
                  update(
                    "micro_schedule_mode",
                    e.target.value as typeof settings.micro_schedule_mode,
                  )
                }
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
                <span>
                  Fixed times (comma-separated,{" "}
                  {settings.clock_format === "12h" ? "h:mm AM/PM" : "hh:mm"})
                </span>
                <input
                  type="text"
                  value={microFixedTimesText}
                  onChange={(e) => setMicroFixedTimesText(e.target.value)}
                  onBlur={() => {
                    const parsed = parseClockList(microFixedTimesText);
                    setMicroFixedTimesText(
                      formatClockList(parsed, settings.clock_format),
                    );
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
            <Advanced label="Advanced micro timing">
              <NumberRow
                label="Idle reset threshold (minutes)"
                value={settings.micro_idle_reset_secs}
                min={1}
                multiplier={60}
                onChange={(v) => update("micro_idle_reset_secs", v)}
                tip="If you've been idle longer than this, the next-break timer resets when you come back — Entracte assumes you already took a break."
              />
            </Advanced>
          </>
        )}
      </section>

      <h2>Long breaks</h2>
      <section>
        <CheckboxRow
          label="Enable long breaks"
          value={settings.long_enabled}
          onChange={(v) => update("long_enabled", v)}
          tip="Longer, less frequent breaks. Set how they're delivered (overlay, windowed, or notification) on the Breaks tab."
        />
        {settings.long_enabled && (
          <>
            <label className="row">
              <span>Schedule</span>
              <select
                value={settings.long_schedule_mode}
                onChange={(e) =>
                  update(
                    "long_schedule_mode",
                    e.target.value as typeof settings.long_schedule_mode,
                  )
                }
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
                <span>
                  Fixed times (comma-separated,{" "}
                  {settings.clock_format === "12h" ? "h:mm AM/PM" : "hh:mm"})
                </span>
                <input
                  type="text"
                  value={longFixedTimesText}
                  onChange={(e) => setLongFixedTimesText(e.target.value)}
                  onBlur={() => {
                    const parsed = parseClockList(longFixedTimesText);
                    setLongFixedTimesText(
                      formatClockList(parsed, settings.clock_format),
                    );
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
            <Advanced label="Advanced long timing">
              <NumberRow
                label="Idle reset threshold (minutes)"
                value={settings.long_idle_reset_secs}
                min={1}
                multiplier={60}
                onChange={(v) => update("long_idle_reset_secs", v)}
                tip="If you've been idle longer than this, the next-break timer resets when you come back — Entracte assumes you already took a break."
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
              format={settings.clock_format}
            />
            <TimeRow
              label="End"
              value={settings.bedtime_end_minutes}
              onChange={(v) => update("bedtime_end_minutes", v)}
              format={settings.clock_format}
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
                  invoke("trigger_test_break", {
                    kind: "sleep",
                    durationSecs: 15,
                  })
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
                {formatScreenTime(
                  settings.daily_screen_time_budget_minutes * 60,
                )}
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
