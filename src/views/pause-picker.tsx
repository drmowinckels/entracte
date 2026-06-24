import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  formatMinutesOfDay,
  parseMinutesOfDay,
  secondsUntil,
} from "../lib/time";
import {
  dateFieldOrder,
  monthNames,
  type DateField,
} from "../lib/locale-format";
import type { ClockFormat } from "./settings/types";
import "./pause-picker.css";

/** Clamp a day to the last valid day of the given month (e.g. 31 → 28/29
 * for February) so a stale day selection can't produce an invalid date. */
function clampDay(year: number, month: number, day: number): number {
  return Math.min(day, new Date(year, month + 1, 0).getDate());
}

/** Standalone popup launched from the tray's "Pause until…" item. Renders
 * its own date (locale-ordered) and time (honouring the app's 12h/24h
 * setting) fields rather than a native `datetime-local`, whose format the
 * WebView locks to its own locale (en-US in a non-localised app) regardless
 * of the OS region. Pauses all breaks until the chosen moment, then closes. */
export function PausePicker() {
  const now = useMemo(() => new Date(), []);
  const [locale, setLocale] = useState("en-US");
  const [clockFormat, setClockFormat] = useState<ClockFormat>("24h");
  const [year, setYear] = useState(now.getFullYear());
  const [month, setMonth] = useState(now.getMonth());
  const [day, setDay] = useState(now.getDate());
  const [timeDraft, setTimeDraft] = useState(() =>
    formatMinutesOfDay(now.getHours() * 60 + now.getMinutes(), "24h"),
  );

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const loc = await invoke<string>("get_locale");
        if (!cancelled && loc) setLocale(loc);
      } catch (e) {
        console.error("get_locale failed", e);
      }
      try {
        const s = await invoke<{ clock_format?: string }>("get_settings");
        if (!cancelled) {
          const fmt: ClockFormat = s?.clock_format === "12h" ? "12h" : "24h";
          setClockFormat(fmt);
          setTimeDraft(
            formatMinutesOfDay(now.getHours() * 60 + now.getMinutes(), fmt),
          );
        }
      } catch (e) {
        console.error("get_settings failed", e);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [now]);

  const order = useMemo(() => dateFieldOrder(locale), [locale]);
  const months = useMemo(() => monthNames(locale), [locale]);
  const years = useMemo(
    () => Array.from({ length: 3 }, (_, i) => now.getFullYear() + i),
    [now],
  );

  const minutes = parseMinutesOfDay(timeDraft);
  const target =
    minutes === null
      ? null
      : new Date(
          year,
          month,
          clampDay(year, month, day),
          Math.floor(minutes / 60),
          minutes % 60,
        );
  const secs = target ? secondsUntil(target) : null;

  const close = () => void invoke("close_pause_window");
  const submit = async () => {
    if (secs === null) return;
    await invoke("pause", { durationSecs: secs });
    close();
  };

  const fieldFor = (field: DateField) => {
    if (field === "day") {
      return (
        <select
          key="day"
          aria-label="Day"
          value={day}
          onChange={(e) => setDay(Number(e.target.value))}
        >
          {Array.from({ length: 31 }, (_, i) => i + 1).map((d) => (
            <option key={d} value={d}>
              {d}
            </option>
          ))}
        </select>
      );
    }
    if (field === "month") {
      return (
        <select
          key="month"
          aria-label="Month"
          value={month}
          onChange={(e) => setMonth(Number(e.target.value))}
        >
          {months.map((name, i) => (
            <option key={name} value={i}>
              {name}
            </option>
          ))}
        </select>
      );
    }
    return (
      <select
        key="year"
        aria-label="Year"
        value={year}
        onChange={(e) => setYear(Number(e.target.value))}
      >
        {years.map((y) => (
          <option key={y} value={y}>
            {y}
          </option>
        ))}
      </select>
    );
  };

  return (
    <main className="pause-picker">
      <h1 className="pause-picker-title">Pause until</h1>
      <p className="pause-picker-hint">
        Suppress all breaks until the chosen date and time, shown in your
        region&apos;s format.
      </p>
      <div className="pause-picker-date">{order.map(fieldFor)}</div>
      <label className="pause-picker-time">
        <span>Time</span>
        <input
          type="text"
          className="pause-picker-input"
          aria-label="Time"
          inputMode="numeric"
          spellCheck={false}
          placeholder={clockFormat === "12h" ? "h:mm AM/PM" : "HH:MM"}
          value={timeDraft}
          // The window opens for this picker, so focusing the time field is
          // expected rather than disorienting (mirrors the profile rename).
          // eslint-disable-next-line jsx-a11y/no-autofocus
          autoFocus
          onChange={(e) => setTimeDraft(e.target.value)}
          onBlur={() => {
            const m = parseMinutesOfDay(timeDraft);
            if (m !== null) setTimeDraft(formatMinutesOfDay(m, clockFormat));
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter" && secs !== null) void submit();
            if (e.key === "Escape") close();
          }}
        />
      </label>
      <div className="pause-picker-actions">
        <button type="button" className="secondary" onClick={close}>
          Cancel
        </button>
        <button
          type="button"
          disabled={secs === null}
          onClick={() => void submit()}
        >
          Pause
        </button>
      </div>
    </main>
  );
}
