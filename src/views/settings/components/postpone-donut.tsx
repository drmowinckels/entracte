import type { PostponeFollowThrough } from "../types";

const RADIUS = 42;
const STROKE = 14;
const SIZE = 120;
const CIRC = 2 * Math.PI * RADIUS;

type Segment = {
  key: "taken" | "dismissed" | "skipped" | "unresolved";
  label: string;
  value: number;
  cssVar: string;
};

function segmentsFrom(data: PostponeFollowThrough): Segment[] {
  return [
    { key: "taken", label: "Eventually taken", value: data.taken, cssVar: "--primary-color" },
    {
      key: "dismissed",
      label: "Dismissed instead",
      value: data.dismissed,
      cssVar: "--secondary-color",
    },
    { key: "skipped", label: "Skipped instead", value: data.skipped, cssVar: "--accent-color" },
    {
      key: "unresolved",
      label: "Still pending",
      value: data.unresolved,
      cssVar: "--light-primary-color",
    },
  ];
}

export function PostponeDonut({ data }: { data: PostponeFollowThrough }) {
  if (data.total === 0) return null;
  const segments = segmentsFrom(data);
  const arcs: { seg: Segment; len: number; offset: number }[] = [];
  let offset = 0;
  for (const seg of segments) {
    const len = (seg.value / data.total) * CIRC;
    arcs.push({ seg, len, offset });
    offset += len;
  }
  return (
    <div className="donut-wrap">
      <svg
        className="donut"
        viewBox={`0 0 ${SIZE} ${SIZE}`}
        role="img"
        aria-label={`Postpone follow-through: ${data.taken} taken, ${data.dismissed} dismissed, ${data.skipped} skipped, ${data.unresolved} pending`}
      >
        <circle
          cx={SIZE / 2}
          cy={SIZE / 2}
          r={RADIUS}
          fill="none"
          stroke="var(--input-border)"
          strokeWidth={STROKE}
        />
        {arcs.map(({ seg, len, offset: o }) =>
          seg.value === 0 ? null : (
            <circle
              key={seg.key}
              cx={SIZE / 2}
              cy={SIZE / 2}
              r={RADIUS}
              fill="none"
              stroke={`var(${seg.cssVar})`}
              strokeWidth={STROKE}
              strokeDasharray={`${len} ${CIRC - len}`}
              strokeDashoffset={-o}
              transform={`rotate(-90 ${SIZE / 2} ${SIZE / 2})`}
            />
          ),
        )}
        <text
          x={SIZE / 2}
          y={SIZE / 2 - 4}
          textAnchor="middle"
          className="donut-total"
        >
          {data.total}
        </text>
        <text
          x={SIZE / 2}
          y={SIZE / 2 + 12}
          textAnchor="middle"
          className="donut-caption"
        >
          {data.total === 1 ? "postpone" : "postpones"}
        </text>
      </svg>
      <ul className="donut-legend">
        {segments.map((s) => {
          const pct = Math.round((s.value / data.total) * 100);
          return (
            <li key={s.key} data-segment={s.key}>
              <span className="donut-swatch" aria-hidden="true" />
              <span className="donut-legend-label">{s.label}</span>
              <span className="donut-legend-value">
                {s.value}
                <span className="stat-card-sub"> · {pct}%</span>
              </span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
