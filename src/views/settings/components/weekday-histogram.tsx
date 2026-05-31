import { weekdayLabel } from "../../../lib/stats-format";
import type { WeekdayBucket } from "../types";

export function WeekdayHistogram({ days }: { days: WeekdayBucket[] }) {
  const max = Math.max(1, ...days.flatMap((d) => [d.taken, d.dismissed]));
  return (
    <div
      className="weekday-histogram"
      role="img"
      aria-label="Breaks taken vs dismissed by weekday"
    >
      {days.map((d) => {
        const takenPct = (d.taken / max) * 100;
        const dismissedPct = (d.dismissed / max) * 100;
        const label = weekdayLabel(d.weekday);
        return (
          <div key={d.weekday} className="weekday-col">
            <div className="weekday-bar-pair">
              <div
                className="weekday-bar weekday-bar-taken"
                ref={(el) => {
                  el?.style.setProperty("--weekday-bar-height", `${takenPct}%`);
                }}
                title={`${label}: ${d.taken} taken`}
              />
              <div
                className="weekday-bar weekday-bar-dismissed"
                ref={(el) => {
                  el?.style.setProperty(
                    "--weekday-bar-height",
                    `${dismissedPct}%`,
                  );
                }}
                title={`${label}: ${d.dismissed} dismissed`}
              />
            </div>
            <span className="weekday-label">{label}</span>
          </div>
        );
      })}
    </div>
  );
}
