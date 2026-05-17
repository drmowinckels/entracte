import type { ReactNode } from "react";
import { minutesToTime, timeToMinutes } from "../../../lib/time";
import { PLATFORM_LABELS, type Platform, usePlatform } from "../../../lib/platform";
import { InfoTip } from "./info-tip";

type RowLabelProps = {
  label: ReactNode;
  tip?: string;
};

function RowLabel({ label, tip }: RowLabelProps) {
  return (
    <span>
      {label}
      {tip && <InfoTip text={tip} />}
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
};

export function TimeRow({ label, value, onChange, disabled, tip }: TimeRowProps) {
  return (
    <label className={`row${disabled ? " disabled" : ""}`}>
      <RowLabel label={label} tip={tip} />
      <input
        type="time"
        value={minutesToTime(value)}
        disabled={disabled}
        onChange={(e) => onChange(timeToMinutes(e.target.value))}
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
};

export function CheckboxRow({
  label,
  value,
  onChange,
  onlyOn,
  tip,
}: CheckboxRowProps) {
  const platform = usePlatform();
  const supported = !onlyOn || onlyOn.includes(platform);
  const displayLabel = supported
    ? label
    : `${label} (${onlyOn!.map((p) => PLATFORM_LABELS[p]).join("/")} only)`;
  return (
    <label className={`row checkbox-row${supported ? "" : " disabled"}`}>
      <RowLabel label={displayLabel} tip={tip} />
      <input
        type="checkbox"
        checked={value}
        disabled={!supported}
        onChange={(e) => onChange(e.target.checked)}
      />
    </label>
  );
}
