import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  ask as askDialog,
  open as openDialog,
  save as saveDialog,
} from "@tauri-apps/plugin-dialog";
import {
  deltaDirection,
  deltaPct,
  dismissalRate,
  formatHoursMinutes,
} from "../../../lib/stats-format";
import { localDateString } from "../../../lib/time";
import { HourHistogram } from "../components/hour-histogram";
import { Heatmap } from "../components/heatmap";
import { PostponeDonut } from "../components/postpone-donut";
import { SuppressionBars } from "../components/suppression-bars";
import { WeekdayHistogram } from "../components/weekday-histogram";
import type { UseStats } from "../hooks/use-stats";
import type { StatsRange } from "../types";
import { downloadCsv } from "../utils";

function DeltaChip({
  curr,
  prev,
  goodDirection = "up",
}: {
  curr: number;
  prev: number;
  goodDirection?: "up" | "down";
}) {
  const dir = deltaDirection(curr, prev);
  const tone = dir === "flat" ? "flat" : dir === goodDirection ? "up" : "down";
  return (
    <span className={`delta-chip ${tone}`} title={`Previous: ${prev}`}>
      {deltaPct(curr, prev)}
    </span>
  );
}

export function InsightsTab({ stats }: { stats: UseStats }) {
  // Destructure to stable callback references — `useStats` returns a fresh
  // object every parent render, so `[range, stats]` as effect deps used to
  // fire on every render and re-trigger `refreshDigest` indefinitely.
  const { stats: session, digest, digestLoading, reset, refreshDigest } = stats;
  const [range, setRange] = useState<StatsRange>("week");
  const [backupStatus, setBackupStatus] = useState<{
    kind: "ok" | "err";
    message: string;
  } | null>(null);

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
    const confirmed = await askDialog(
      "Clear all break history? This cannot be undone.",
      {
        title: "Clear history",
        kind: "warning",
        okLabel: "Clear",
        cancelLabel: "Cancel",
      },
    );
    if (!confirmed) return;
    try {
      await invoke("clear_event_log");
      await refreshDigest(range);
    } catch (e) {
      console.error("clear failed", e);
    }
  };

  const onExportBackup = async () => {
    setBackupStatus(null);
    try {
      const path = await saveDialog({
        defaultPath: `entracte-backup-${localDateString()}.json`,
        filters: [{ name: "Entracte backup", extensions: ["json"] }],
      });
      if (typeof path !== "string" || !path) return;
      await invoke("export_backup_to_path", { path });
      setBackupStatus({ kind: "ok", message: `Backup written to ${path}` });
    } catch (e) {
      console.error("backup export failed", e);
      setBackupStatus({ kind: "err", message: `Backup export failed: ${e}` });
    }
  };

  const onImportBackup = async () => {
    setBackupStatus(null);
    try {
      const path = await openDialog({
        multiple: false,
        directory: false,
        filters: [{ name: "Entracte backup", extensions: ["json"] }],
      });
      if (typeof path !== "string" || !path) return;
      const confirmed = await askDialog(
        "Importing replaces your profiles, settings, break history, pause state, and supporter record on this machine.\n\nContinue?",
        {
          title: "Import backup",
          kind: "warning",
          okLabel: "Replace",
          cancelLabel: "Cancel",
        },
      );
      if (!confirmed) return;
      await invoke("import_backup_from_path", { path });
      await refreshDigest(range);
      setBackupStatus({ kind: "ok", message: "Backup imported" });
    } catch (e) {
      console.error("backup import failed", e);
      setBackupStatus({ kind: "err", message: `Backup import failed: ${e}` });
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
                  <DeltaChip
                    curr={digest.micro_taken + digest.long_taken}
                    prev={digest.previous.breaks_taken}
                  />
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
                  <DeltaChip
                    curr={digest.micro_dismissed + digest.long_dismissed}
                    prev={digest.previous.breaks_dismissed}
                    goodDirection="down"
                  />
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
                  {digest.pause_count} pause
                  {digest.pause_count === 1 ? "" : "s"}
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
            <p className="stat-card-sub">
              Delta chips compare with the previous{" "}
              {range === "month" ? "30 days" : "7 days"}.
            </p>
          </section>

          {digest.postpone_follow_through.total > 0 && (
            <>
              <h2>Postpone follow-through</h2>
              <section>
                <p className="stat-card-sub">
                  How postponed breaks eventually resolved.
                </p>
                <PostponeDonut data={digest.postpone_follow_through} />
              </section>
            </>
          )}

          {digest.suppressions_by_kind.length > 0 && (
            <>
              <h2>Breaks suppressed by</h2>
              <section>
                <SuppressionBars rows={digest.suppressions_by_kind} />
              </section>
            </>
          )}

          <h2>By weekday</h2>
          <section>
            <WeekdayHistogram days={digest.by_weekday} />
            <p className="stat-card-sub">
              Solid: taken. Faded: dismissed. Hover a bar for counts.
            </p>
          </section>

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
              <button className="secondary" onClick={onExportBackup}>
                Export full backup
              </button>
              <button className="secondary" onClick={onImportBackup}>
                Import full backup
              </button>
              <button className="secondary" onClick={onClearLog}>
                Clear history
              </button>
            </div>
            <p className="stat-card-sub">
              Full-backup files contain your manual supporter token (if you have
              one). Treat them like a password — keep them on a device you
              control, don&apos;t post them in public bug reports.
            </p>
            {backupStatus && (
              <p
                className={
                  backupStatus.kind === "err" ? "placeholder" : "stat-card-sub"
                }
                role={backupStatus.kind === "err" ? "alert" : "status"}
              >
                {backupStatus.message}
              </p>
            )}
          </section>
        </>
      )}
    </>
  );
}
