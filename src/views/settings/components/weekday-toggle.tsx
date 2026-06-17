import type { ReactNode } from "react";
import { dayActive, toggleDay, WEEKDAYS } from "../../../lib/weekdays";
import { InfoTip } from "./info-tip";

export type WeekdayToggleProps = {
  label: ReactNode;
  /** 7-bit weekday mask (bit 0 = Monday … bit 6 = Sunday). */
  mask: number;
  onChange: (next: number) => void;
  disabled?: boolean;
  tip?: string;
};

/** A row of seven toggle buttons for picking which weekdays the work
 * window applies to. Each button is a labelled `aria-pressed` toggle so
 * screen-reader and keyboard users get the full state and a focus path. */
export function WeekdayToggle({
  label,
  mask,
  onChange,
  disabled,
  tip,
}: WeekdayToggleProps) {
  return (
    <div className={`row weekday-row${disabled ? " disabled" : ""}`}>
      <span>
        {label}
        {tip && <InfoTip text={tip} />}
      </span>
      <div
        className="weekday-toggle"
        role="group"
        aria-label="Days the work window applies to"
      >
        {WEEKDAYS.map((day) => {
          const active = dayActive(mask, day.bit);
          return (
            <button
              key={day.bit}
              type="button"
              className={`weekday-chip${active ? " active" : ""}`}
              aria-pressed={active}
              aria-label={day.name}
              disabled={disabled}
              onClick={() => onChange(toggleDay(mask, day.bit))}
            >
              {day.abbr}
            </button>
          );
        })}
      </div>
    </div>
  );
}
