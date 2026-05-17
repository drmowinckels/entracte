import { invoke } from "@tauri-apps/api/core";
import { announceBreak, dialogLabel, remainingAriaLabel } from "../lib/a11y";
import { soundById } from "../lib/sounds";
import { SoundCredit } from "./break-overlay/sound-credit";
import { useBreakState } from "./break-overlay/hooks/use-break-state";
import { useAmbientSound } from "./break-overlay/hooks/use-ambient-sound";
import { useTypingPause } from "./break-overlay/hooks/use-typing-pause";
import { useHintRotation } from "./break-overlay/hooks/use-hint-rotation";
import { useEscapeToDismiss } from "./break-overlay/hooks/use-escape-to-dismiss";
import { useCountdown } from "./break-overlay/hooks/use-countdown";
import { useClock } from "./break-overlay/hooks/use-clock";
import { useOverlayCssVars } from "./break-overlay/hooks/use-overlay-css-vars";
import { useFocusTrap } from "./break-overlay/hooks/use-focus-trap";
import { useMountFocus } from "./break-overlay/hooks/use-mount-focus";
import { derivePostpone } from "./break-overlay/postpone";
import { breakSoundFor, labelFor } from "./break-overlay/types";
import {
  RING_RADIUS,
  systemPrefersContrast,
  systemPrefersReducedTransparency,
} from "./break-overlay/visual";
import "./break-overlay.css";

/** The break-time overlay window. Subscribes to `break:start` /
 * `break:end`, runs the per-second countdown, draws the progress
 * ring + hint, plays the configured chime / ambient sound, and
 * exposes the postpone / skip / finish buttons. Multiple instances
 * may run simultaneously when `monitor_placement = "all"`. */
export default function BreakOverlay() {
  const {
    active,
    remaining,
    finished,
    hintIndex,
    appearance,
    resolvedTheme,
    postponeState,
    setRemaining,
    setHintIndex,
    setFinished,
    clearBreak,
  } = useBreakState();

  const paused = useTypingPause(active, appearance.pause_countdown_if_typing);
  const clock = useClock(
    Boolean(active) && appearance.show_current_time,
    1000,
    appearance.clock_format,
  );

  useAmbientSound(active, appearance);
  useHintRotation(active, setHintIndex);

  const { triggerFinish } = useCountdown(
    active,
    remaining,
    paused,
    appearance,
    setRemaining,
    setFinished,
    clearBreak,
  );

  const onSkip = () => {
    invoke("end_break", { reason: "dismissed" });
    clearBreak();
  };

  useEscapeToDismiss(active, onSkip);

  const highContrast = appearance.overlay_high_contrast || systemPrefersContrast();
  const opaque = highContrast || systemPrefersReducedTransparency();
  const { rootRef, ringBarRef } = useOverlayCssVars(
    active,
    remaining,
    appearance,
    resolvedTheme,
    highContrast,
    opaque,
  );

  const strictMode = appearance.strict_mode;
  const dialogSemantics = !strictMode;
  useMountFocus(rootRef, Boolean(active), dialogSemantics);
  useFocusTrap(rootRef, dialogSemantics && Boolean(active));

  if (!active) return null;

  const minutes = Math.floor(remaining / 60);
  const seconds = remaining % 60;
  const label = labelFor(active.kind);
  const hintText = active.hints[hintIndex] ?? "";
  const intensity = Math.max(0, Math.min(1, active.health_intensity));
  const dismissable = !active.enforceable;
  const showPostpone = active.postpone_available && !finished;
  const showSkip = dismissable && !finished;
  const showFinishButton = finished && active.manual_finish;
  const activeSoundCfg = breakSoundFor(active.kind, appearance);
  const creditSound =
    activeSoundCfg && activeSoundCfg.mode !== "off" && activeSoundCfg.sound_id
      ? soundById(activeSoundCfg.sound_id)
      : undefined;
  const postpone = derivePostpone(postponeState);

  const onPostpone = () => {
    if (postpone.exhausted) return;
    invoke("postpone_break", { kind: active.kind });
  };

  const timerLabel = finished ? "Time's up" : remainingAriaLabel(remaining);
  const announcement = announceBreak(active.kind, active.duration_secs);

  return (
    <div
      ref={rootRef}
      className={`overlay-root${highContrast ? " high-contrast" : ""}`}
      tabIndex={dialogSemantics ? -1 : undefined}
      role={dialogSemantics ? "dialog" : undefined}
      aria-modal={dialogSemantics ? true : undefined}
      aria-label={dialogSemantics ? dialogLabel(active.kind) : undefined}
      aria-describedby={dialogSemantics ? "overlay-detail" : undefined}
    >
      {/* Live region: announces only the initial "break started"
          message. The rotating hint deliberately lives outside this
          element — putting it inside means every rotation triggers
          a re-announcement, which is unusable with a screen reader. */}
      <div
        className="sr-only"
        role={strictMode ? "alert" : "status"}
        aria-live={strictMode ? "assertive" : "polite"}
      >
        {announcement}
      </div>
      {/* Described-by target: read once when the dialog is focused.
          Content can change (hint rotation) without re-announcing,
          because aria-describedby is not a live region. */}
      <div id="overlay-detail" className="sr-only">
        {appearance.show_hint && hintText ? hintText : ""}
      </div>
      {intensity > 0 && <div className="overlay-vignette" aria-hidden="true" />}
      {appearance.show_current_time && (
        <div className="overlay-clock" aria-hidden="true">
          {clock}
        </div>
      )}
      <div className="overlay-card">
        <p className="overlay-kind" id="overlay-kind">
          {label}
        </p>
        <div className="overlay-progress">
          <svg
            className="overlay-progress-svg"
            viewBox="0 0 280 280"
            aria-hidden="true"
          >
            <circle
              className="overlay-progress-track"
              cx="140"
              cy="140"
              r={RING_RADIUS}
            />
            <circle
              ref={ringBarRef}
              className="overlay-progress-bar"
              cx="140"
              cy="140"
              r={RING_RADIUS}
            />
          </svg>
          <p className="overlay-timer" aria-label={timerLabel} aria-live="off">
            {finished
              ? "Done"
              : minutes > 0
                ? `${minutes}:${seconds.toString().padStart(2, "0")}`
                : `${seconds}s`}
          </p>
        </div>
        {appearance.show_hint && hintText && (
          <p className="overlay-hint">{hintText}</p>
        )}
        {paused && !finished && (
          <p className="overlay-paused">
            Paused — break resumes when you stop typing
          </p>
        )}
        <div className="overlay-actions">
          {showFinishButton && (
            <button
              className="overlay-button primary"
              onClick={triggerFinish}
              aria-label="End break"
            >
              I'm back
            </button>
          )}
          {showPostpone && (
            <button
              className="overlay-button"
              onClick={onPostpone}
              disabled={postpone.exhausted}
              aria-label="Postpone break"
            >
              {postpone.label}
            </button>
          )}
          {showSkip && (
            <button
              className="overlay-button ghost"
              onClick={onSkip}
              aria-label="Skip break"
            >
              Skip
            </button>
          )}
        </div>
        {showPostpone && postpone.exhausted && (
          <p className="overlay-dismiss">
            Postpone exhausted — take this break
          </p>
        )}
      </div>
      {creditSound && <SoundCredit sound={creditSound} />}
    </div>
  );
}
