import { InfoTip } from "./info-tip";

// Preset stops mirrored on the select. The slider below covers any value
// in `[0.1, 1.0]` (the same clamp `centered_windowed_rect` applies in
// Rust), so the presets are just shortcuts — the slider is the custom
// control and is always visible while a concrete size is in effect.
const PRESETS = [0.7, 0.8, 0.9];

const toPct = (fraction: number): number => Math.round(fraction * 100);

const matchedPreset = (fraction: number): number | undefined =>
  PRESETS.find((p) => Math.abs(p - fraction) < 0.001);

export type WindowedSizeRowProps = {
  label: string;
  tip?: string;
  // `null` means "inherit the global fraction"; only offered when
  // `allowInherit` is set (the per-kind override rows).
  value: number | null;
  allowInherit: boolean;
  // The global fraction, used to seed the slider when an override is first
  // turned on so it starts from the value the user already sees.
  fallback: number;
  onChange: (value: number | null) => void;
};

export function WindowedSizeRow({
  label,
  tip,
  value,
  allowInherit,
  fallback,
  onChange,
}: WindowedSizeRowProps) {
  const isInherit = value === null;
  const effective = value ?? fallback;
  const preset = matchedPreset(effective);
  const selectValue = isInherit
    ? "inherit"
    : preset !== undefined
      ? String(preset)
      : "custom";

  return (
    <>
      <label className="row">
        <span>
          {label}
          {tip && <InfoTip text={tip} />}
        </span>
        <select
          value={selectValue}
          onChange={(e) => {
            const next = e.target.value;
            onChange(next === "inherit" ? null : Number(next));
          }}
        >
          {allowInherit && <option value="inherit">Same as global</option>}
          <option value="0.7">70%</option>
          <option value="0.8">80%</option>
          <option value="0.9">90%</option>
          <option value="custom" disabled>
            Custom
          </option>
        </select>
      </label>
      {!isInherit && (
        <label className="row">
          <span>Custom size</span>
          <span className="range-wrap">
            <input
              type="range"
              min={10}
              max={100}
              step={5}
              value={toPct(effective)}
              onChange={(e) => onChange(Number(e.target.value) / 100)}
            />
            <span className="range-value">{toPct(effective)}%</span>
          </span>
        </label>
      )}
    </>
  );
}
