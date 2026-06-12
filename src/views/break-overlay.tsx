import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import {
  announceBreak,
  breakDescription,
  dialogLabel,
  remainingAriaLabel,
} from "../lib/a11y";
import { soundById } from "../lib/sounds";
import { useCustomStylesheet } from "../lib/use-custom-stylesheet";
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
import { useMilestoneAnnouncer } from "./break-overlay/hooks/use-milestone-announcer";
import { useMountFocus } from "./break-overlay/hooks/use-mount-focus";
import { derivePostpone } from "./break-overlay/postpone";
import { routineProgress } from "./break-overlay/routine";
import {
  breathAriaLabel,
  breathLabel,
  breathProgress,
} from "./break-overlay/breath";
import {
  ENFORCEABLE_LONG_BREAK_HINT,
  shouldShowEnforceableHint,
} from "./break-overlay/skip-hint";
import { breakSoundFor, labelFor } from "./break-overlay/types";
import {
  RING_RADIUS,
  clamp01,
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
  useCustomStylesheet(appearance.custom_css);

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

  const highContrast =
    appearance.overlay_high_contrast || systemPrefersContrast();
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
  const milestone = useMilestoneAnnouncer(
    active?.kind ?? null,
    active?.duration_secs ?? 0,
    remaining,
    finished,
  );

  if (!active) return null;

  const minutes = Math.floor(remaining / 60);
  const seconds = remaining % 60;
  const label = labelFor(active.kind);
  const hintText = active.hints[hintIndex] ?? "";
  // A selected guided routine steps through its own text on the break's
  // own countdown and takes the place of the rotating hint. With none
  // selected (empty steps) the overlay falls back to the hint above.
  const routineSteps = active.routine_steps ?? [];
  // Effective pacing: the routine's own declared pacing takes precedence;
  // fall back to the global `routine_fill` toggle when absent.
  const effectivePacing =
    active.routine_pacing ??
    (appearance.routine_fill ? ("fill" as const) : undefined);
  const routine = routineProgress(
    routineSteps,
    active.duration_secs - remaining,
    effectivePacing !== undefined
      ? {
          fillToSecs: active.duration_secs,
          pacing: effectivePacing,
          maxStepSecs: active.routine_max_step_secs,
        }
      : undefined,
  );
  const routineText = routine ? routineSteps[routine.index].text : "";
  // A plugin-supplied image for the current step, if any. The backend sends an
  // absolute path; convertFileSrc turns it into an `asset:` URL the webview can
  // load. A broken/missing file simply hides (see onError) — never blocks the
  // break.
  const routineAsset = routine ? routineSteps[routine.index].asset : undefined;
  const routineImageSrc = routineAsset
    ? convertFileSrc(routineAsset)
    : undefined;
  // A breathing routine replaces step text with phase labels and pulses the
  // ring (driven by --breath-scale in use-overlay-css-vars). Takes precedence
  // over step rendering when present.
  const breath = active.routine_breath ?? null;
  const breathProg = breath
    ? breathProgress(breath, active.duration_secs - remaining)
    : null;
  const intensity = clamp01(active.health_intensity);
  const dismissable = !active.enforceable && active.skip_available;
  const showPostpone = active.postpone_available && !finished;
  const showSkip = dismissable && !finished;
  const showEnforceableHint = shouldShowEnforceableHint({
    kind: active.kind,
    enforceable: active.enforceable,
    postpone_available: active.postpone_available,
    finished,
  });
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
      {/* Strict mode only: there is no dialog to carry context, so this
          assertive live region is the sole start announcement. In
          non-strict mode the dialog's aria-describedby speaks the same
          information once, on focus — no second utterance, so the start
          stays calm rather than repeating itself. */}
      {strictMode && (
        <div
          className="sr-only"
          role="alert"
          aria-live="assertive"
          data-testid="overlay-announcement"
        >
          {announcement}
        </div>
      )}
      {/* Separate polite live region for milestone progress (halfway,
          1 minute left, 10 seconds left, end). Always polite even in
          strict mode — users have opted in to being interrupted on
          start, but per-second progress chatter would be hostile. */}
      <div
        className="sr-only"
        role="status"
        aria-live="polite"
        aria-atomic="true"
        data-testid="overlay-milestone"
      >
        {milestone}
      </div>
      {/* Described-by target: read once when the dialog is focused —
          the duration, then the wellness tip. Content can change (hint
          rotation) without re-announcing, because aria-describedby is
          not a live region. */}
      <div id="overlay-detail" className="sr-only">
        {breakDescription(
          active.duration_secs,
          routine
            ? routineText
            : appearance.show_hint && hintText
              ? hintText
              : "",
        )}
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
            {breathProg && (
              <circle
                className="overlay-breath-circle"
                cx="140"
                cy="140"
                r={RING_RADIUS - 22}
                aria-hidden="true"
              />
            )}
          </svg>
          <p className="overlay-timer" aria-label={timerLabel} aria-live="off">
            {finished
              ? "Done"
              : minutes > 0
                ? `${minutes}:${seconds.toString().padStart(2, "0")}`
                : `${seconds}s`}
          </p>
        </div>
        {breathProg ? (
          <div className="overlay-routine overlay-breath">
            <p
              className="overlay-hint overlay-routine-step overlay-breath-phase"
              role="note"
              aria-live="polite"
              aria-atomic="true"
              // eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex
              tabIndex={0}
              aria-label={breathAriaLabel(breathProg)}
            >
              {breathLabel(breathProg)}
            </p>
          </div>
        ) : routine ? (
          <div className="overlay-routine">
            {routineImageSrc && (
              <img
                className="overlay-routine-image"
                src={routineImageSrc}
                alt=""
                // The step text below already conveys the instruction to
                // assistive tech; the image is decorative reinforcement.
                aria-hidden="true"
                // A missing or unreadable sidecar must never break the
                // routine — drop the element and keep the text-only step.
                onError={(e) => {
                  e.currentTarget.style.display = "none";
                }}
              />
            )}
            <p
              className="overlay-hint overlay-routine-step"
              role="note"
              // Polite live region: each guided step is announced as it
              // becomes current, which is the whole point of a routine.
              aria-live="polite"
              aria-atomic="true"
              // Deliberately focusable: the overlay traps focus, so keyboard
              // and screen-reader users can only reach the step text if it
              // sits in the tab order.
              // eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex
              tabIndex={0}
              aria-label={`Step ${routine.index + 1} of ${routine.total}: ${routineText}`}
            >
              {routineText}
            </p>
            <p className="overlay-routine-progress" aria-hidden="true">
              Step {routine.index + 1} of {routine.total}
              {routine.stepRemaining > 0 ? ` · ${routine.stepRemaining}s` : ""}
            </p>
          </div>
        ) : (
          appearance.show_hint &&
          hintText && (
            <p
              className="overlay-hint"
              role="note"
              // Deliberately focusable: the overlay traps focus, so keyboard
              // and screen-reader users can only reach the wellness tip if
              // it sits in the tab order.
              // eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex
              tabIndex={0}
              aria-label={`Wellness tip: ${hintText}`}
            >
              {hintText}
            </p>
          )
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
        {showEnforceableHint && (
          <p
            className="overlay-enforceable-hint"
            role="note"
            data-testid="overlay-enforceable-hint"
          >
            {ENFORCEABLE_LONG_BREAK_HINT}
          </p>
        )}
      </div>
      {creditSound && <SoundCredit sound={creditSound} />}
    </div>
  );
}
