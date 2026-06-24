import type { Tab } from "./types";

/** One searchable destination in the Settings window. `anchorId` must match
 * the `id` on a section heading inside the tab named by `tabId`, so the
 * navigator can switch to the tab and scroll the section into view. */
export type SettingsSearchEntry = {
  id: string;
  label: string;
  tabId: Tab;
  anchorId: string;
  /** Extra terms (synonyms, control names) folded into the match. */
  keywords: string;
};

/** Static map of every section a user might search for. Metadata only — the
 * controls themselves are rendered by their tab, not from this list. */
export const SETTINGS_INDEX: SettingsSearchEntry[] = [
  // Schedule
  {
    id: "active-hours",
    label: "Active hours",
    tabId: "schedule",
    anchorId: "settings-active-hours",
    keywords: "work window weekdays days start end time range when",
  },
  {
    id: "micro-breaks",
    label: "Micro breaks",
    tabId: "schedule",
    anchorId: "settings-micro-breaks",
    keywords: "interval fixed times duration enable cadence frequency idle",
  },
  {
    id: "long-breaks",
    label: "Long breaks",
    tabId: "schedule",
    anchorId: "settings-long-breaks",
    keywords: "interval fixed times duration enable cadence frequency idle",
  },
  {
    id: "bedtime",
    label: "Bedtime",
    tabId: "schedule",
    anchorId: "settings-bedtime",
    keywords: "sleep night wind down reminder window",
  },
  {
    id: "screen-time",
    label: "Daily screen time",
    tabId: "schedule",
    anchorId: "settings-screen-time",
    keywords: "budget limit usage time at keyboard wrap up",
  },
  // Breaks
  {
    id: "delivery",
    label: "Delivery mode",
    tabId: "breaks",
    anchorId: "settings-delivery",
    keywords: "overlay windowed notification fullscreen test how appears",
  },
  {
    id: "overlay",
    label: "Overlay appearance",
    tabId: "breaks",
    anchorId: "settings-overlay",
    keywords:
      "transparency opacity theme colour color text size high contrast monitor vignette",
  },
  {
    id: "sound",
    label: "Sound",
    tabId: "breaks",
    anchorId: "settings-sound",
    keywords: "volume chime ambient track audio custom file",
  },
  {
    id: "skip-postpone",
    label: "Skip & postpone",
    tabId: "breaks",
    anchorId: "settings-skip-postpone",
    keywords:
      "strict mode escalation enforce manual finish cannot be dismissed snooze",
  },
  {
    id: "break-ideas",
    label: "Break ideas",
    tabId: "breaks",
    anchorId: "settings-break-ideas",
    keywords: "hints routines mix physical psychological solo social guided",
  },
  {
    id: "chores",
    label: "Today's chores",
    tabId: "breaks",
    anchorId: "settings-chores",
    keywords: "tasks post-it morning prompt to do list",
  },
  {
    id: "content-packs",
    label: "Content packs",
    tabId: "breaks",
    anchorId: "settings-content-packs",
    keywords: "import export share routines hints json",
  },
  {
    id: "custom-css",
    label: "Custom CSS",
    tabId: "breaks",
    anchorId: "settings-custom-css",
    keywords: "stylesheet style supporter overlay appearance",
  },
  // Pausing
  {
    id: "auto-pause",
    label: "Auto-pause",
    tabId: "quiet",
    anchorId: "settings-auto-pause",
    keywords:
      "do not disturb dnd focus camera webcam fullscreen video suppress",
  },
  {
    id: "during-breaks",
    label: "Pause media during breaks",
    tabId: "quiet",
    anchorId: "settings-during-breaks",
    keywords: "music spotify video play pause media",
  },
  {
    id: "app-pause",
    label: "Pause for specific apps",
    tabId: "quiet",
    anchorId: "settings-app-pause",
    keywords: "zoom obs keynote running application suppress",
  },
  {
    id: "manual-pause",
    label: "Manual pause",
    tabId: "quiet",
    anchorId: "settings-manual-pause",
    keywords: "pause until resume holiday snooze",
  },
  // System
  {
    id: "startup",
    label: "Start at login",
    tabId: "system",
    anchorId: "settings-startup",
    keywords: "autostart boot launch",
  },
  {
    id: "display",
    label: "Time format",
    tabId: "system",
    anchorId: "settings-display",
    keywords: "clock 12 24 hour am pm",
  },
  {
    id: "notifications",
    label: "Notifications",
    tabId: "system",
    anchorId: "settings-notifications",
    keywords: "pre-break heads up lead time warning",
  },
  {
    id: "hotkeys",
    label: "Global hotkeys",
    tabId: "system",
    anchorId: "settings-hotkeys",
    keywords: "keyboard shortcuts accelerator pause resume trigger",
  },
  {
    id: "tray",
    label: "Tray countdown",
    tabId: "system",
    anchorId: "settings-tray",
    keywords: "menu bar icon timer countdown next break",
  },
  {
    id: "plugins",
    label: "Plugins",
    tabId: "system",
    anchorId: "settings-plugins",
    keywords: "extensions install content detector export",
  },
  {
    id: "hooks",
    label: "Event hooks",
    tabId: "system",
    anchorId: "settings-hooks",
    keywords: "shell command script break start end advanced automation",
  },
  // Insights
  {
    id: "insights",
    label: "Insights & stats",
    tabId: "insights",
    anchorId: "settings-insights",
    keywords: "stats history breaks taken skipped dismissed heatmap session",
  },
  {
    id: "manage-data",
    label: "Manage data",
    tabId: "insights",
    anchorId: "settings-manage-data",
    keywords: "export csv backup import clear history reset",
  },
  // Profiles
  {
    id: "profiles",
    label: "Profiles",
    tabId: "profiles",
    anchorId: "settings-profiles",
    keywords: "switch duplicate rename reset preset",
  },
  // About
  {
    id: "about",
    label: "About & updates",
    tabId: "about",
    anchorId: "settings-about",
    keywords: "version update check release",
  },
  {
    id: "supporter",
    label: "Supporter",
    tabId: "about",
    anchorId: "settings-supporter",
    keywords: "license key unlock customisation pack donate",
  },
  {
    id: "diagnostics",
    label: "Diagnostics",
    tabId: "about",
    anchorId: "settings-diagnostics",
    keywords: "report logs bug issue copy",
  },
];

const MAX_RESULTS = 8;

/** Case-insensitive AND-match over each entry's label + keywords. Returns at
 * most {@link MAX_RESULTS} entries, in index order. */
export function filterSettingsIndex(query: string): SettingsSearchEntry[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const terms = q.split(/\s+/);
  return SETTINGS_INDEX.filter((entry) => {
    const haystack = `${entry.label} ${entry.keywords}`.toLowerCase();
    return terms.every((term) => haystack.includes(term));
  }).slice(0, MAX_RESULTS);
}
