import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  hasToken,
  suggestionsForPlatform,
  tokenFor,
} from "../../../lib/app-suggestions";
import { usePlatform } from "../../../lib/platform";
import { formatRemaining } from "../../../lib/time";
import { CheckboxRow } from "../components/rows";
import type { UseSettings } from "../hooks/use-settings";
import type { PauseInfo, SchedulerSettings } from "../types";
import { linesToList, listToLines } from "../utils";

export function QuietTab({
  settings,
  update,
  pauseInfo,
}: {
  settings: SchedulerSettings;
  update: UseSettings["update"];
  pauseInfo: PauseInfo;
}) {
  const platform = usePlatform();
  const [appPauseText, setAppPauseText] = useState(
    listToLines(settings.app_pause_list),
  );

  // Re-seed if the active profile switched and replaced the list.
  useEffect(() => {
    setAppPauseText(listToLines(settings.app_pause_list));
  }, [settings.app_pause_list]);

  return (
    <>
      <h2>Auto-pause</h2>
      <section>
        <p className="placeholder">
          Breaks are automatically suppressed while these conditions apply.
        </p>
        <CheckboxRow
          label="Do Not Disturb is on"
          value={settings.pause_during_dnd}
          onChange={(v) => update("pause_during_dnd", v)}
          onlyOn={["macos", "windows"]}
          tip="Reads your OS-level DnD / Focus state. When on, scheduled breaks are suppressed until DnD turns off."
        />
        <CheckboxRow
          label="Camera is in use"
          value={settings.pause_during_camera}
          onChange={(v) => update("pause_during_camera", v)}
          tip="Suppresses breaks while another app is using your webcam — keeps video meetings uninterrupted."
        />
        <CheckboxRow
          label="Fullscreen video is playing"
          value={settings.pause_during_video}
          onChange={(v) => update("pause_during_video", v)}
          tip="Suppresses breaks while a fullscreen video is detected."
        />
      </section>

      <h2>Pause for specific apps</h2>
      <section>
        <CheckboxRow
          label="Pause when any of these apps are running"
          value={settings.app_pause_enabled}
          onChange={(v) => update("app_pause_enabled", v)}
          tip="Matches partial app names case-insensitively. Whenever any listed app is running, breaks are suppressed."
        />
        {settings.app_pause_enabled && (
          <>
            <label className="row stacked">
              <span>
                One app name per line — partial, case-insensitive match (e.g.
                zoom, obs, keynote)
              </span>
              <textarea
                className="textarea"
                rows={4}
                value={appPauseText}
                onChange={(e) => setAppPauseText(e.target.value)}
                onBlur={() =>
                  update("app_pause_list", linesToList(appPauseText))
                }
              />
            </label>
            <div className="row stacked">
              <span className="hint-label">Quick add</span>
              <div className="app-suggestion-chips">
                {suggestionsForPlatform(platform).map((s) => {
                  const token = tokenFor(s, platform)!;
                  const present = hasToken(linesToList(appPauseText), token);
                  return (
                    <button
                      type="button"
                      key={s.label}
                      className="app-suggestion-chip"
                      disabled={present}
                      onClick={() => {
                        const list = linesToList(appPauseText);
                        if (hasToken(list, token)) return;
                        const next = [...list, token];
                        setAppPauseText(listToLines(next));
                        update("app_pause_list", next);
                      }}
                    >
                      {present ? "" : "+ "}
                      {s.label}
                    </button>
                  );
                })}
              </div>
            </div>
          </>
        )}
      </section>

      <h2>Manual pause</h2>
      <section>
        {pauseInfo.paused ? (
          <div className="pause-control">
            <button
              className="secondary"
              onClick={async () => {
                await invoke("resume");
              }}
            >
              Resume
            </button>
            <span className="pause-status">
              {pauseInfo.remaining_secs != null
                ? `Paused — ${formatRemaining(pauseInfo.remaining_secs)} left`
                : "Paused indefinitely"}
            </span>
          </div>
        ) : (
          <p className="placeholder">
            Pause from the menu bar icon — choose a duration there.
          </p>
        )}
      </section>
    </>
  );
}
