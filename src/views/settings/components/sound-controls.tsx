import { useEffect, useRef } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  playCustomSound,
  playSound,
  previewAmbient,
  previewCustomAmbient,
  soundDisplayName,
  soundsForMode,
  type AmbientHandle,
} from "../../../lib/sounds";
import { CUSTOM_SOUND_ID } from "../../../lib/break-sound";
import { SOUND_MODES } from "../constants";
import type { BreakSound, BreakSoundMode } from "../types";
import { InfoTip } from "./info-tip";

const PICK_CUSTOM_VALUE = "__pick_custom__";

function defaultSoundIdFor(mode: BreakSoundMode): string {
  if (mode === "off") return "";
  return soundsForMode(mode)[0]?.id ?? "";
}

function basename(path: string): string {
  if (!path) return "";
  const sep = path.includes("\\") ? "\\" : "/";
  return path.split(sep).pop() ?? path;
}

export type SoundControlsProps = {
  sound: BreakSound;
  volume: number;
  onChange: (next: BreakSound) => void;
  isSupporter?: boolean;
};

export function SoundControls({
  sound,
  volume,
  onChange,
  isSupporter = false,
}: SoundControlsProps) {
  const previewRef = useRef<AmbientHandle | null>(null);
  useEffect(() => () => previewRef.current?.stop(), []);

  const stopPreview = () => {
    previewRef.current?.stop();
    previewRef.current = null;
  };

  const isCustom = sound.sound_id === CUSTOM_SOUND_ID;
  const customPath = sound.custom_path ?? "";

  const pickCustomFile = async () => {
    stopPreview();
    try {
      const selected = await openDialog({
        multiple: false,
        directory: false,
        filters: [
          {
            name: "Audio",
            extensions: ["mp3", "wav", "ogg", "m4a", "aac", "flac"],
          },
        ],
      });
      if (typeof selected !== "string") return;
      onChange({
        ...sound,
        mode: sound.mode === "off" ? "end_chime" : sound.mode,
        sound_id: CUSTOM_SOUND_ID,
        custom_path: selected,
      });
    } catch (e) {
      console.error("file picker failed", e);
    }
  };

  const onModeChange = (mode: BreakSoundMode) => {
    stopPreview();
    if (mode === "off") {
      onChange({ ...sound, mode, sound_id: "" });
      return;
    }
    if (isCustom) {
      onChange({ ...sound, mode });
      return;
    }
    const stillValid = soundsForMode(mode).some((s) => s.id === sound.sound_id);
    onChange({
      ...sound,
      mode,
      sound_id: stillValid ? sound.sound_id : defaultSoundIdFor(mode),
    });
  };

  const onSoundChange = (id: string) => {
    if (id === PICK_CUSTOM_VALUE) {
      void pickCustomFile();
      return;
    }
    stopPreview();
    onChange({ ...sound, sound_id: id });
  };

  const useBundled = () => {
    stopPreview();
    onChange({
      ...sound,
      sound_id: defaultSoundIdFor(sound.mode),
      custom_path: "",
    });
  };

  const onPreview = () => {
    stopPreview();
    if (sound.mode === "off" || volume <= 0) return;
    if (isCustom) {
      if (!customPath) return;
      if (sound.mode === "end_chime") {
        playCustomSound(customPath, volume);
        return;
      }
      previewRef.current = previewCustomAmbient(customPath, volume);
      return;
    }
    if (!sound.sound_id) return;
    if (sound.mode === "end_chime") {
      playSound(sound.sound_id, volume);
      return;
    }
    previewRef.current = previewAmbient(sound.sound_id, volume);
  };

  const previewDisabled =
    sound.mode === "off" ||
    volume <= 0 ||
    (isCustom ? !customPath : !sound.sound_id);
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
      {sound.mode !== "off" && !isCustom && (
        <label className="row">
          <span>Track</span>
          <select
            value={sound.sound_id}
            onChange={(e) => onSoundChange(e.target.value)}
          >
            {options.map((s) => (
              <option key={s.id} value={s.id}>
                {soundDisplayName(s)}
              </option>
            ))}
            {isSupporter && (
              <option value={PICK_CUSTOM_VALUE}>Custom file…</option>
            )}
          </select>
        </label>
      )}
      {sound.mode !== "off" && isCustom && (
        <label className="row">
          <span>Track</span>
          <span className="actions inline">
            <span className="placeholder" title={customPath}>
              {customPath ? basename(customPath) : "No file selected"}
            </span>
            {isSupporter && (
              <button className="secondary" onClick={pickCustomFile}>
                {customPath ? "Replace…" : "Choose file…"}
              </button>
            )}
            <button className="secondary" onClick={useBundled}>
              Use bundled
            </button>
          </span>
        </label>
      )}
      {sound.mode !== "off" && (
        <div className="actions inline">
          <button
            className="secondary"
            onClick={onPreview}
            disabled={previewDisabled}
          >
            Preview
          </button>
        </div>
      )}
    </>
  );
}
