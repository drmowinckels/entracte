#!/usr/bin/env bash
set -euo pipefail

# Re-encode break sounds from lossless originals to small MP3s for shipping.
#
# Originals are not committed to the repo. Place them in a directory (default:
# /tmp/breakly-sound-sources) named <id>-original.<wav|flac|mp3>, then run:
#
#   ./apps/desktop/scripts/encode-sounds.sh [SOURCE_DIR]
#
# Files in the table below are re-encoded; anything else under the sounds dir
# is left untouched. To add a new sound, drop the original in SOURCE_DIR, add a
# row to the table, and re-run.

SOURCE_DIR="${1:-/tmp/breakly-sound-sources}"
DEST_DIR="$(cd "$(dirname "$0")/.." && pwd)/src/assets/sounds"

if ! command -v ffmpeg >/dev/null; then
  echo "ffmpeg not found" >&2
  exit 1
fi

# id | source-ext | output-filename | channels | bitrate | extra-input-args (optional)
#
# tone   = 96 kbps mono  (chimes, bells, bowls, short noise loops)
# stereo = 96 kbps stereo (ambient, music, long resonant tones)
# extra-input-args is appended before -i to trim or seek (e.g. "-ss 60 -t 30")
ROWS=(
  "398496|wav|398496-wind-chimes-single-04.mp3|1|96k|"
  "445633|wav|445633-traditional-asian-percussion-01.mp3|1|96k|"
  "587417|wav|587417-7.mp3|1|96k|"
  "204915|wav|204915-singing-bowl-a-tuned.mp3|1|96k|"
  "573805|wav|573805-singing-bowl-long-without-reverb.mp3|2|96k|"
  "406895|wav|406895-rain-and-chimes-outside.mp3|2|96k|"
  "725602|wav|725602-a-rainy-day-in-town-with-birds.mp3|2|96k|"
  "852048|wav|852048-rain-rain-garden-veranda-on-different-surfaces-thunder-rolls.mp3|2|96k|"
  "852474|flac|852474-far-people-traffic-wind-suburban-s-hertogenbosch-netherlands.mp3|2|96k|"
  "349314|wav|349314-brown-noise-10s.mp3|1|64k|"
  "808969|wav|808969-relaxation-music-69.mp3|2|128k|"
  "337048|mp3|337048-131348-kaonaya-bell-at-daitokuji-temple-kyoto-modified.mp3|1|96k|"
  "579290|mp3|579290-wineglass3.mp3|1|96k|"
  "180732|wav|180732-location-stream-1.mp3|2|96k|"
  "578524|flac|578524-calm-ocean-waves.mp3|2|96k|"
  "832628|wav|832628-calm-ambient-piano-loop.mp3|2|96k|"
  "837176|wav|837176-lighting-incense-burner-charcoal.mp3|2|96k|"
  "851196|wav|851196-the-awakening-forest-early-morning-birds-symphony.mp3|2|96k|"
  "403326|mp3|403326-ultra-soft-noise-loop-30s.mp3|2|64k|-ss 60 -t 30"
)

for row in "${ROWS[@]}"; do
  IFS='|' read -r id ext out channels bitrate extra <<<"$row"
  src="$SOURCE_DIR/${id}-original.${ext}"
  dst="$DEST_DIR/${out}"
  if [[ ! -f "$src" ]]; then
    echo "SKIP $id (source missing: $src)"
    continue
  fi
  # shellcheck disable=SC2086
  ffmpeg -hide_banner -loglevel error -y $extra -i "$src" \
    -ac "$channels" -ar 44100 -codec:a libmp3lame -b:a "$bitrate" \
    "$dst"
  size=$(du -h "$dst" | awk '{print $1}')
  echo "OK   $id  ${channels}ch  ${bitrate}  ${size}  ${out}"
done
