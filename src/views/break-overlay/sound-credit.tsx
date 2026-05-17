import { openUrl } from "@tauri-apps/plugin-opener";
import type { Sound } from "../../lib/sounds";

export function SoundCredit({ sound }: { sound: Sound }) {
  return (
    <p className="overlay-credit">
      <span aria-hidden="true">♪ </span>
      <a
        href={sound.source_url}
        onClick={(e) => {
          e.preventDefault();
          openUrl(sound.source_url).catch(() => {});
        }}
      >
        {sound.title}
      </a>
      {" — "}
      {sound.author} · {sound.license_short}
    </p>
  );
}
