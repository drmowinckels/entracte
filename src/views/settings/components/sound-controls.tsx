import { useEffect, useRef } from "react";
import {
  playSound,
  previewAmbient,
  soundDisplayName,
  soundsForMode,
  type AmbientHandle,
} from "../../../lib/sounds";
import { SOUND_MODES } from "../constants";
import type { BreakSound, BreakSoundMode } from "../types";
import { InfoTip } from "./info-tip";

function defaultSoundIdFor(mode: BreakSoundMode): string {
  if (mode === "off") return "";
  return soundsForMode(mode)[0]?.id ?? "";
}

export type SoundControlsProps = {
  sound: BreakSound;
  volume: number;
  onChange: (next: BreakSound) => void;
};

export function SoundControls({ sound, volume, onChange }: SoundControlsProps) {
  const previewRef = useRef<AmbientHandle | null>(null);
  useEffect(() => () => previewRef.current?.stop(), []);

  const stopPreview = () => {
    previewRef.current?.stop();
    previewRef.current = null;
  };

  const onModeChange = (mode: BreakSoundMode) => {
    stopPreview();
    if (mode === "off") {
      onChange({ mode, sound_id: "" });
      return;
    }
    const stillValid = soundsForMode(mode).some((s) => s.id === sound.sound_id);
    onChange({
      mode,
      sound_id: stillValid ? sound.sound_id : defaultSoundIdFor(mode),
    });
  };

  const onSoundChange = (id: string) => {
    stopPreview();
    onChange({ ...sound, sound_id: id });
  };

  const onPreview = () => {
    stopPreview();
    if (sound.mode === "off" || !sound.sound_id || volume <= 0) return;
    if (sound.mode === "end_chime") {
      playSound(sound.sound_id, volume);
      return;
    }
    previewRef.current = previewAmbient(sound.sound_id, volume);
  };

  const previewDisabled = sound.mode === "off" || !sound.sound_id || volume <= 0;
  const options = sound.mode === "off" ? [] : soundsForMode(sound.mode);

  return (
    <>
      <label className="row">
        <span>
          Sound
          <InfoTip text="End chime plays once when the break finishes. Ambient loops throughout the break and stops when it ends." />
        </span>
        <select
          value={sound.mode}
          onChange={(e) => onModeChange(e.target.value as BreakSoundMode)}
        >
          {SOUND_MODES.map((m) => (
            <option key={m.id} value={m.id}>
              {m.label}
            </option>
          ))}
        </select>
      </label>
      {sound.mode !== "off" && (
        <label className="row">
          <span>Track</span>
          <select value={sound.sound_id} onChange={(e) => onSoundChange(e.target.value)}>
            {options.map((s) => (
              <option key={s.id} value={s.id}>
                {soundDisplayName(s)}
              </option>
            ))}
          </select>
        </label>
      )}
      {sound.mode !== "off" && (
        <div className="actions inline">
          <button className="secondary" onClick={onPreview} disabled={previewDisabled}>
            Preview
          </button>
        </div>
      )}
    </>
  );
}
