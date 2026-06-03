import { useEffect, useState, type ReactNode } from "react";
import { formatMinutesOfDay, parseMinutesOfDay } from "../../../lib/time";
import {
  PLATFORM_LABELS,
  type Platform,
  usePlatform,
} from "../../../lib/platform";
import { InfoTip } from "./info-tip";
import type { ClockFormat } from "../types";

type RowLabelProps = {
  label: ReactNode;
  tip?: string;
  tipWarn?: boolean;
};

function RowLabel({ label, tip, tipWarn }: RowLabelProps) {
  return (
    <span>
      {label}
      {tip && <InfoTip text={tip} warn={tipWarn} />}
    </span>
  );
}

export type NumberRowProps = {
  label: ReactNode;
  value: number;
  min: number;
  /** Display unit multiplier — e.g. 60 to show seconds as minutes. */
  multiplier: number;
  onChange: (next: number) => void;
  disabled?: boolean;
  tip?: string;
};

export function NumberRow({
  label,
  value,
  min,
  multiplier,
  onChange,
  disabled,
  tip,
}: NumberRowProps) {
  return (
    <label className={`row${disabled ? " disabled" : ""}`}>
      <RowLabel label={label} tip={tip} />
      <input
        type="number"
        min={min}
        disabled={disabled}
        value={Math.round(value / multiplier)}
        onChange={(e) => onChange(Number(e.target.value) * multiplier)}
      />
    </label>
  );
}

export type TimeRowProps = {
  label: ReactNode;
  value: number;
  onChange: (next: number) => void;
  disabled?: boolean;
  tip?: string;
  format?: ClockFormat;
};

export function TimeRow({
  label,
  value,
  onChange,
  disabled,
  tip,
  format = "24h",
}: TimeRowProps) {
  // Local draft state so the user can edit freely; commit on blur/Enter.
  // <input type="time"> can't be forced off the OS locale on WebKit, so
  // we own the rendering instead.
  const [draft, setDraft] = useState(() => formatMinutesOfDay(value, format));
  useEffect(() => {
    setDraft(formatMinutesOfDay(value, format));
  }, [value, format]);

  const commit = () => {
    const parsed = parseMinutesOfDay(draft);
    if (parsed === null) {
      setDraft(formatMinutesOfDay(value, format));
      return;
    }
    if (parsed !== value) onChange(parsed);
    setDraft(formatMinutesOfDay(parsed, format));
  };

  return (
    <label className={`row${disabled ? " disabled" : ""}`}>
      <RowLabel label={label} tip={tip} />
      <input
        type="text"
        className="time-input"
        inputMode="numeric"
        spellCheck={false}
        placeholder={format === "12h" ? "h:mm AM/PM" : "HH:MM"}
        value={draft}
        disabled={disabled}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") (e.target as HTMLInputElement).blur();
        }}
      />
    </label>
  );
}

export type CheckboxRowProps = {
  label: ReactNode;
  value: boolean;
  onChange: (next: boolean) => void;
  /** Restrict the control to these platforms; on others it renders disabled with a suffix. */
  onlyOn?: Platform[];
  tip?: string;
  /** Render the tip as a warning (caution glyph + styling) instead of plain info. */
  tipWarn?: boolean;
};

export function CheckboxRow({
  label,
  value,
  onChange,
  onlyOn,
  tip,
  tipWarn,
}: CheckboxRowProps) {
  const platform = usePlatform();
  const supported = !onlyOn || onlyOn.includes(platform);
  const displayLabel = supported
    ? label
    : `${label} (${onlyOn!.map((p) => PLATFORM_LABELS[p]).join("/")} only)`;
  return (
    <label className={`row checkbox-row${supported ? "" : " disabled"}`}>
      <RowLabel label={displayLabel} tip={tip} tipWarn={tipWarn} />
      <input
        type="checkbox"
        checked={value}
        disabled={!supported}
        onChange={(e) => onChange(e.target.checked)}
      />
    </label>
  );
}
