import type { BreakSoundMode, HookEvent, MonitorPlacement, Tab } from "./types";

export const MONITOR_PLACEMENTS: { id: MonitorPlacement; label: string }[] = [
  { id: "primary", label: "Primary monitor" },
  { id: "active", label: "Monitor under cursor" },
  { id: "all", label: "All monitors" },
];

export const HOOK_EVENTS: { id: HookEvent; label: string }[] = [
  { id: "break_start", label: "Break starts" },
  { id: "break_end", label: "Break ends" },
  { id: "break_postponed", label: "Break postponed" },
  { id: "break_skipped", label: "Break skipped" },
  { id: "pause_start", label: "Pause starts" },
  { id: "pause_end", label: "Pause ends" },
];

export const OVERLAY_THEMES = [
  { id: "dark", label: "Dark", rgb: "20, 24, 32" },
  { id: "midnight", label: "Midnight", rgb: "10, 14, 26" },
  { id: "forest", label: "Forest", rgb: "15, 31, 23" },
  { id: "rose", label: "Rose", rgb: "31, 15, 20" },
  { id: "sunset", label: "Sunset", rgb: "31, 24, 16" },
  { id: "rotate", label: "Rotate", rgb: "" },
  { id: "custom", label: "Custom…", rgb: "" },
];

export const ROTATION_GRADIENT =
  "linear-gradient(135deg, rgb(20, 24, 32) 0%, rgb(10, 14, 26) 25%, rgb(15, 31, 23) 50%, rgb(31, 15, 20) 75%, rgb(31, 24, 16) 100%)";

export const SOUND_MODES: { id: BreakSoundMode; label: string }[] = [
  { id: "off", label: "Off" },
  { id: "end_chime", label: "Chime at end of break" },
  { id: "ambient", label: "Ambient (loops during break)" },
];

export const TABS: { id: Tab; label: string }[] = [
  { id: "schedule", label: "Schedule" },
  { id: "breaks", label: "Breaks" },
  { id: "quiet", label: "Quiet times" },
  { id: "system", label: "System" },
  { id: "insights", label: "Insights" },
  { id: "profiles", label: "Profiles" },
  { id: "about", label: "About" },
];
