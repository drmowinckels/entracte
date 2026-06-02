import type { Platform } from "./platform";

/** A single Quick-Add chip for the Auto-pause apps list. `tokens`
 * maps each supported platform to the process-name fragment we'd
 * match against; missing platforms hide the chip there. */
export type AppSuggestion = {
  label: string;
  tokens: Partial<Record<Platform, string>>;
};

/** Built-in suggestions shown as Quick-Add chips on the Quiet tab. */
export const APP_SUGGESTIONS: AppSuggestion[] = [
  { label: "Zoom", tokens: { macos: "zoom", windows: "zoom", linux: "zoom" } },
  {
    label: "Microsoft Teams",
    tokens: { macos: "teams", windows: "teams", linux: "teams" },
  },
  {
    label: "Slack",
    tokens: { macos: "slack", windows: "slack", linux: "slack" },
  },
  {
    label: "Webex",
    tokens: { macos: "webex", windows: "webex", linux: "webex" },
  },
  {
    label: "Discord",
    tokens: { macos: "discord", windows: "discord", linux: "discord" },
  },
  { label: "Keynote", tokens: { macos: "keynote" } },
  { label: "PowerPoint", tokens: { macos: "powerpoint", windows: "powerpnt" } },
  { label: "LibreOffice Impress", tokens: { linux: "impress" } },
  {
    label: "OBS Studio",
    tokens: { macos: "obs", windows: "obs", linux: "obs" },
  },
  { label: "QuickTime Player", tokens: { macos: "quicktime" } },
];

/** Get the platform-specific process-match token for a suggestion,
 * or `null` if the suggestion doesn't apply on this platform. */
export function tokenFor(
  suggestion: AppSuggestion,
  platform: Platform,
): string | null {
  return suggestion.tokens[platform] ?? null;
}

/** Subset of `APP_SUGGESTIONS` that have a token on this platform. */
export function suggestionsForPlatform(platform: Platform): AppSuggestion[] {
  return APP_SUGGESTIONS.filter((s) => tokenFor(s, platform) !== null);
}

/** True iff `existing` already contains `token` (trimmed,
 * case-insensitive). Used to grey out a chip the user has added. */
export function hasToken(existing: string[], token: string): boolean {
  const target = token.toLowerCase();
  return existing.some((e) => e.trim().toLowerCase() === target);
}
