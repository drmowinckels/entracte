export function HourHistogram({ values }: { values: number[] }) {
  const max = Math.max(1, ...values);
  return (
    <div className="hour-histogram">
      {values.map((v, h) => {
        const pct = (v / max) * 100;
        return (
          <div key={h} className="hour-bar-wrap">
            <div
              className="hour-bar"
              ref={(el) => {
                el?.style.setProperty("--hour-bar-height", `${pct}%`);
              }}
              title={`${h}:00 — ${v} break${v === 1 ? "" : "s"}`}
            />
            {h % 6 === 0 && <span className="hour-label">{h}</span>}
          </div>
        );
      })}
    </div>
  );
}
