import { useEffect, useRef, useState } from "react";
import type { UseSettings } from "../../hooks/use-settings";
import type { SchedulerSettings } from "../../types";
import { CheckboxRow, TimeRow } from "../rows";
import { InfoTip } from "../info-tip";
import "./onboarding.css";

type StepId = "welcome" | "login" | "window" | "hints" | "winddown" | "done";

const STEPS: { id: StepId; title: string }[] = [
  { id: "welcome", title: "Welcome" },
  { id: "login", title: "Start at login" },
  { id: "window", title: "Working hours" },
  { id: "hints", title: "Wellness hints" },
  { id: "winddown", title: "Wind down" },
  { id: "done", title: "All set" },
];

export type OnboardingWizardProps = {
  settings: SchedulerSettings;
  update: UseSettings["update"];
  setAutostart: UseSettings["setAutostart"];
  /** Persist completion and dismiss the wizard (finish or skip). */
  onFinish: () => void;
};

/** First-run guided setup. A modal over the Settings window that walks a
 * new user through the handful of settings most worth choosing up front;
 * every control writes through the same `update`/`setAutostart` helpers
 * the real tabs use, so finishing leaves the app configured. */
export function OnboardingWizard({
  settings,
  update,
  setAutostart,
  onFinish,
}: OnboardingWizardProps) {
  const [index, setIndex] = useState(0);
  const step = STEPS[index];
  const isFirst = index === 0;
  const isLast = index === STEPS.length - 1;
  const dialogRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    dialogRef.current?.focus();
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onFinish();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onFinish]);

  const back = () => setIndex((i) => Math.max(0, i - 1));
  const next = () => {
    if (isLast) onFinish();
    else setIndex((i) => Math.min(STEPS.length - 1, i + 1));
  };

  return (
    <div className="onboarding-backdrop">
      <div
        className="onboarding-card"
        role="dialog"
        aria-modal="true"
        aria-labelledby="onboarding-title"
        tabIndex={-1}
        ref={dialogRef}
      >
        <header className="onboarding-head">
          <p className="onboarding-step-count">
            Step {index + 1} of {STEPS.length}
          </p>
          <ol className="onboarding-dots" aria-hidden="true">
            {STEPS.map((s, i) => (
              <li
                key={s.id}
                className={`onboarding-dot${i === index ? " current" : ""}${
                  i < index ? " done" : ""
                }`}
              />
            ))}
          </ol>
        </header>

        <div className="onboarding-body">
          <StepContent
            step={step.id}
            settings={settings}
            update={update}
            setAutostart={setAutostart}
          />
        </div>

        <footer className="onboarding-foot">
          <button type="button" className="link" onClick={onFinish}>
            {isLast ? "Close" : "Skip setup"}
          </button>
          <div className="onboarding-nav">
            {!isFirst && (
              <button type="button" className="secondary" onClick={back}>
                Back
              </button>
            )}
            <button type="button" onClick={next}>
              {isLast ? "Finish" : "Next"}
            </button>
          </div>
        </footer>
      </div>
    </div>
  );
}

function StepContent({
  step,
  settings,
  update,
  setAutostart,
}: {
  step: StepId;
  settings: SchedulerSettings;
  update: UseSettings["update"];
  setAutostart: UseSettings["setAutostart"];
}) {
  switch (step) {
    case "welcome":
      return (
        <>
          <h2 id="onboarding-title">Welcome to Entracte</h2>
          <p>
            Entracte nudges you to step away from the screen with short micro
            breaks and longer rest breaks. Let’s tune a few things so the
            reminders fit how you work — it takes under a minute, and you can
            change everything later in Settings.
          </p>
        </>
      );
    case "login":
      return (
        <>
          <h2 id="onboarding-title">Start Entracte at login</h2>
          <p>
            Break reminders only work while Entracte is running. Starting it
            automatically means you don’t have to remember to launch it each
            day.
          </p>
          <CheckboxRow
            label="Start Entracte when I log in"
            value={settings.autostart_enabled}
            onChange={(v) => setAutostart(v)}
          />
        </>
      );
    case "window":
      return (
        <>
          <h2 id="onboarding-title">When should breaks fire?</h2>
          <p>
            Limit reminders to your working hours so Entracte stays quiet
            evenings and weekends. Leave this off to be reminded around the
            clock.
          </p>
          <CheckboxRow
            label="Only remind me during working hours"
            value={settings.work_window_enabled}
            onChange={(v) => update("work_window_enabled", v)}
          />
          {settings.work_window_enabled && (
            <>
              <TimeRow
                label="Start of day"
                value={settings.work_start_minutes}
                format={settings.clock_format}
                onChange={(v) => update("work_start_minutes", v)}
              />
              <TimeRow
                label="End of day"
                value={settings.work_end_minutes}
                format={settings.clock_format}
                onChange={(v) => update("work_end_minutes", v)}
              />
            </>
          )}
        </>
      );
    case "hints":
      return (
        <>
          <h2 id="onboarding-title">Wellness hints</h2>
          <p>
            Each break can show a suggestion — a stretch, a breath, a moment
            away from the desk. Long-break ideas come in two flavours.
          </p>
          <CheckboxRow
            label="Show a wellness hint during breaks"
            value={settings.show_hint}
            onChange={(v) => update("show_hint", v)}
          />
          {settings.show_hint && (
            <label className="row">
              <span>
                Long-break suggestions
                <InfoTip text="Solo: things to do on your own (stretch, fresh air, snack). Social: things to do with someone (call, walk together). Working alone? Pick Solo only to drop the social prompts." />
              </span>
              <select
                value={settings.long_hint_mix}
                onChange={(e) =>
                  update(
                    "long_hint_mix",
                    e.target.value as SchedulerSettings["long_hint_mix"],
                  )
                }
              >
                <option value="both">Mix of solo and social</option>
                <option value="solo">Solo only — I work alone</option>
                <option value="social">Social only</option>
              </select>
            </label>
          )}
        </>
      );
    case "winddown":
      return (
        <>
          <h2 id="onboarding-title">Wind down &amp; focus</h2>
          <p>
            Bedtime prompts gently remind you to log off as the evening winds
            down. Strict mode removes the skip and postpone buttons so breaks
            always happen — handy if you tend to dismiss them.
          </p>
          <CheckboxRow
            label="Remind me to wind down before bed"
            value={settings.bedtime_enabled}
            onChange={(v) => update("bedtime_enabled", v)}
          />
          {settings.bedtime_enabled && (
            <>
              <TimeRow
                label="Wind-down starts"
                value={settings.bedtime_start_minutes}
                format={settings.clock_format}
                onChange={(v) => update("bedtime_start_minutes", v)}
              />
              <TimeRow
                label="Wind-down ends"
                value={settings.bedtime_end_minutes}
                format={settings.clock_format}
                onChange={(v) => update("bedtime_end_minutes", v)}
              />
            </>
          )}
          <CheckboxRow
            label="Strict mode (breaks can’t be skipped or postponed)"
            value={settings.strict_mode}
            onChange={(v) => update("strict_mode", v)}
            tip="Disables every escape hatch on the overlay. You can turn this off any time on the Breaks tab."
          />
        </>
      );
    case "done":
      return (
        <>
          <h2 id="onboarding-title">You’re all set</h2>
          <p>
            That’s the essentials. Everything you picked — plus break intervals,
            sounds, overlay appearance, profiles and more — lives in Settings,
            ready whenever you want to fine-tune it.
          </p>
        </>
      );
  }
}
