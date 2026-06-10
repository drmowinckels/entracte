use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Timelike, Utc, Weekday};
use log::error;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::scheduler::BreakKind;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Completed,
    Dismissed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkipSource {
    User,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum GuardReason {
    Dnd,
    Camera,
    Idle,
    AppPause,
    Typing,
    Video,
    Plugin,
}

impl GuardReason {
    fn label(self) -> &'static str {
        match self {
            GuardReason::Dnd => "Do Not Disturb",
            GuardReason::Camera => "Camera in use",
            GuardReason::Idle => "Idle",
            GuardReason::AppPause => "Paused-app running",
            GuardReason::Typing => "Actively typing",
            GuardReason::Video => "Video playing",
            GuardReason::Plugin => "Plugin detector",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventPayload {
    BreakStart {
        kind: BreakKind,
        duration_secs: u64,
        enforceable: bool,
    },
    BreakEnd {
        kind: BreakKind,
        outcome: Outcome,
    },
    BreakPostponed {
        kind: BreakKind,
        minutes: u32,
    },
    BreakSkipped {
        kind: BreakKind,
        source: SkipSource,
    },
    BreakResumed {
        kind: BreakKind,
    },
    PauseStart {
        duration_secs: Option<u64>,
    },
    PauseEnd,
    GuardSuppress {
        kind: BreakKind,
        reason: GuardReason,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggedEvent {
    pub t: DateTime<Utc>,
    #[serde(flatten)]
    pub event: EventPayload,
}

impl LoggedEvent {
    pub fn now(event: EventPayload) -> Self {
        Self {
            t: Utc::now(),
            event,
        }
    }
}

/// Background writer for `events.jsonl`. `log` is fire-and-forget over an
/// mpsc channel; the writer thread holds `write_lock` while appending so
/// `clear_log` can take the same lock to safely truncate without racing an
/// in-flight write.
#[derive(Clone)]
pub struct Logger {
    tx: mpsc::UnboundedSender<LoggedEvent>,
    write_lock: Arc<std::sync::Mutex<()>>,
}

impl Logger {
    pub fn spawn(path: PathBuf) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<LoggedEvent>();
        let write_lock = Arc::new(std::sync::Mutex::new(()));
        let lock_for_thread = write_lock.clone();
        std::thread::spawn(move || {
            while let Some(ev) = rx.blocking_recv() {
                let _guard = lock_for_thread.lock().unwrap_or_else(|p| p.into_inner());
                if let Err(e) = append_one(&path, &ev) {
                    error!("stats: failed to append event to {}: {e}", path.display());
                }
            }
        });
        Self { tx, write_lock }
    }

    pub fn log(&self, event: EventPayload) {
        let _ = self.tx.send(LoggedEvent::now(event));
    }

    /// Shared lock the writer holds across each append. Take it before
    /// touching `events.jsonl` from outside the writer thread.
    pub fn write_lock(&self) -> &Arc<std::sync::Mutex<()>> {
        &self.write_lock
    }
}

fn append_one(path: &Path, event: &LoggedEvent) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(path)?;
    let mut line = serde_json::to_string(event).map_err(std::io::Error::other)?;
    line.push('\n');
    file.write_all(line.as_bytes())?;
    Ok(())
}

pub fn read_all(path: &Path) -> Vec<LoggedEvent> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    content
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.is_empty() {
                None
            } else {
                serde_json::from_str::<LoggedEvent>(l).ok()
            }
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct SuppressionCount {
    pub reason: String,
    pub label: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SuppressionByKind {
    pub kind: String,
    pub reason: String,
    pub label: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct DayBucket {
    pub date: String,
    pub taken: u32,
    pub dismissed: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct WeekdayBucket {
    pub weekday: u8,
    pub taken: u32,
    pub dismissed: u32,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct PreviousPeriod {
    pub breaks_taken: u32,
    pub breaks_dismissed: u32,
    pub postponed_total: u32,
    pub skipped_total: u32,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct PostponeFollowThrough {
    pub total: u32,
    pub taken: u32,
    pub dismissed: u32,
    pub skipped: u32,
    pub unresolved: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Digest {
    pub range: String,
    pub range_start: String,
    pub range_end: String,
    pub micro_taken: u32,
    pub micro_dismissed: u32,
    pub long_taken: u32,
    pub long_dismissed: u32,
    pub sleep_shown: u32,
    pub postponed_total: u32,
    pub skipped_total: u32,
    pub suppressions: Vec<SuppressionCount>,
    pub suppressions_by_kind: Vec<SuppressionByKind>,
    pub pause_total_secs: u64,
    pub pause_count: u32,
    pub by_hour: Vec<u32>,
    pub by_day: Vec<DayBucket>,
    pub by_weekday: Vec<WeekdayBucket>,
    pub previous: PreviousPeriod,
    pub postpone_follow_through: PostponeFollowThrough,
}

fn weekday_index(d: Weekday) -> u8 {
    d.num_days_from_monday() as u8
}

pub fn compute_digest(events: &[LoggedEvent], range: &str, now: DateTime<Local>) -> Digest {
    let days_back: i64 = match range {
        "month" => 30,
        _ => 7,
    };
    let range_start = now - Duration::days(days_back);
    let prev_range_start = now - Duration::days(days_back * 2);

    let mut micro_taken = 0u32;
    let mut micro_dismissed = 0u32;
    let mut long_taken = 0u32;
    let mut long_dismissed = 0u32;
    let mut sleep_shown = 0u32;
    let mut postponed_total = 0u32;
    let mut skipped_total = 0u32;
    let mut pause_total_secs: u64 = 0;
    let mut pause_count = 0u32;
    let mut by_hour = vec![0u32; 24];
    let mut by_weekday_taken = [0u32; 7];
    let mut by_weekday_dismissed = [0u32; 7];
    let mut sup_map: HashMap<GuardReason, u32> = HashMap::new();
    let mut sup_kind_map: HashMap<(BreakKind, GuardReason), u32> = HashMap::new();
    let mut previous = PreviousPeriod::default();
    let mut open_pause: Option<DateTime<Utc>> = None;

    for e in events {
        let local = e.t.with_timezone(&Local);
        let in_range = local >= range_start && local <= now;
        let in_prev = local >= prev_range_start && local < range_start;
        if !in_range && !in_prev {
            continue;
        }
        if in_prev {
            match &e.event {
                EventPayload::BreakEnd { kind, outcome } => match (*kind, *outcome) {
                    (BreakKind::Micro | BreakKind::Long, Outcome::Completed) => {
                        previous.breaks_taken += 1
                    }
                    (BreakKind::Micro | BreakKind::Long, Outcome::Dismissed) => {
                        previous.breaks_dismissed += 1
                    }
                    (BreakKind::Sleep, _) => {}
                },
                EventPayload::BreakPostponed { .. } => previous.postponed_total += 1,
                EventPayload::BreakSkipped { .. } => previous.skipped_total += 1,
                _ => {}
            }
            continue;
        }
        match &e.event {
            EventPayload::BreakEnd { kind, outcome } => {
                match (*kind, *outcome) {
                    (BreakKind::Micro, Outcome::Completed) => micro_taken += 1,
                    (BreakKind::Micro, Outcome::Dismissed) => micro_dismissed += 1,
                    (BreakKind::Long, Outcome::Completed) => long_taken += 1,
                    (BreakKind::Long, Outcome::Dismissed) => long_dismissed += 1,
                    (BreakKind::Sleep, _) => sleep_shown += 1,
                }
                let wd = weekday_index(local.weekday()) as usize;
                match (*kind, *outcome) {
                    (BreakKind::Micro | BreakKind::Long, Outcome::Completed) => {
                        by_weekday_taken[wd] += 1;
                        let h = local.hour() as usize;
                        by_hour[h] += 1;
                    }
                    (BreakKind::Micro | BreakKind::Long, Outcome::Dismissed) => {
                        by_weekday_dismissed[wd] += 1;
                    }
                    (BreakKind::Sleep, _) => {}
                }
            }
            EventPayload::BreakPostponed { .. } => postponed_total += 1,
            EventPayload::BreakSkipped { .. } => skipped_total += 1,
            EventPayload::BreakResumed { .. } => {}
            EventPayload::GuardSuppress { kind, reason } => {
                *sup_map.entry(*reason).or_insert(0) += 1;
                *sup_kind_map.entry((*kind, *reason)).or_insert(0) += 1;
            }
            EventPayload::PauseStart { .. } => {
                open_pause = Some(e.t);
            }
            EventPayload::PauseEnd => {
                if let Some(ps) = open_pause.take() {
                    let dur = (e.t - ps).num_seconds().max(0) as u64;
                    pause_total_secs += dur;
                    pause_count += 1;
                }
            }
            EventPayload::BreakStart { .. } => {}
        }
    }

    let mut suppressions: Vec<SuppressionCount> = sup_map
        .into_iter()
        .map(|(reason, count)| SuppressionCount {
            reason: format!("{reason:?}").to_lowercase(),
            label: reason.label().to_string(),
            count,
        })
        .collect();
    suppressions.sort_by_key(|s| std::cmp::Reverse(s.count));

    let mut suppressions_by_kind: Vec<SuppressionByKind> = sup_kind_map
        .into_iter()
        .map(|((kind, reason), count)| SuppressionByKind {
            kind: kind_str(kind).to_string(),
            reason: format!("{reason:?}").to_lowercase(),
            label: reason.label().to_string(),
            count,
        })
        .collect();
    suppressions_by_kind.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.reason.cmp(&b.reason))
    });

    let by_weekday: Vec<WeekdayBucket> = (0u8..7)
        .map(|w| WeekdayBucket {
            weekday: w,
            taken: by_weekday_taken[w as usize],
            dismissed: by_weekday_dismissed[w as usize],
        })
        .collect();

    let postpone_follow_through = compute_postpone_follow_through(events, range_start, now);

    let heatmap_days = 84i64;
    let heatmap_start = (now - Duration::days(heatmap_days - 1)).date_naive();
    let today = now.date_naive();
    let mut buckets: HashMap<NaiveDate, (u32, u32)> = HashMap::new();
    for i in 0..heatmap_days {
        let d = heatmap_start + Duration::days(i);
        buckets.insert(d, (0, 0));
    }
    for e in events {
        let local = e.t.with_timezone(&Local);
        let date = local.date_naive();
        if date < heatmap_start || date > today {
            continue;
        }
        if let EventPayload::BreakEnd { outcome, .. } = e.event {
            if let Some(b) = buckets.get_mut(&date) {
                match outcome {
                    Outcome::Completed => b.0 += 1,
                    Outcome::Dismissed => b.1 += 1,
                }
            }
        }
    }
    let mut by_day: Vec<(NaiveDate, (u32, u32))> = buckets.into_iter().collect();
    by_day.sort_by_key(|a| a.0);
    let by_day = by_day
        .into_iter()
        .map(|(d, (taken, dismissed))| DayBucket {
            date: d.format("%Y-%m-%d").to_string(),
            taken,
            dismissed,
        })
        .collect();

    Digest {
        range: range.to_string(),
        range_start: range_start.to_rfc3339(),
        range_end: now.to_rfc3339(),
        micro_taken,
        micro_dismissed,
        long_taken,
        long_dismissed,
        sleep_shown,
        postponed_total,
        skipped_total,
        suppressions,
        suppressions_by_kind,
        pause_total_secs,
        pause_count,
        by_hour,
        by_day,
        by_weekday,
        previous,
        postpone_follow_through,
    }
}

/// For every `BreakPostponed` event inside `[range_start, now]`, look
/// forward in the (chronologically ordered) event stream for the next
/// `BreakEnd` or `BreakSkipped` of the same kind and bucket the outcome.
/// Intervening postpones of the same kind don't resolve — we keep
/// scanning. A postpone with no later resolution in the log counts as
/// `unresolved`.
fn compute_postpone_follow_through(
    events: &[LoggedEvent],
    range_start: DateTime<Local>,
    now: DateTime<Local>,
) -> PostponeFollowThrough {
    let mut out = PostponeFollowThrough::default();
    for (i, e) in events.iter().enumerate() {
        let EventPayload::BreakPostponed { kind, .. } = &e.event else {
            continue;
        };
        let local = e.t.with_timezone(&Local);
        if local < range_start || local > now {
            continue;
        }
        out.total += 1;
        let mut resolved = false;
        for f in &events[i + 1..] {
            match &f.event {
                EventPayload::BreakEnd { kind: k2, outcome } if k2 == kind => {
                    match outcome {
                        Outcome::Completed => out.taken += 1,
                        Outcome::Dismissed => out.dismissed += 1,
                    }
                    resolved = true;
                    break;
                }
                EventPayload::BreakSkipped { kind: k2, .. } if k2 == kind => {
                    out.skipped += 1;
                    resolved = true;
                    break;
                }
                _ => {}
            }
        }
        if !resolved {
            out.unresolved += 1;
        }
    }
    out
}

type CsvFields<'a> = (
    &'a str,
    Option<&'a str>,
    Option<&'a str>,
    Option<&'a str>,
    Option<String>,
    Option<String>,
);

pub fn export_csv(events: &[LoggedEvent]) -> String {
    let mut out = String::from("timestamp,type,kind,outcome,reason,duration_secs,minutes\n");
    for e in events {
        let t = e.t.to_rfc3339();
        let (typ, kind, outcome, reason, dur, min): CsvFields = match &e.event {
            EventPayload::BreakStart {
                kind,
                duration_secs,
                ..
            } => (
                "break_start",
                Some(kind_str(*kind)),
                None,
                None,
                Some(duration_secs.to_string()),
                None,
            ),
            EventPayload::BreakEnd { kind, outcome } => (
                "break_end",
                Some(kind_str(*kind)),
                Some(outcome_str(*outcome)),
                None,
                None,
                None,
            ),
            EventPayload::BreakPostponed { kind, minutes } => (
                "break_postponed",
                Some(kind_str(*kind)),
                None,
                None,
                None,
                Some(minutes.to_string()),
            ),
            EventPayload::BreakSkipped { kind, .. } => (
                "break_skipped",
                Some(kind_str(*kind)),
                None,
                None,
                None,
                None,
            ),
            EventPayload::BreakResumed { kind } => (
                "break_resumed",
                Some(kind_str(*kind)),
                None,
                None,
                None,
                None,
            ),
            EventPayload::PauseStart { duration_secs } => (
                "pause_start",
                None,
                None,
                None,
                duration_secs.map(|d| d.to_string()),
                None,
            ),
            EventPayload::PauseEnd => ("pause_end", None, None, None, None, None),
            EventPayload::GuardSuppress { kind, reason } => (
                "guard_suppress",
                Some(kind_str(*kind)),
                None,
                Some(guard_str(*reason)),
                None,
                None,
            ),
        };
        out.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            t,
            typ,
            kind.unwrap_or(""),
            outcome.unwrap_or(""),
            reason.unwrap_or(""),
            dur.unwrap_or_default(),
            min.unwrap_or_default(),
        ));
    }
    out
}

fn kind_str(k: BreakKind) -> &'static str {
    match k {
        BreakKind::Micro => "micro",
        BreakKind::Long => "long",
        BreakKind::Sleep => "sleep",
    }
}

fn outcome_str(o: Outcome) -> &'static str {
    match o {
        Outcome::Completed => "completed",
        Outcome::Dismissed => "dismissed",
    }
}

fn guard_str(g: GuardReason) -> &'static str {
    match g {
        GuardReason::Dnd => "dnd",
        GuardReason::Camera => "camera",
        GuardReason::Idle => "idle",
        GuardReason::AppPause => "app_pause",
        GuardReason::Typing => "typing",
        GuardReason::Video => "video",
        GuardReason::Plugin => "plugin",
    }
}

/// Remove `events.jsonl`. Takes the shared writer lock so an in-flight
/// append from the [`Logger`] worker thread can finish first — without it,
/// the writer could re-create the file between our `remove_file` and the
/// next event landing.
pub fn clear_log(path: &Path, write_lock: &std::sync::Mutex<()>) -> std::io::Result<()> {
    let _guard = write_lock.lock().unwrap_or_else(|p| p.into_inner());
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ev(at: DateTime<Local>, payload: EventPayload) -> LoggedEvent {
        LoggedEvent {
            t: at.with_timezone(&Utc),
            event: payload,
        }
    }

    fn now() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 5, 14, 14, 0, 0).unwrap()
    }

    #[test]
    fn empty_digest_has_zero_totals() {
        let d = compute_digest(&[], "week", now());
        assert_eq!(d.micro_taken, 0);
        assert_eq!(d.long_taken, 0);
        assert_eq!(d.sleep_shown, 0);
        assert_eq!(d.by_day.len(), 84);
        assert_eq!(d.by_hour.len(), 24);
    }

    #[test]
    fn counts_break_end_completed_vs_dismissed() {
        let n = now();
        let events = vec![
            ev(
                n - Duration::hours(2),
                EventPayload::BreakEnd {
                    kind: BreakKind::Micro,
                    outcome: Outcome::Completed,
                },
            ),
            ev(
                n - Duration::hours(1),
                EventPayload::BreakEnd {
                    kind: BreakKind::Micro,
                    outcome: Outcome::Dismissed,
                },
            ),
            ev(
                n - Duration::days(3),
                EventPayload::BreakEnd {
                    kind: BreakKind::Long,
                    outcome: Outcome::Completed,
                },
            ),
            ev(
                n - Duration::days(2),
                EventPayload::BreakEnd {
                    kind: BreakKind::Sleep,
                    outcome: Outcome::Completed,
                },
            ),
        ];
        let d = compute_digest(&events, "week", n);
        assert_eq!(d.micro_taken, 1);
        assert_eq!(d.micro_dismissed, 1);
        assert_eq!(d.long_taken, 1);
        assert_eq!(d.sleep_shown, 1);
    }

    #[test]
    fn week_range_excludes_older_events() {
        let n = now();
        let events = vec![
            ev(
                n - Duration::days(2),
                EventPayload::BreakEnd {
                    kind: BreakKind::Micro,
                    outcome: Outcome::Completed,
                },
            ),
            ev(
                n - Duration::days(20),
                EventPayload::BreakEnd {
                    kind: BreakKind::Micro,
                    outcome: Outcome::Completed,
                },
            ),
        ];
        let d_week = compute_digest(&events, "week", n);
        assert_eq!(d_week.micro_taken, 1);
        let d_month = compute_digest(&events, "month", n);
        assert_eq!(d_month.micro_taken, 2);
    }

    #[test]
    fn suppressions_sorted_by_count_desc() {
        let n = now();
        let events = vec![
            ev(
                n - Duration::hours(1),
                EventPayload::GuardSuppress {
                    kind: BreakKind::Micro,
                    reason: GuardReason::Camera,
                },
            ),
            ev(
                n - Duration::hours(2),
                EventPayload::GuardSuppress {
                    kind: BreakKind::Micro,
                    reason: GuardReason::Camera,
                },
            ),
            ev(
                n - Duration::hours(3),
                EventPayload::GuardSuppress {
                    kind: BreakKind::Long,
                    reason: GuardReason::Dnd,
                },
            ),
        ];
        let d = compute_digest(&events, "week", n);
        assert_eq!(d.suppressions.len(), 2);
        assert_eq!(d.suppressions[0].reason, "camera");
        assert_eq!(d.suppressions[0].count, 2);
        assert_eq!(d.suppressions[1].reason, "dnd");
        assert_eq!(d.suppressions[1].count, 1);
    }

    #[test]
    fn pause_pairs_start_and_end() {
        let n = now();
        let events = vec![
            ev(
                n - Duration::hours(2),
                EventPayload::PauseStart {
                    duration_secs: Some(3600),
                },
            ),
            ev(n - Duration::hours(1), EventPayload::PauseEnd),
            ev(
                n - Duration::minutes(30),
                EventPayload::PauseStart {
                    duration_secs: None,
                },
            ),
            ev(n - Duration::minutes(15), EventPayload::PauseEnd),
        ];
        let d = compute_digest(&events, "week", n);
        assert_eq!(d.pause_count, 2);
        assert_eq!(d.pause_total_secs, 3600 + 15 * 60);
    }

    #[test]
    fn by_hour_buckets_completed_breaks() {
        let n = now();
        let nine_am = Local.with_ymd_and_hms(2026, 5, 14, 9, 30, 0).unwrap();
        let events = vec![
            ev(
                nine_am,
                EventPayload::BreakEnd {
                    kind: BreakKind::Micro,
                    outcome: Outcome::Completed,
                },
            ),
            ev(
                nine_am + Duration::minutes(5),
                EventPayload::BreakEnd {
                    kind: BreakKind::Micro,
                    outcome: Outcome::Completed,
                },
            ),
            ev(
                nine_am,
                EventPayload::BreakEnd {
                    kind: BreakKind::Micro,
                    outcome: Outcome::Dismissed,
                },
            ),
        ];
        let d = compute_digest(&events, "week", n);
        assert_eq!(d.by_hour[9], 2);
        assert_eq!(d.by_hour[8], 0);
    }

    #[test]
    fn heatmap_always_has_84_days_in_order() {
        let n = now();
        let d = compute_digest(&[], "week", n);
        assert_eq!(d.by_day.len(), 84);
        for window in d.by_day.windows(2) {
            assert!(window[0].date < window[1].date);
        }
        assert_eq!(
            d.by_day.last().unwrap().date,
            n.format("%Y-%m-%d").to_string()
        );
    }

    #[test]
    fn csv_export_has_header_and_rows() {
        let n = now();
        let events = vec![
            ev(
                n,
                EventPayload::BreakEnd {
                    kind: BreakKind::Micro,
                    outcome: Outcome::Completed,
                },
            ),
            ev(
                n,
                EventPayload::GuardSuppress {
                    kind: BreakKind::Long,
                    reason: GuardReason::Dnd,
                },
            ),
        ];
        let csv = export_csv(&events);
        let lines: Vec<&str> = csv.lines().collect();
        assert!(lines[0].starts_with("timestamp,type"));
        assert!(lines[1].contains("break_end"));
        assert!(lines[1].contains("micro"));
        assert!(lines[1].contains("completed"));
        assert!(lines[2].contains("guard_suppress"));
        assert!(lines[2].contains("dnd"));
    }

    #[test]
    fn round_trip_event_through_json() {
        let n = now();
        let original = ev(
            n,
            EventPayload::BreakStart {
                kind: BreakKind::Long,
                duration_secs: 600,
                enforceable: true,
            },
        );
        let json = serde_json::to_string(&original).unwrap();
        assert!(json.contains("\"type\":\"break_start\""));
        assert!(json.contains("\"kind\":\"long\""));
        let parsed: LoggedEvent = serde_json::from_str(&json).unwrap();
        match parsed.event {
            EventPayload::BreakStart {
                duration_secs,
                enforceable,
                ..
            } => {
                assert_eq!(duration_secs, 600);
                assert!(enforceable);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn read_all_skips_blank_and_corrupt_lines() {
        let dir = crate::test_support::temp_dir();
        let path = dir.path().join("events.jsonl");
        let valid = serde_json::to_string(&ev(
            now(),
            EventPayload::BreakEnd {
                kind: BreakKind::Micro,
                outcome: Outcome::Completed,
            },
        ))
        .unwrap();
        let body = format!("\n{valid}\nnot json\n\n{valid}\n");
        std::fs::write(&path, body).unwrap();
        let events = read_all(&path);
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn read_all_returns_empty_when_missing() {
        let path = PathBuf::from("/tmp/entracte-definitely-does-not-exist.jsonl");
        let events = read_all(&path);
        assert!(events.is_empty());
    }

    #[test]
    fn typing_guard_reason_round_trips() {
        let n = now();
        let original = ev(
            n,
            EventPayload::GuardSuppress {
                kind: BreakKind::Micro,
                reason: GuardReason::Typing,
            },
        );
        let json = serde_json::to_string(&original).unwrap();
        assert!(json.contains("\"reason\":\"typing\""));
        let parsed: LoggedEvent = serde_json::from_str(&json).unwrap();
        match parsed.event {
            EventPayload::GuardSuppress { reason, .. } => {
                assert_eq!(reason, GuardReason::Typing);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn typing_suppression_counts_into_digest() {
        let n = now();
        let events = vec![
            ev(
                n - Duration::hours(1),
                EventPayload::GuardSuppress {
                    kind: BreakKind::Long,
                    reason: GuardReason::Typing,
                },
            ),
            ev(
                n - Duration::hours(2),
                EventPayload::GuardSuppress {
                    kind: BreakKind::Micro,
                    reason: GuardReason::Typing,
                },
            ),
        ];
        let d = compute_digest(&events, "week", n);
        let typing = d.suppressions.iter().find(|s| s.reason == "typing");
        let typing = typing.expect("typing suppression present");
        assert_eq!(typing.count, 2);
        assert_eq!(typing.label, "Actively typing");
    }

    #[test]
    fn typing_guard_reason_csv_uses_snake_case() {
        let n = now();
        let events = vec![ev(
            n,
            EventPayload::GuardSuppress {
                kind: BreakKind::Long,
                reason: GuardReason::Typing,
            },
        )];
        let csv = export_csv(&events);
        assert!(csv.lines().nth(1).unwrap().contains("typing"));
    }

    #[test]
    fn by_weekday_indexes_monday_zero_to_sunday_six() {
        let n = now();
        let thursday = Local.with_ymd_and_hms(2026, 5, 14, 10, 0, 0).unwrap();
        let sunday = Local.with_ymd_and_hms(2026, 5, 10, 10, 0, 0).unwrap();
        let events = vec![
            ev(
                thursday,
                EventPayload::BreakEnd {
                    kind: BreakKind::Micro,
                    outcome: Outcome::Completed,
                },
            ),
            ev(
                sunday,
                EventPayload::BreakEnd {
                    kind: BreakKind::Long,
                    outcome: Outcome::Dismissed,
                },
            ),
        ];
        let d = compute_digest(&events, "week", n);
        assert_eq!(d.by_weekday.len(), 7);
        assert_eq!(d.by_weekday[3].weekday, 3);
        assert_eq!(d.by_weekday[3].taken, 1);
        assert_eq!(d.by_weekday[6].weekday, 6);
        assert_eq!(d.by_weekday[6].dismissed, 1);
    }

    #[test]
    fn by_weekday_ignores_sleep_prompts() {
        let n = now();
        let events = vec![ev(
            n,
            EventPayload::BreakEnd {
                kind: BreakKind::Sleep,
                outcome: Outcome::Completed,
            },
        )];
        let d = compute_digest(&events, "week", n);
        assert!(d.by_weekday.iter().all(|w| w.taken == 0));
    }

    #[test]
    fn previous_period_tallies_one_window_back() {
        let n = now();
        let events = vec![
            ev(
                n - Duration::days(2),
                EventPayload::BreakEnd {
                    kind: BreakKind::Micro,
                    outcome: Outcome::Completed,
                },
            ),
            ev(
                n - Duration::days(9),
                EventPayload::BreakEnd {
                    kind: BreakKind::Long,
                    outcome: Outcome::Completed,
                },
            ),
            ev(
                n - Duration::days(10),
                EventPayload::BreakEnd {
                    kind: BreakKind::Long,
                    outcome: Outcome::Dismissed,
                },
            ),
            ev(
                n - Duration::days(8),
                EventPayload::BreakPostponed {
                    kind: BreakKind::Micro,
                    minutes: 5,
                },
            ),
            ev(
                n - Duration::days(20),
                EventPayload::BreakSkipped {
                    kind: BreakKind::Micro,
                    source: SkipSource::User,
                },
            ),
        ];
        let d = compute_digest(&events, "week", n);
        assert_eq!(d.micro_taken + d.long_taken, 1);
        assert_eq!(d.previous.breaks_taken, 1);
        assert_eq!(d.previous.breaks_dismissed, 1);
        assert_eq!(d.previous.postponed_total, 1);
        assert_eq!(
            d.previous.skipped_total, 0,
            "events older than two windows back are excluded"
        );
    }

    #[test]
    fn suppressions_by_kind_splits_reason_per_break_kind() {
        let n = now();
        let events = vec![
            ev(
                n - Duration::hours(1),
                EventPayload::GuardSuppress {
                    kind: BreakKind::Long,
                    reason: GuardReason::Dnd,
                },
            ),
            ev(
                n - Duration::hours(2),
                EventPayload::GuardSuppress {
                    kind: BreakKind::Long,
                    reason: GuardReason::Dnd,
                },
            ),
            ev(
                n - Duration::hours(3),
                EventPayload::GuardSuppress {
                    kind: BreakKind::Micro,
                    reason: GuardReason::Dnd,
                },
            ),
            ev(
                n - Duration::hours(4),
                EventPayload::GuardSuppress {
                    kind: BreakKind::Micro,
                    reason: GuardReason::Camera,
                },
            ),
        ];
        let d = compute_digest(&events, "week", n);
        assert_eq!(d.suppressions_by_kind.len(), 3);
        assert_eq!(d.suppressions_by_kind[0].kind, "long");
        assert_eq!(d.suppressions_by_kind[0].reason, "dnd");
        assert_eq!(d.suppressions_by_kind[0].count, 2);
        let micro_dnd = d
            .suppressions_by_kind
            .iter()
            .find(|s| s.kind == "micro" && s.reason == "dnd")
            .expect("micro/dnd present");
        assert_eq!(micro_dnd.count, 1);
        let total_dnd: u32 = d
            .suppressions_by_kind
            .iter()
            .filter(|s| s.reason == "dnd")
            .map(|s| s.count)
            .sum();
        let agg_dnd = d
            .suppressions
            .iter()
            .find(|s| s.reason == "dnd")
            .unwrap()
            .count;
        assert_eq!(
            total_dnd, agg_dnd,
            "per-kind split must sum to the flat suppressions count"
        );
    }

    #[test]
    fn postpone_follow_through_taken_dismissed_skipped_unresolved() {
        let n = now();
        let events = vec![
            // Postponed and later taken
            ev(
                n - Duration::hours(5),
                EventPayload::BreakPostponed {
                    kind: BreakKind::Micro,
                    minutes: 5,
                },
            ),
            ev(
                n - Duration::hours(4),
                EventPayload::BreakEnd {
                    kind: BreakKind::Micro,
                    outcome: Outcome::Completed,
                },
            ),
            // Postponed and later dismissed
            ev(
                n - Duration::hours(3),
                EventPayload::BreakPostponed {
                    kind: BreakKind::Long,
                    minutes: 10,
                },
            ),
            ev(
                n - Duration::hours(2),
                EventPayload::BreakEnd {
                    kind: BreakKind::Long,
                    outcome: Outcome::Dismissed,
                },
            ),
            // Postponed and later skipped
            ev(
                n - Duration::hours(1) - Duration::minutes(30),
                EventPayload::BreakPostponed {
                    kind: BreakKind::Micro,
                    minutes: 5,
                },
            ),
            ev(
                n - Duration::hours(1),
                EventPayload::BreakSkipped {
                    kind: BreakKind::Micro,
                    source: SkipSource::User,
                },
            ),
            // Postponed with no resolution after it
            ev(
                n - Duration::minutes(10),
                EventPayload::BreakPostponed {
                    kind: BreakKind::Long,
                    minutes: 10,
                },
            ),
        ];
        let d = compute_digest(&events, "week", n);
        assert_eq!(d.postpone_follow_through.total, 4);
        assert_eq!(d.postpone_follow_through.taken, 1);
        assert_eq!(d.postpone_follow_through.dismissed, 1);
        assert_eq!(d.postpone_follow_through.skipped, 1);
        assert_eq!(d.postpone_follow_through.unresolved, 1);
    }

    #[test]
    fn postpone_follow_through_skips_intervening_other_kind() {
        let n = now();
        let events = vec![
            ev(
                n - Duration::hours(3),
                EventPayload::BreakPostponed {
                    kind: BreakKind::Long,
                    minutes: 5,
                },
            ),
            // BreakEnd of a different kind — must not resolve the long postpone
            ev(
                n - Duration::hours(2),
                EventPayload::BreakEnd {
                    kind: BreakKind::Micro,
                    outcome: Outcome::Completed,
                },
            ),
            ev(
                n - Duration::hours(1),
                EventPayload::BreakEnd {
                    kind: BreakKind::Long,
                    outcome: Outcome::Completed,
                },
            ),
        ];
        let d = compute_digest(&events, "week", n);
        assert_eq!(d.postpone_follow_through.total, 1);
        assert_eq!(d.postpone_follow_through.taken, 1);
        assert_eq!(d.postpone_follow_through.unresolved, 0);
    }

    #[test]
    fn postpone_follow_through_only_counts_postpones_in_range() {
        let n = now();
        let events = vec![
            ev(
                n - Duration::days(20),
                EventPayload::BreakPostponed {
                    kind: BreakKind::Micro,
                    minutes: 5,
                },
            ),
            ev(
                n - Duration::days(19),
                EventPayload::BreakEnd {
                    kind: BreakKind::Micro,
                    outcome: Outcome::Completed,
                },
            ),
        ];
        let d = compute_digest(&events, "week", n);
        assert_eq!(
            d.postpone_follow_through.total, 0,
            "postpone outside the week range should not contribute"
        );
    }
}
