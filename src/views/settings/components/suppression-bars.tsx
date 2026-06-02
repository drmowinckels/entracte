import {
  KIND_ORDER,
  groupSuppressionsByReason,
} from "../../../lib/stats-format";
import type { SuppressionByKind } from "../types";

const KIND_LABEL: Record<string, string> = {
  micro: "Micro",
  long: "Long",
  sleep: "Sleep",
};

export function SuppressionBars({ rows }: { rows: SuppressionByKind[] }) {
  const grouped = groupSuppressionsByReason(rows);
  if (grouped.length === 0) return null;
  const max = Math.max(1, ...grouped.map((g) => g.total));
  const kindsPresent = Array.from(
    new Set(grouped.flatMap((g) => g.segments.map((s) => s.kind))),
  ).sort((a, b) => KIND_ORDER.indexOf(a) - KIND_ORDER.indexOf(b));
  return (
    <div
      className="suppression-bars"
      role="table"
      aria-label="Suppressions by reason and break kind"
    >
      <div className="suppression-legend" role="presentation">
        {kindsPresent.map((kind) => (
          <span key={kind} className="suppression-legend-item" data-kind={kind}>
            <span className="suppression-swatch" aria-hidden="true" />
            {KIND_LABEL[kind] ?? kind}
          </span>
        ))}
      </div>
      {grouped.map((g) => {
        const widthPct = (g.total / max) * 100;
        return (
          <div key={g.reason} className="suppression-row" role="row">
            <span className="suppression-label" role="rowheader">
              {g.label}
            </span>
            <div
              className="suppression-track"
              role="cell"
              aria-label={`${g.label}: ${g.total} suppressions`}
            >
              <div
                className="suppression-bar"
                ref={(el) => {
                  el?.style.setProperty("--bar-width", `${widthPct}%`);
                }}
              >
                {g.segments.map((s) => {
                  const segPct = (s.count / g.total) * 100;
                  return (
                    <div
                      key={s.kind}
                      className="suppression-seg"
                      data-kind={s.kind}
                      ref={(el) => {
                        el?.style.setProperty("--seg-width", `${segPct}%`);
                      }}
                      title={`${KIND_LABEL[s.kind] ?? s.kind} — ${g.label}: ${s.count}`}
                    />
                  );
                })}
              </div>
            </div>
            <span className="suppression-total" role="cell">
              {g.total}
            </span>
          </div>
        );
      })}
    </div>
  );
}
