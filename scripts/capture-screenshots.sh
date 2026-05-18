#!/usr/bin/env bash
# Capture break-overlay screenshots for the docs site.
#
# Drives the running Entracte app via its CLI:
#   - Backs up overlay-related settings, restores on exit (including on error).
#   - Forces opacity=1.0 + high-contrast=false so screencapture doesn't pick
#     up whatever's behind a translucent overlay.
#   - Trims micro_duration_secs so the overlay disappears quickly between
#     shots without us having to manually dismiss.
#   - For each variant: trigger → 1.2 s settle → look up the overlay's
#     CGWindowID via a tiny Swift one-liner → `screencapture -l <id>` so
#     only the overlay window's pixels land in the PNG. The user's other
#     apps, the menu bar, and the dock stay completely off-frame.
#
# Run from the repo root with the app already running (`npm run tauri dev`
# in another shell). Output PNGs land in docs/screenshots/.
#
# Requires: bash 4+, macOS screencapture, /usr/bin/swift (ships with Xcode CLT).

set -euo pipefail

ENTRACTE="${ENTRACTE:-$(find "$(pwd)" -path '*/src-tauri/target/debug/entracte' -type f 2>/dev/null | head -1)}"
if [ -z "$ENTRACTE" ] || [ ! -x "$ENTRACTE" ]; then
  echo "couldn't find an executable entracte binary; set ENTRACTE=/path/to/entracte" >&2
  exit 1
fi
OUT_DIR="$(pwd)/docs/screenshots"
mkdir -p "$OUT_DIR"

SWIFT_QUERY='import Cocoa; import Quartz
guard let infos = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] else { exit(1) }
for w in infos {
  let owner = w[kCGWindowOwnerName as String] as? String ?? ""
  let name = w[kCGWindowName as String] as? String ?? ""
  if owner.lowercased().contains("ntracte") && name == "Entracte Break" {
    print(w[kCGWindowNumber as String] as? Int ?? 0)
    exit(0)
  }
}
exit(2)'

find_overlay_id() {
  /usr/bin/swift -e "$SWIFT_QUERY" 2>/dev/null
}

backup_one() { "$ENTRACTE" settings get "$1"; }

ORIG_OPACITY=$(backup_one overlay_opacity)
ORIG_COLOR=$(backup_one overlay_color)
ORIG_HIGH_CONTRAST=$(backup_one overlay_high_contrast)
ORIG_MICRO_MODE=$(backup_one micro_break_mode)
ORIG_MICRO_DURATION=$(backup_one micro_duration_secs)
ORIG_MICRO_MANUAL=$(backup_one micro_manual_finish)
ORIG_POSTPONE=$(backup_one postpone_enabled)
ORIG_SHOW_HINT=$(backup_one show_hint)
ORIG_PAUSE_TYPING=$(backup_one pause_countdown_if_typing)

restore() {
  echo "restoring settings..." >&2
  "$ENTRACTE" settings set overlay_opacity "$ORIG_OPACITY" >/dev/null || true
  "$ENTRACTE" settings set overlay_color "$ORIG_COLOR" >/dev/null || true
  "$ENTRACTE" settings set overlay_high_contrast "$ORIG_HIGH_CONTRAST" >/dev/null || true
  "$ENTRACTE" settings set micro_break_mode "$ORIG_MICRO_MODE" >/dev/null || true
  "$ENTRACTE" settings set micro_duration_secs "$ORIG_MICRO_DURATION" >/dev/null || true
  "$ENTRACTE" settings set micro_manual_finish "$ORIG_MICRO_MANUAL" >/dev/null || true
  "$ENTRACTE" settings set postpone_enabled "$ORIG_POSTPONE" >/dev/null || true
  "$ENTRACTE" settings set show_hint "$ORIG_SHOW_HINT" >/dev/null || true
  "$ENTRACTE" settings set pause_countdown_if_typing "$ORIG_PAUSE_TYPING" >/dev/null || true
}
trap restore EXIT

# Screenshot-friendly state.
"$ENTRACTE" settings set overlay_opacity 1.0 >/dev/null
"$ENTRACTE" settings set overlay_high_contrast false >/dev/null
"$ENTRACTE" settings set show_hint true >/dev/null
"$ENTRACTE" settings set postpone_enabled true >/dev/null
"$ENTRACTE" settings set micro_manual_finish false >/dev/null
# Long enough that we have time to capture, short enough to recycle quickly.
"$ENTRACTE" settings set micro_duration_secs 8 >/dev/null
# Don't pause the countdown while we're typing into the shell.
"$ENTRACTE" settings set pause_countdown_if_typing false >/dev/null

capture() {
  local name="$1"
  "$ENTRACTE" trigger micro >/dev/null
  sleep 1.5
  local id
  id=$(find_overlay_id || true)
  if [ -z "$id" ]; then
    echo "  ⚠ couldn't find overlay window for '$name' — skipping" >&2
    sleep 8
    return
  fi
  screencapture -l "$id" -o "$OUT_DIR/${name}.png"
  echo "  → $OUT_DIR/${name}.png  (window $id)"
  # Wait out the remaining duration before triggering the next break.
  sleep 8
}

set_theme() {
  "$ENTRACTE" --colour="$1" >/dev/null
  sleep 0.4
}

echo "[1/3] Theme variants (fullscreen overlay)"
"$ENTRACTE" settings set micro_break_mode '"overlay"' >/dev/null
for theme in dark midnight forest rose sunset; do
  set_theme "$theme"
  capture "break-overlay-${theme}"
done

echo "[2/3] Windowed mode (dark theme)"
set_theme dark
"$ENTRACTE" settings set micro_break_mode '"windowed"' >/dev/null
capture "break-overlay-windowed"
"$ENTRACTE" settings set micro_break_mode '"overlay"' >/dev/null

echo "[3/3] High vignette (after skipping 5 breaks — drives intensity to 1.0)"
set_theme dark
for _ in 1 2 3 4 5; do "$ENTRACTE" skip micro >/dev/null; done
capture "break-overlay-high-vignette"

# Replace the old break-overlay-active.png too (it's the dark-theme fullscreen).
cp "$OUT_DIR/break-overlay-dark.png" "$OUT_DIR/break-overlay-active.png"

echo "done"
