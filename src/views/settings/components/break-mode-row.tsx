import { BREAK_MODE_OPTIONS, type BreakMode } from "../../../lib/break-mode";
import { InfoTip } from "./info-tip";

type EnabledKey = "micro_enabled" | "long_enabled";
type ModeKey = "micro_break_mode" | "long_break_mode";

export type BreakModeRowProps = {
  label: string;
  enabled: boolean;
  mode: string;
  enabledKey: EnabledKey;
  modeKey: ModeKey;
  onChange: (
    patch: Partial<Record<EnabledKey | ModeKey, boolean | string>>,
  ) => void;
  tip?: string;
};

export function BreakModeRow({
  label,
  enabled,
  mode,
  enabledKey,
  modeKey,
  onChange,
  tip,
}: BreakModeRowProps) {
  const current: "off" | BreakMode = enabled ? (mode as BreakMode) : "off";
  return (
    <label className="row">
      <span>
        {label}
        {tip && <InfoTip text={tip} />}
      </span>
      <select
        value={current}
        onChange={(e) => {
          const value = e.target.value;
          if (value === "off") {
            onChange({ [enabledKey]: false });
          } else {
            onChange({ [enabledKey]: true, [modeKey]: value });
          }
        }}
      >
        <option value="off">Off</option>
        {BREAK_MODE_OPTIONS.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>
    </label>
  );
}
