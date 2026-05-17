import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  dismissalRate,
  formatHoursMinutes,
} from "../../../lib/stats-format";
import { localDateString } from "../../../lib/time";
import { HourHistogram } from "../components/hour-histogram";
import { Heatmap } from "../components/heatmap";
import type { UseStats } from "../hooks/use-stats";
import type { StatsRange } from "../types";
import { downloadCsv } from "../utils";

export function InsightsTab({ stats }: { stats: UseStats }) {
  // Destructure to stable callback references — `useStats` returns a fresh
  // object every parent render, so `[range, stats]` as effect deps used to
  // fire on every render and re-trigger `refreshDigest` indefinitely.
  const { stats: session, digest, digestLoading, reset, refreshDigest } = stats;
  const [range, setRange] = useState<StatsRange>("week");

  useEffect(() => {
    refreshDigest(range);
  }, [range, refreshDigest]);

  const intensity = useMemo(() => {
    const total = session.taken + session.skipped;
    if (total === 0) return 0;
    return Math.round((session.skipped / total) * 100);
  }, [session.taken, session.skipped]);

  const onExportCsv = async () => {
    try {
      const csv = await invoke<string>("export_stats_csv");
      downloadCsv(`entracte-stats-${localDateString()}.csv`, csv);
    } catch (e) {
      console.error("export failed", e);
    }
  };

  const onClearLog = async () => {
    if (!confirm("Clear all break history? This cannot be undone.")) return;
    try {
      await invoke("clear_event_log");
      await refreshDigest(range);
    } catch (e) {
      console.error("clear failed", e);
    }
  };

  return (
    <>
      <h2>This session</h2>
      <section>
        <p className="placeholder">
          Live counters since this run started. They reset every time Entracte
          restarts.
        </p>
        <div className="stats-grid">
          <div className="stat">
            <span className="stat-value">{session.taken}</span>
            <span className="stat-label">Taken</span>
          </div>
          <div className="stat">
            <span className="stat-value">{session.skipped}</span>
            <span className="stat-label">Skipped</span>
          </div>
          <div className="stat">
            <span className="stat-value">{session.postponed}</span>
            <span className="stat-label">Postponed</span>
          </div>
          <div className="stat">
            <span className="stat-value">{intensity}%</span>
            <span className="stat-label">Skip rate</span>
          </div>
        </div>
        <div className="actions inline">
          <button className="secondary" onClick={reset}>
            Reset session counters
          </button>
        </div>
      </section>

      <h2>Range</h2>
      <section>
        <div className="range-toggle">
          <button
            className={range === "week" ? "active" : "secondary"}
            onClick={() => setRange("week")}
          >
            Past week
          </button>
          <button
            className={range === "month" ? "active" : "secondary"}
            onClick={() => setRange("month")}
          >
            Past month
          </button>
        </div>
      </section>

      {!digest || digestLoading ? (
        <p className="placeholder">Loading stats…</p>
      ) : (
        <>
          <h2>Summary</h2>
          <section>
            <div className="stat-grid">
              <div className="stat-card">
                <span className="stat-card-value">
                  {digest.micro_taken + digest.long_taken}
                </span>
                <span className="stat-card-label">Breaks taken</span>
                <span className="stat-card-sub">
                  {digest.micro_taken} micro, {digest.long_taken} long
                </span>
              </div>
              <div className="stat-card">
                <span className="stat-card-value">
                  {dismissalRate(
                    digest.micro_taken + digest.long_taken,
                    digest.micro_dismissed + digest.long_dismissed,
                  )}
                </span>
                <span className="stat-card-label">Dismissal rate</span>
                <span className="stat-card-sub">
                  {digest.micro_dismissed + digest.long_dismissed} dismissed,{" "}
                  {digest.postponed_total} postponed
                </span>
              </div>
              <div className="stat-card">
                <span className="stat-card-value">
                  {formatHoursMinutes(digest.pause_total_secs)}
                </span>
                <span className="stat-card-label">Time paused</span>
                <span className="stat-card-sub">
                  {digest.pause_count} pause{digest.pause_count === 1 ? "" : "s"}
                </span>
              </div>
              <div className="stat-card">
                <span className="stat-card-value">
                  {digest.suppressions[0]?.count ?? 0}
                </span>
                <span className="stat-card-label">Top suppression</span>
                <span className="stat-card-sub">
                  {digest.suppressions[0]?.label ?? "None"}
                </span>
              </div>
            </div>
          </section>

          {digest.suppressions.length > 0 && (
            <>
              <h2>Breaks suppressed by</h2>
              <section>
                {digest.suppressions.map((s) => (
                  <div key={s.reason} className="row">
                    <span>{s.label}</span>
                    <span className="stat-card-sub">{s.count}</span>
                  </div>
                ))}
              </section>
            </>
          )}

          <h2>Time of day</h2>
          <section>
            <HourHistogram values={digest.by_hour} />
          </section>

          <h2>Past 12 weeks</h2>
          <section>
            <Heatmap days={digest.by_day} />
          </section>

          <h2>Manage data</h2>
          <section>
            <div className="actions inline">
              <button onClick={onExportCsv}>Export CSV</button>
              <button className="secondary" onClick={onClearLog}>
                Clear history
              </button>
            </div>
          </section>
        </>
      )}
    </>
  );
}
