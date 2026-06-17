use std::time::{Duration, Instant};

use chrono::{Datelike, Local, Timelike};

use super::types::{BreakDelivery, BreakKind};

/// All of the scheduler's per-tick mutable timing state.
///
/// Held behind a `tokio::Mutex` inside `Scheduler`. Every field tracks
/// either when something last happened (`last_*`) or where we are in
/// a per-kind state machine (warned, deferred-since, postpone counter).
#[derive(Debug)]
pub struct BreakTimers {
    pub last_micro: Instant,
    pub last_long: Instant,
    pub last_sleep: Option<Instant>,
    pub micro_warned: bool,
    pub long_warned: bool,
    pub active_break: Option<BreakKind>,
    pub micro_deferred_since: Option<Instant>,
    pub long_deferred_since: Option<Instant>,
    pub micro_postpone_count: u32,
    pub long_postpone_count: u32,
    pub last_skipped_or_postponed: Option<(BreakKind, Instant)>,
    /// `(local-date, minute-of-day)` of the most recent fixed-time micro
    /// fire. Keyed by date so the dedupe survives DST transitions: a
    /// "fall back" 02:00 → 01:00 reuses the same minute on the same day,
    /// and "spring forward" never strands the dedupe pointing at a minute
    /// that no longer exists on the wall clock.
    pub last_micro_fixed_fire: Option<(String, u32)>,
    pub last_long_fixed_fire: Option<(String, u32)>,
}

impl BreakTimers {
    /// Fresh timers with both interval clocks anchored at `Instant::now()`
    /// and every flag / counter cleared. Used at scheduler boot.
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            last_micro: now,
            last_long: now,
            last_sleep: None,
            micro_warned: false,
            long_warned: false,
            active_break: None,
            micro_deferred_since: None,
            long_deferred_since: None,
            micro_postpone_count: 0,
            long_postpone_count: 0,
            last_skipped_or_postponed: None,
            last_micro_fixed_fire: None,
            last_long_fixed_fire: None,
        }
    }
}

/// Re-anchor the micro / long interval clocks to `now` and clear the
/// deferral / postpone / warn state, without disturbing `last_sleep`,
/// `active_break`, or the fixed-time dedupe keys. `now` is injected so
/// the re-anchor is a pure function over `BreakTimers` and a timestamp.
///
/// Used on two paths that both want "fresh intervals from this instant":
/// a profile switch ([`reset_timers_keep_sleep`]) and a resume from pause
/// ([`reanchor_intervals_on_resume`]). Sleep/bedtime semantics and any
/// fixed-time schedule survive untouched — only the interval anchors and
/// their per-cycle flags move.
pub fn reanchor_intervals_to(t: &mut BreakTimers, now: Instant) {
    t.last_micro = now;
    t.last_long = now;
    t.micro_warned = false;
    t.long_warned = false;
    t.micro_deferred_since = None;
    t.long_deferred_since = None;
    t.micro_postpone_count = 0;
    t.long_postpone_count = 0;
}

/// Reset the micro / long timers and clear deferral / postpone state
/// without disturbing `last_sleep` or `active_break`. Called when the
/// active profile switches: a new profile gets fresh intervals but we
/// don't want to re-fire a sleep prompt that's already shown today.
pub fn reset_timers_keep_sleep(t: &mut BreakTimers) {
    reanchor_intervals_to(t, Instant::now());
}

/// Re-anchor the interval clocks when the scheduler resumes from a pause
/// (timed-pause expiry or a manual "Resume"). Paused time must not count
/// toward break intervals: without this, an hour-long pause leaves
/// `last_micro` / `last_long` an hour in the past, so the very next tick
/// sees every interval already overdue — the tray reads `0:00` and a
/// break fires the instant the user resumes (#134).
///
/// Re-anchoring to the resume `now` restarts each interval from a full
/// period and suppresses the spurious immediate fire. It deliberately
/// leaves `last_sleep` and the fixed-time dedupe keys alone, so bedtime
/// re-prompt cadence and fixed-time schedules keep their own clocks: a
/// fixed-time break legitimately due at the resume minute still fires via
/// its own date/minute path.
pub fn reanchor_intervals_on_resume(t: &mut BreakTimers, now: Instant) {
    reanchor_intervals_to(t, now);
}

/// Apply the timer bookkeeping for a scheduled micro/long break that has
/// just fired. Pure analogue of the inline resets in `run_loop`'s
/// fixed-time and interval fire paths, so the field-by-field divergence
/// between the two break kinds (and between a fixed vs interval fire) is
/// unit-testable without a windowing runtime or a live clock.
///
/// - A `Long` fire resets *both* interval clocks (a long break implies a
///   micro break just happened) and clears both warn/deferral flags; a
///   `Micro` fire touches only the micro fields.
/// - `now` is injected (the live call site passes `Instant::now()`).
/// - `fixed_key` is `Some((local-date, minute-of-day))` for a fixed-time
///   fire — recording the dedupe key — and `None` for an interval fire.
/// - `active_break` is only stashed for deliveries that hold the screen
///   (`Overlay` / `Windowed`); a `Notification` leaves it unset.
///
/// `Sleep` never reaches here (it uses the bedtime fire path) and is a
/// no-op for the per-kind resets.
pub fn record_scheduled_fire(
    t: &mut BreakTimers,
    kind: BreakKind,
    delivery: BreakDelivery,
    now: Instant,
    fixed_key: Option<(String, u32)>,
) {
    match kind {
        BreakKind::Long => {
            t.last_long = now;
            t.last_micro = now;
            t.long_warned = false;
            t.micro_warned = false;
            t.long_deferred_since = None;
            t.micro_deferred_since = None;
            if let Some(key) = fixed_key {
                t.last_long_fixed_fire = Some(key);
            }
        }
        BreakKind::Micro => {
            t.last_micro = now;
            t.micro_warned = false;
            t.micro_deferred_since = None;
            if let Some(key) = fixed_key {
                t.last_micro_fixed_fire = Some(key);
            }
        }
        BreakKind::Sleep => {}
    }
    if matches!(delivery, BreakDelivery::Overlay | BreakDelivery::Windowed) {
        t.active_break = Some(kind);
    }
}

/// Clear the "resume last skipped break" slot. Returns `true` iff a
/// stored break was actually cleared (used to decide whether to emit
/// the `last_break:changed` event).
pub fn clear_last_break(t: &mut BreakTimers) -> bool {
    if t.last_skipped_or_postponed.is_some() {
        t.last_skipped_or_postponed = None;
        true
    } else {
        false
    }
}

/// Minutes since local midnight (0..1440). The unit used everywhere
/// the scheduler reasons about time-of-day windows (work hours,
/// bedtime window, fixed-time break list).
pub fn current_minutes() -> u32 {
    let now = Local::now();
    now.hour() * 60 + now.minute()
}

/// ISO-8601 date in local time (`"YYYY-MM-DD"`). Used to detect
/// midnight rollovers for screen-time / fixed-time dedupe state.
pub fn local_today_string() -> String {
    Local::now().date_naive().format("%Y-%m-%d").to_string()
}

/// The current local weekday as days-since-Monday (`0` = Monday … `6` =
/// Sunday). Matches the bit layout of `Settings::work_days_mask` so the
/// work-window day check can index it directly.
pub fn current_weekday() -> u32 {
    Local::now().weekday().num_days_from_monday()
}

/// Parse `"HH:MM"` (or `"H:MM"`) into minutes since midnight.
/// Returns `None` on anything out of range or unparseable — used to
/// filter the user's fixed-time list without spilling errors.
pub fn parse_hhmm(s: &str) -> Option<u32> {
    let trimmed = s.trim();
    let (h_str, m_str) = trimmed.split_once(':')?;
    if h_str.is_empty() || m_str.len() != 2 {
        return None;
    }
    let h: u32 = h_str.parse().ok()?;
    let m: u32 = m_str.parse().ok()?;
    if h >= 24 || m >= 60 {
        return None;
    }
    Some(h * 60 + m)
}

/// Dedupe gate for fixed-time fires: `true` unless we already fired
/// this exact `(date, minute)` slot. Prevents the 1Hz tick from firing
/// the same fixed slot up to 60 times, and (because the key includes
/// the local date) stays correct across DST: `02:00` on a "fall back"
/// day fires once even though the wall clock visits it twice, and
/// `02:30` on a "spring forward" day simply never matches.
pub fn should_fire_fixed_now(
    today: &str,
    current_min: u32,
    last_fire: Option<&(String, u32)>,
) -> bool {
    match last_fire {
        Some((day, prev_min)) => day != today || *prev_min != current_min,
        None => true,
    }
}

/// True iff `now` (minutes since midnight) falls inside `[start, end)`,
/// with wrap-around: a window like `22:00`–`06:00` correctly straddles
/// midnight. `start == end` is treated as an empty window.
pub fn in_window(now: u32, start: u32, end: u32) -> bool {
    if start == end {
        return false;
    }
    if start < end {
        now >= start && now < end
    } else {
        now >= start || now < end
    }
}

/// True iff the work window is open right now: the time-of-day is inside
/// `[start, end)` **and** the relevant weekday's bit is set in
/// `days_mask`. `today` is days-since-Monday (`0`=Mon … `6`=Sun), the
/// same layout as the mask.
///
/// For a window that wraps past midnight (`start > end`, e.g.
/// 22:00–06:00) the early-morning portion is gated on the day the window
/// *began* — Friday's 22:00 shift still covers Saturday 02:00 even when
/// Saturday is off, and an off Sunday won't start a window that bleeds
/// into Monday morning. A window that doesn't wrap is gated on `today`.
///
/// `days_mask == 0` (no day enabled) always returns false; callers gate
/// on `work_window_enabled` before reaching here, so that simply means
/// the window never opens.
pub fn work_window_active(now: u32, start: u32, end: u32, today: u32, days_mask: u8) -> bool {
    if !in_window(now, start, end) {
        return false;
    }
    let window_day = if start <= end || now >= start {
        today
    } else {
        (today + 6) % 7
    };
    days_mask & (1 << window_day) != 0
}

/// True iff an interval-mode break of this kind is due to fire now.
///
/// All inputs are explicit so callers can drive it with a synthetic
/// `Instant` in tests. `now.saturating_duration_since(last_fire)` mirrors
/// the production check `last_fire.elapsed()` with a frozen clock.
///
/// `mode_includes_interval` is the de-stringified equivalent of the
/// settings's `*_schedule_mode` ∈ {`"interval"`, `"both"`} — done at the
/// call site so this stays clock-agnostic.
pub fn interval_break_due(
    enabled: bool,
    mode_includes_interval: bool,
    last_fire: Instant,
    interval_secs: u64,
    idle_suppressed: bool,
    now: Instant,
) -> bool {
    enabled
        && mode_includes_interval
        && !idle_suppressed
        && now.saturating_duration_since(last_fire) >= Duration::from_secs(interval_secs)
}

/// The four conditions that gate a pre-break warning before any timing
/// is considered: the feature must be on, the schedule must include
/// interval breaks, the user must not be idle-suppressed, and we must not
/// have already warned this cycle. Grouped so each is set by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrebreakGate {
    pub enabled: bool,
    pub mode_includes_interval: bool,
    pub already_warned: bool,
    pub idle_suppressed: bool,
}

/// True iff the pre-break notification for this kind should fire now —
/// i.e. we're inside the lead window before a due interval break, and
/// we haven't already shown the notification for this cycle.
///
/// Pure analogue of the inline check in `run_loop`. Decoupled from
/// `Scheduler` state so tests can drive it with synthetic instants. The
/// four short-circuit conditions are grouped into [`PrebreakGate`] so
/// call sites set each by name instead of passing a run of positional
/// booleans.
pub fn prebreak_warn_due(
    gate: PrebreakGate,
    last_fire: Instant,
    interval_secs: u64,
    lead_secs: u64,
    now: Instant,
) -> bool {
    if !gate.enabled || !gate.mode_includes_interval || gate.idle_suppressed || gate.already_warned
    {
        return false;
    }
    let interval = Duration::from_secs(interval_secs);
    let lead = Duration::from_secs(lead_secs);
    let warn_at = interval.saturating_sub(lead);
    let elapsed = now.saturating_duration_since(last_fire);
    elapsed >= warn_at && elapsed < interval
}

/// Decision returned by `decide_bedtime` — fully captures what the tick
/// should do with the bedtime window. The caller still performs the
/// side effects (overlay, hooks, logging, timer mutation).
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum BedtimeAction {
    /// In the bedtime window AND it's time to (re)show the prompt.
    Fire,
    /// In the bedtime window but the per-window interval hasn't elapsed
    /// since the last prompt — only reset the micro/long anchors so
    /// they don't pile up while the user is winding down.
    ResetTimersOnly,
    /// Outside the bedtime window — bedtime branch is a no-op this tick.
    NotInWindow,
}

/// The configured bedtime window: whether the feature is on, its
/// start/end minute-of-day bounds, and the per-window re-prompt interval.
/// Grouped so `decide_bedtime`'s call sites set each field by name rather
/// than passing four positional values that are easy to transpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BedtimeWindow {
    pub enabled: bool,
    pub start_min: u32,
    pub end_min: u32,
    pub interval_secs: u64,
}

/// Pure bedtime decision: combine the time-of-day window, the per-window
/// interval, and the `last_sleep` anchor into one of three actions.
///
/// `last_sleep == None` always fires on the first tick of the window —
/// the cap only kicks in for re-prompts.
///
/// `resumed_from_suspend` is `true` on the single tick that follows a
/// wake from sleep (detected via a wall-clock jump in the run loop). The
/// monotonic `last_sleep` anchor can show an arbitrarily large elapsed
/// interval across a suspend, which would otherwise fire a stale catch-up
/// prompt the instant the lid opens. When we've *already* prompted this
/// window (`last_sleep` is `Some`), we demote that catch-up to
/// `ResetTimersOnly` so opening the laptop mid-window stays quiet. A
/// first entry into the window (`last_sleep` is `None`, e.g. the laptop
/// was closed before bedtime and opened inside it) still fires normally.
pub fn decide_bedtime(
    window: BedtimeWindow,
    now_min: u32,
    last_sleep_fire: Option<Instant>,
    now: Instant,
    resumed_from_suspend: bool,
) -> BedtimeAction {
    if !window.enabled || !in_window(now_min, window.start_min, window.end_min) {
        return BedtimeAction::NotInWindow;
    }
    let should_fire = match last_sleep_fire {
        None => true,
        Some(t) => now.saturating_duration_since(t) >= Duration::from_secs(window.interval_secs),
    };
    if should_fire {
        if resumed_from_suspend && last_sleep_fire.is_some() {
            return BedtimeAction::ResetTimersOnly;
        }
        BedtimeAction::Fire
    } else {
        BedtimeAction::ResetTimersOnly
    }
}

/// Decide whether a due break should be delayed because the user is
/// mid-keystroke. Returns `true` while we should keep waiting and
/// `false` once either the user has paused typing OR the deferral cap
/// has been reached (so we don't postpone indefinitely).
///
/// `idle_secs` is `None` when idle detection is unavailable this tick
/// (e.g. Wayland exposes no portable HID-idle query, so the probe keeps
/// failing). We can't tell whether the user is typing then, so we don't
/// defer — otherwise the "is the user idle?" proxy is permanently 0 and
/// every break stalls for the full deferral cap before appearing (#67).
///
/// `deferred_since` is the instant the current defer-streak started,
/// or `None` if this is the first tick of the streak.
pub fn should_defer_for_typing(
    enabled: bool,
    idle_secs: Option<u64>,
    grace_secs: u64,
    deferred_since: Option<Instant>,
    max_deferral_secs: u64,
    now: Instant,
) -> bool {
    if !enabled || grace_secs == 0 {
        return false;
    }
    let Some(idle_secs) = idle_secs else {
        return false;
    };
    if idle_secs >= grace_secs {
        return false;
    }
    match deferred_since {
        None => true,
        Some(started) => now.duration_since(started) < Duration::from_secs(max_deferral_secs),
    }
}

/// How many times the user has postponed the current break of this
/// kind. `Sleep` always returns 0 (sleep prompts don't escalate).
pub fn postpone_counter(t: &BreakTimers, kind: BreakKind) -> u32 {
    match kind {
        BreakKind::Micro => t.micro_postpone_count,
        BreakKind::Long => t.long_postpone_count,
        BreakKind::Sleep => 0,
    }
}

/// Zero out the postpone counter for this kind. Called when a break
/// completes successfully — the user gets a fresh budget next time.
pub fn reset_postpone_counter(t: &mut BreakTimers, kind: BreakKind) {
    match kind {
        BreakKind::Micro => t.micro_postpone_count = 0,
        BreakKind::Long => t.long_postpone_count = 0,
        BreakKind::Sleep => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn in_window_normal() {
        assert!(in_window(540, 540, 1080));
        assert!(in_window(800, 540, 1080));
        assert!(in_window(1079, 540, 1080));
        assert!(!in_window(539, 540, 1080));
        assert!(!in_window(1080, 540, 1080));
        assert!(!in_window(0, 540, 1080));
    }

    #[test]
    fn in_window_wraps_midnight() {
        assert!(in_window(1320, 1320, 360));
        assert!(in_window(1439, 1320, 360));
        assert!(in_window(0, 1320, 360));
        assert!(in_window(359, 1320, 360));
        assert!(!in_window(360, 1320, 360));
        assert!(!in_window(720, 1320, 360));
    }

    #[test]
    fn in_window_empty_when_equal() {
        assert!(!in_window(0, 720, 720));
        assert!(!in_window(720, 720, 720));
        assert!(!in_window(1000, 720, 720));
    }

    #[test]
    fn current_minutes_in_range() {
        let m = current_minutes();
        assert!(m < 24 * 60);
    }

    #[test]
    fn current_weekday_in_range() {
        assert!(current_weekday() < 7);
    }

    const ALL_DAYS: u8 = 0b111_1111;

    #[test]
    fn work_window_active_all_days_matches_in_window() {
        // With every day enabled, the day gate is a no-op: the result is
        // exactly `in_window` regardless of `today`.
        for today in 0..7 {
            assert_eq!(
                work_window_active(800, 540, 1080, today, ALL_DAYS),
                in_window(800, 540, 1080),
            );
            assert_eq!(
                work_window_active(200, 540, 1080, today, ALL_DAYS),
                in_window(200, 540, 1080),
            );
        }
    }

    #[test]
    fn work_window_active_gates_on_enabled_day() {
        // 09:00–17:00, weekdays only (Mon..Fri = bits 0..4).
        let weekdays = 0b001_1111;
        // 10:00 Wednesday (today=2) → enabled.
        assert!(work_window_active(600, 540, 1080, 2, weekdays));
        // 10:00 Saturday (today=5) → disabled even though inside the hours.
        assert!(!work_window_active(600, 540, 1080, 5, weekdays));
        // 20:00 Wednesday → outside the hours regardless of the day.
        assert!(!work_window_active(1200, 540, 1080, 2, weekdays));
    }

    #[test]
    fn work_window_active_empty_mask_is_never_active() {
        for today in 0..7 {
            assert!(!work_window_active(600, 540, 1080, today, 0));
        }
    }

    #[test]
    fn work_window_active_wrap_gates_morning_on_starting_day() {
        // 22:00–06:00 window, enabled Friday only (bit 4).
        let friday = 1 << 4;
        // 23:00 Friday (today=4) → the window started today → active.
        assert!(work_window_active(1380, 1320, 360, 4, friday));
        // 02:00 Saturday (today=5) → the window started *Friday*, so it's
        // still Friday's shift → active even though Saturday is off.
        assert!(work_window_active(120, 1320, 360, 5, friday));
        // 02:00 Friday (today=4) belongs to Thursday's shift → Thursday is
        // off → not active.
        assert!(!work_window_active(120, 1320, 360, 4, friday));
        // 23:00 Saturday (today=5) starts Saturday's shift → off.
        assert!(!work_window_active(1380, 1320, 360, 5, friday));
    }

    #[test]
    fn work_window_active_wrap_sunday_to_monday_morning() {
        // 23:00–01:00 window, enabled Sunday only (bit 6). Monday 00:30
        // (today=0) belongs to Sunday's shift: (0 + 6) % 7 == 6 → active.
        let sunday = 1 << 6;
        assert!(work_window_active(30, 1380, 60, 0, sunday));
        // Sunday 00:30 (today=6) belongs to Saturday's shift → off.
        assert!(!work_window_active(30, 1380, 60, 6, sunday));
    }

    #[test]
    fn parse_hhmm_valid_two_digit_hour() {
        assert_eq!(parse_hhmm("00:00"), Some(0));
        assert_eq!(parse_hhmm("09:15"), Some(555));
        assert_eq!(parse_hhmm("12:30"), Some(12 * 60 + 30));
        assert_eq!(parse_hhmm("23:59"), Some(23 * 60 + 59));
    }

    #[test]
    fn parse_hhmm_single_digit_hour() {
        assert_eq!(parse_hhmm("8:05"), Some(8 * 60 + 5));
        assert_eq!(parse_hhmm("9:00"), Some(9 * 60));
    }

    #[test]
    fn parse_hhmm_trims_whitespace() {
        assert_eq!(parse_hhmm("  12:30 "), Some(12 * 60 + 30));
    }

    #[test]
    fn parse_hhmm_rejects_out_of_range() {
        assert_eq!(parse_hhmm("24:00"), None);
        assert_eq!(parse_hhmm("99:99"), None);
        assert_eq!(parse_hhmm("12:60"), None);
        assert_eq!(parse_hhmm("25:30"), None);
    }

    #[test]
    fn parse_hhmm_rejects_garbage() {
        assert_eq!(parse_hhmm(""), None);
        assert_eq!(parse_hhmm("abc"), None);
        assert_eq!(parse_hhmm("12:3"), None);
        assert_eq!(parse_hhmm(":30"), None);
        assert_eq!(parse_hhmm("12:"), None);
        assert_eq!(parse_hhmm("12-30"), None);
        assert_eq!(parse_hhmm("12:30:00"), None);
    }

    #[test]
    fn should_fire_fixed_now_first_fire() {
        assert!(should_fire_fixed_now("2026-03-08", 750, None));
    }

    #[test]
    fn should_fire_fixed_now_same_day_same_minute_dedupes() {
        let last = ("2026-03-08".to_string(), 750u32);
        assert!(!should_fire_fixed_now("2026-03-08", 750, Some(&last)));
    }

    #[test]
    fn should_fire_fixed_now_same_day_different_minute_refires() {
        let last = ("2026-03-08".to_string(), 750u32);
        assert!(should_fire_fixed_now("2026-03-08", 751, Some(&last)));
        assert!(should_fire_fixed_now("2026-03-08", 1020, Some(&last)));
    }

    #[test]
    fn should_fire_fixed_now_new_day_refires_same_minute() {
        // Crossing midnight resets the dedupe so the next day's first
        // hit of the fixed-time slot fires.
        let last = ("2026-03-08".to_string(), 750u32);
        assert!(should_fire_fixed_now("2026-03-09", 750, Some(&last)));
    }

    #[test]
    fn should_fire_fixed_now_dst_fall_back_does_not_double_fire() {
        // North-American "fall back": at 02:00 local the clock jumps
        // back to 01:00 so wall-clock minutes 60..119 are traversed
        // twice. Same-day dedupe must keep a single fire per (date,
        // minute) — otherwise every fixed-time slot in that hour would
        // fire twice on DST end.
        let last = ("2026-11-01".to_string(), 90u32); // 01:30 first pass
        assert!(!should_fire_fixed_now("2026-11-01", 90, Some(&last)));
    }

    #[test]
    fn should_fire_fixed_now_dst_spring_forward_does_not_resurrect_skipped_minute() {
        // "Spring forward": 02:00 → 03:00, so minutes 120..179 (02:00–02:59)
        // are skipped entirely. The minute simply never appears on the
        // wall clock, so dedupe doesn't need a fix for it — but if a
        // previous day's fire was at 02:30 the next day at 02:30 (a real
        // minute on a non-DST day) should still re-fire, which it does
        // because the date key differs.
        let last = ("2026-03-08".to_string(), 150u32); // 02:30 the day before DST
        assert!(should_fire_fixed_now("2026-03-09", 180, Some(&last)));
    }

    #[test]
    fn fixed_dedupe_state_clears_on_break_timers_new() {
        let t = BreakTimers::new();
        assert!(t.last_micro_fixed_fire.is_none());
        assert!(t.last_long_fixed_fire.is_none());
    }

    #[test]
    fn typing_defer_disabled_returns_false() {
        let now = Instant::now();
        assert!(!should_defer_for_typing(false, Some(0), 10, None, 60, now));
    }

    #[test]
    fn typing_defer_zero_grace_returns_false() {
        let now = Instant::now();
        assert!(!should_defer_for_typing(true, Some(0), 0, None, 60, now));
    }

    #[test]
    fn typing_defer_idle_unavailable_does_not_defer() {
        // Wayland (#67): no idle reading this tick. We can't tell the
        // user is typing, so we must fire rather than stall for the cap.
        let now = Instant::now();
        assert!(!should_defer_for_typing(true, None, 10, None, 60, now));
        // Even mid-streak, an unavailable reading releases the defer.
        let started = Instant::now();
        assert!(!should_defer_for_typing(
            true,
            None,
            10,
            Some(started),
            60,
            started + Duration::from_secs(5)
        ));
    }

    #[test]
    fn typing_defer_when_actively_typing_first_tick() {
        let now = Instant::now();
        assert!(should_defer_for_typing(true, Some(1), 10, None, 60, now));
    }

    #[test]
    fn typing_defer_idle_above_grace_does_not_defer() {
        let now = Instant::now();
        assert!(!should_defer_for_typing(true, Some(10), 10, None, 60, now));
        assert!(!should_defer_for_typing(true, Some(30), 10, None, 60, now));
    }

    #[test]
    fn typing_defer_within_cap_keeps_deferring() {
        // Anchor at `started` and derive `now = started + 30s`; never
        // subtract from `Instant::now()` (panics on Windows when the
        // monotonic clock is younger than the offset).
        let started = Instant::now();
        let now = started + Duration::from_secs(30);
        assert!(should_defer_for_typing(
            true,
            Some(1),
            10,
            Some(started),
            60,
            now
        ));
    }

    #[test]
    fn typing_defer_cap_reached_fires_anyway() {
        let started = Instant::now();
        let now = started + Duration::from_secs(60);
        assert!(!should_defer_for_typing(
            true,
            Some(1),
            10,
            Some(started),
            60,
            now
        ));
        let older = Instant::now();
        let now_later = older + Duration::from_secs(120);
        assert!(!should_defer_for_typing(
            true,
            Some(1),
            10,
            Some(older),
            60,
            now_later
        ));
    }

    #[test]
    fn postpone_counter_reads_per_kind() {
        let mut t = BreakTimers::new();
        t.micro_postpone_count = 2;
        t.long_postpone_count = 5;
        assert_eq!(postpone_counter(&t, BreakKind::Micro), 2);
        assert_eq!(postpone_counter(&t, BreakKind::Long), 5);
        assert_eq!(postpone_counter(&t, BreakKind::Sleep), 0);
    }

    #[test]
    fn reset_postpone_counter_only_clears_target_kind() {
        let mut t = BreakTimers::new();
        t.micro_postpone_count = 3;
        t.long_postpone_count = 4;
        reset_postpone_counter(&mut t, BreakKind::Micro);
        assert_eq!(t.micro_postpone_count, 0);
        assert_eq!(t.long_postpone_count, 4);
        reset_postpone_counter(&mut t, BreakKind::Long);
        assert_eq!(t.long_postpone_count, 0);
    }

    #[test]
    fn clear_last_break_returns_whether_cleared() {
        let mut t = BreakTimers::new();
        assert!(!clear_last_break(&mut t));
        t.last_skipped_or_postponed = Some((BreakKind::Long, Instant::now()));
        assert!(clear_last_break(&mut t));
        assert!(t.last_skipped_or_postponed.is_none());
        assert!(!clear_last_break(&mut t));
    }

    #[test]
    fn reset_timers_keep_sleep_preserves_last_sleep_and_active_break() {
        let mut t = BreakTimers::new();
        let sleep_at = Instant::now();
        t.last_sleep = Some(sleep_at);
        t.active_break = Some(BreakKind::Long);
        t.micro_warned = true;
        t.long_warned = true;
        t.micro_postpone_count = 2;
        t.long_postpone_count = 3;
        t.micro_deferred_since = Some(Instant::now());
        t.long_deferred_since = Some(Instant::now());

        reset_timers_keep_sleep(&mut t);

        assert_eq!(t.last_sleep, Some(sleep_at));
        assert_eq!(t.active_break, Some(BreakKind::Long));
        assert!(!t.micro_warned);
        assert!(!t.long_warned);
        assert_eq!(t.micro_postpone_count, 0);
        assert_eq!(t.long_postpone_count, 0);
        assert!(t.micro_deferred_since.is_none());
        assert!(t.long_deferred_since.is_none());
    }

    #[test]
    fn reset_timers_keep_sleep_clears_with_no_sleep() {
        let mut t = BreakTimers::new();
        assert!(t.last_sleep.is_none());
        t.micro_warned = true;
        reset_timers_keep_sleep(&mut t);
        assert!(t.last_sleep.is_none());
        assert!(!t.micro_warned);
    }

    #[test]
    fn reanchor_on_resume_moves_both_interval_clocks_to_now() {
        // The #134 core: after an hour paused the anchors are an hour
        // stale; re-anchoring to the resume instant means no interval is
        // due (full period remaining) on the very next tick.
        let paused_at = Instant::now();
        let mut t = BreakTimers::new();
        t.last_micro = paused_at;
        t.last_long = paused_at;
        let resumed_at = paused_at + Duration::from_secs(3_600);

        reanchor_intervals_on_resume(&mut t, resumed_at);

        assert_eq!(t.last_micro, resumed_at);
        assert_eq!(t.last_long, resumed_at);
        assert!(!interval_break_due(
            true,
            true,
            t.last_micro,
            1_200,
            false,
            resumed_at
        ));
        assert!(!interval_break_due(
            true,
            true,
            t.last_long,
            1_800,
            false,
            resumed_at
        ));
    }

    #[test]
    fn reanchor_on_resume_next_due_is_now_plus_interval() {
        let resumed_at = Instant::now();
        let mut t = BreakTimers::new();
        reanchor_intervals_on_resume(&mut t, resumed_at);
        let interval = 1_200u64;
        assert!(!interval_break_due(
            true,
            true,
            t.last_micro,
            interval,
            false,
            resumed_at + Duration::from_secs(interval - 1)
        ));
        assert!(interval_break_due(
            true,
            true,
            t.last_micro,
            interval,
            false,
            resumed_at + Duration::from_secs(interval)
        ));
    }

    #[test]
    fn reanchor_on_resume_clears_warn_and_deferral_and_postpone() {
        let mut t = BreakTimers::new();
        t.micro_warned = true;
        t.long_warned = true;
        t.micro_deferred_since = Some(Instant::now());
        t.long_deferred_since = Some(Instant::now());
        t.micro_postpone_count = 4;
        t.long_postpone_count = 7;

        reanchor_intervals_on_resume(&mut t, Instant::now());

        assert!(!t.micro_warned);
        assert!(!t.long_warned);
        assert!(t.micro_deferred_since.is_none());
        assert!(t.long_deferred_since.is_none());
        assert_eq!(t.micro_postpone_count, 0);
        assert_eq!(t.long_postpone_count, 0);
    }

    #[test]
    fn reanchor_on_resume_preserves_sleep_and_fixed_time_state() {
        // Bedtime cadence and fixed-time dedupe keys must survive a resume
        // so resuming doesn't re-fire a sleep prompt or a fixed-time slot
        // already handled today.
        let sleep_at = Instant::now();
        let mut t = BreakTimers::new();
        t.last_sleep = Some(sleep_at);
        t.active_break = Some(BreakKind::Sleep);
        t.last_micro_fixed_fire = Some(("2026-06-05".into(), 540));
        t.last_long_fixed_fire = Some(("2026-06-05".into(), 600));

        reanchor_intervals_on_resume(&mut t, Instant::now());

        assert_eq!(t.last_sleep, Some(sleep_at));
        assert_eq!(t.active_break, Some(BreakKind::Sleep));
        assert_eq!(t.last_micro_fixed_fire, Some(("2026-06-05".into(), 540)));
        assert_eq!(t.last_long_fixed_fire, Some(("2026-06-05".into(), 600)));
    }

    // `interval_break_due` — the workhorse decision for "is this break
    // due to fire on this tick?". Frozen clock here is built by
    // anchoring at `Instant::now()` and adding offsets to derive "later"
    // points — never subtracting from `now()`, because on Windows the
    // monotonic clock can underflow on a fresh runner (`Instant::sub`
    // panics if the result would be before boot).

    #[test]
    fn interval_due_fires_when_interval_elapsed() {
        let last = Instant::now();
        let now = last + Duration::from_secs(1200);
        assert!(interval_break_due(true, true, last, 1200, false, now));
    }

    #[test]
    fn interval_due_does_not_fire_before_interval() {
        let last = Instant::now();
        let now = last + Duration::from_secs(1199);
        assert!(!interval_break_due(true, true, last, 1200, false, now));
    }

    #[test]
    fn interval_due_respects_enabled_flag() {
        let last = Instant::now();
        let now = last + Duration::from_secs(2000);
        assert!(!interval_break_due(false, true, last, 1200, false, now));
    }

    #[test]
    fn interval_due_respects_mode_flag() {
        // mode "fixed" → mode_includes_interval is false → no fire even
        // though the interval has elapsed. Catches the regression where
        // a user switched to fixed-only and intervals kept firing.
        let last = Instant::now();
        let now = last + Duration::from_secs(2000);
        assert!(!interval_break_due(true, false, last, 1200, false, now));
    }

    #[test]
    fn interval_due_respects_idle_suppression() {
        let last = Instant::now();
        let now = last + Duration::from_secs(2000);
        assert!(!interval_break_due(true, true, last, 1200, true, now));
    }

    #[test]
    fn interval_due_handles_clock_skew_safely() {
        // `last_fire` in the future shouldn't panic — saturating_sub
        // returns zero, which fails the `>= interval` check.
        let now = Instant::now();
        let future = now + Duration::from_secs(60);
        assert!(!interval_break_due(true, true, future, 30, false, now));
    }

    // `prebreak_warn_due` — fires once per interval cycle, in a narrow
    // band before the break itself. The `already_warned` flag is the
    // dedupe gate.

    /// The "would warn if timing is right" gate: feature on, interval
    /// mode, not yet warned, not idle. Variants flip one field via
    /// struct-update syntax (`PrebreakGate { enabled: false, ..warn_gate() }`).
    fn warn_gate() -> PrebreakGate {
        PrebreakGate {
            enabled: true,
            mode_includes_interval: true,
            already_warned: false,
            idle_suppressed: false,
        }
    }

    #[test]
    fn prebreak_warn_fires_inside_lead_window() {
        // 50s before a 1200s break, lead is 60s → in the warn band.
        let last = Instant::now();
        let now = last + Duration::from_secs(1150);
        assert!(prebreak_warn_due(warn_gate(), last, 1200, 60, now));
    }

    #[test]
    fn prebreak_warn_does_not_fire_before_lead_window() {
        // 100s before a 1200s break, lead is 60s → outside warn band.
        let last = Instant::now();
        let now = last + Duration::from_secs(1100);
        assert!(!prebreak_warn_due(warn_gate(), last, 1200, 60, now));
    }

    #[test]
    fn prebreak_warn_does_not_fire_after_break_due() {
        // Once we've hit the interval the break itself fires — warning
        // shouldn't re-fire post-interval.
        let last = Instant::now();
        let now = last + Duration::from_secs(1200);
        assert!(!prebreak_warn_due(warn_gate(), last, 1200, 60, now));
        let way_late = Instant::now();
        let later_now = way_late + Duration::from_secs(1250);
        assert!(!prebreak_warn_due(
            warn_gate(),
            way_late,
            1200,
            60,
            later_now
        ));
    }

    #[test]
    fn prebreak_warn_dedupes_via_already_warned() {
        let last = Instant::now();
        let now = last + Duration::from_secs(1150);
        assert!(!prebreak_warn_due(
            PrebreakGate {
                already_warned: true,
                ..warn_gate()
            },
            last,
            1200,
            60,
            now,
        ));
    }

    #[test]
    fn prebreak_warn_skips_when_disabled_or_idle() {
        let last = Instant::now();
        let now = last + Duration::from_secs(1150);
        assert!(!prebreak_warn_due(
            PrebreakGate {
                enabled: false,
                ..warn_gate()
            },
            last,
            1200,
            60,
            now,
        ));
        assert!(!prebreak_warn_due(
            PrebreakGate {
                mode_includes_interval: false,
                ..warn_gate()
            },
            last,
            1200,
            60,
            now,
        ));
        assert!(!prebreak_warn_due(
            PrebreakGate {
                idle_suppressed: true,
                ..warn_gate()
            },
            last,
            1200,
            60,
            now,
        ));
    }

    #[test]
    fn prebreak_warn_handles_lead_larger_than_interval() {
        // saturating_sub means warn_at = 0 — the warning fires
        // immediately after the previous break. Unusual config but must
        // not panic or warn forever.
        let last = Instant::now();
        let now = last + Duration::from_secs(10);
        assert!(prebreak_warn_due(warn_gate(), last, 60, 600, now));
    }

    /// Drive a full interval cycle one simulated 1Hz tick at a time the
    /// way the run loop does: at each tick, evaluate `prebreak_warn_due`
    /// against the live `micro_warned` flag (setting it when it fires),
    /// then `interval_break_due`; when the break is due, apply
    /// `record_scheduled_fire` (which clears the warn flag) and re-anchor.
    /// Returns the elapsed-seconds at every tick the warn fired across two
    /// back-to-back cycles, so the test can assert the warning surfaces
    /// exactly once per cycle and re-arms for the next break (#135).
    fn simulate_warn_ticks(interval_secs: u64, lead_secs: u64, cycles: u64) -> Vec<u64> {
        let mut t = BreakTimers::new();
        let base = Instant::now();
        let mut last_anchor = 0u64;
        let mut fired_at = Vec::new();
        let total = interval_secs * cycles;
        for sec in 1..=total {
            let now = base + Duration::from_secs(sec);
            let gate = PrebreakGate {
                enabled: true,
                mode_includes_interval: true,
                already_warned: t.micro_warned,
                idle_suppressed: false,
            };
            if prebreak_warn_due(gate, t.last_micro, interval_secs, lead_secs, now) {
                fired_at.push(sec - last_anchor);
                t.micro_warned = true;
            }
            if interval_break_due(true, true, t.last_micro, interval_secs, false, now) {
                record_scheduled_fire(
                    &mut t,
                    BreakKind::Micro,
                    BreakDelivery::Notification,
                    now,
                    None,
                );
                last_anchor = sec;
            }
        }
        fired_at
    }

    #[test]
    fn prebreak_warn_fires_once_per_cycle_and_rearms() {
        let fired = simulate_warn_ticks(900, 30, 3);
        assert_eq!(
            fired,
            vec![870, 870, 870],
            "warn must fire exactly once per cycle at the lead boundary and re-arm each cycle"
        );
    }

    #[test]
    fn prebreak_warn_survives_narrow_two_tick_window_each_cycle() {
        let fired = simulate_warn_ticks(5, 2, 4);
        assert_eq!(
            fired,
            vec![3, 3, 3, 3],
            "a 2-tick-wide warn band [3,5) must still fire once and re-arm every cycle"
        );
    }

    // `decide_bedtime` — three-way decision combining window membership
    // and per-window interval. The first tick of the window always
    // fires (`None` last_sleep).

    /// 22:00–06:00 window, 30-min re-prompt — the bulk of these cases.
    fn window_2206() -> BedtimeWindow {
        BedtimeWindow {
            enabled: true,
            start_min: 22 * 60,
            end_min: 6 * 60,
            interval_secs: 1800,
        }
    }

    /// 22:00–09:00 window, 5-min re-prompt — the resume-from-suspend cases.
    fn window_2209() -> BedtimeWindow {
        BedtimeWindow {
            enabled: true,
            start_min: 22 * 60,
            end_min: 9 * 60,
            interval_secs: 300,
        }
    }

    #[test]
    fn bedtime_not_in_window_returns_not_in_window() {
        let now = Instant::now();
        // 12:00, window 22:00–06:00
        assert_eq!(
            decide_bedtime(window_2206(), 12 * 60, None, now, false),
            BedtimeAction::NotInWindow
        );
    }

    #[test]
    fn bedtime_disabled_returns_not_in_window_even_in_range() {
        let now = Instant::now();
        assert_eq!(
            decide_bedtime(
                BedtimeWindow {
                    enabled: false,
                    ..window_2206()
                },
                23 * 60,
                None,
                now,
                false,
            ),
            BedtimeAction::NotInWindow
        );
    }

    #[test]
    fn bedtime_first_tick_of_window_fires() {
        let now = Instant::now();
        assert_eq!(
            decide_bedtime(window_2206(), 23 * 60, None, now, false),
            BedtimeAction::Fire
        );
    }

    #[test]
    fn bedtime_re_fires_after_interval() {
        let last = Instant::now();
        let now = last + Duration::from_secs(1800);
        assert_eq!(
            decide_bedtime(window_2206(), 23 * 60, Some(last), now, false),
            BedtimeAction::Fire
        );
    }

    #[test]
    fn bedtime_resets_only_inside_interval() {
        // Half-way through 30min interval — too soon to re-fire.
        let last = Instant::now();
        let now = last + Duration::from_secs(900);
        assert_eq!(
            decide_bedtime(window_2206(), 23 * 60, Some(last), now, false),
            BedtimeAction::ResetTimersOnly
        );
    }

    #[test]
    fn bedtime_window_handles_midnight_wrap() {
        let now = Instant::now();
        // 02:00 should still be in window 22:00–06:00
        assert_eq!(
            decide_bedtime(window_2206(), 2 * 60, None, now, false),
            BedtimeAction::Fire
        );
    }

    #[test]
    fn bedtime_handles_clock_skew_safely() {
        // last_sleep in the future shouldn't panic; saturating means
        // elapsed == 0, so we land in ResetTimersOnly until time catches up.
        let now = Instant::now();
        let future = now + Duration::from_secs(60);
        assert_eq!(
            decide_bedtime(window_2206(), 23 * 60, Some(future), now, false),
            BedtimeAction::ResetTimersOnly
        );
    }

    #[test]
    fn bedtime_resume_from_suspend_does_not_refire_after_prior_prompt() {
        // Issue #61: prompt fired in the evening (last_sleep = Some), the
        // laptop slept for hours, and the user opens it still inside the
        // overnight window. The monotonic elapsed dwarfs the interval, so
        // the plain rule would fire — but the wake tick must stay quiet.
        let last = Instant::now();
        let now = last + Duration::from_secs(8 * 3600);
        assert_eq!(
            decide_bedtime(window_2209(), 8 * 60, Some(last), now, true),
            BedtimeAction::ResetTimersOnly
        );
        // Without the resume flag the same inputs would (wrongly, for the
        // user) re-fire — proving the flag is what suppresses it.
        assert_eq!(
            decide_bedtime(window_2209(), 8 * 60, Some(last), now, false),
            BedtimeAction::Fire
        );
    }

    #[test]
    fn bedtime_resume_into_first_entry_still_fires() {
        // Laptop closed before bedtime, opened inside the window: no prompt
        // has fired this window (last_sleep = None), so even a wake tick
        // should fire the first reminder.
        let now = Instant::now();
        assert_eq!(
            decide_bedtime(window_2209(), 23 * 60, None, now, true),
            BedtimeAction::Fire
        );
    }

    #[test]
    fn bedtime_resume_outside_window_is_noop() {
        // A wake well outside the window is still just NotInWindow.
        let now = Instant::now();
        assert_eq!(
            decide_bedtime(window_2209(), 12 * 60, Some(now), now, true),
            BedtimeAction::NotInWindow
        );
    }

    fn dirty_timers() -> BreakTimers {
        let mut t = BreakTimers::new();
        t.micro_warned = true;
        t.long_warned = true;
        t.micro_deferred_since = Some(Instant::now());
        t.long_deferred_since = Some(Instant::now());
        t
    }

    #[test]
    fn record_scheduled_fire_long_resets_both_clocks_and_records_fixed_key() {
        let mut t = dirty_timers();
        let now = Instant::now();
        record_scheduled_fire(
            &mut t,
            BreakKind::Long,
            BreakDelivery::Overlay,
            now,
            Some(("2026-06-02".into(), 540)),
        );
        assert_eq!(t.last_long, now);
        assert_eq!(t.last_micro, now);
        assert!(!t.long_warned);
        assert!(!t.micro_warned);
        assert_eq!(t.long_deferred_since, None);
        assert_eq!(t.micro_deferred_since, None);
        assert_eq!(t.active_break, Some(BreakKind::Long));
        assert_eq!(t.last_long_fixed_fire, Some(("2026-06-02".into(), 540)));
        assert_eq!(t.last_micro_fixed_fire, None);
    }

    #[test]
    fn record_scheduled_fire_micro_touches_only_micro_fields() {
        let mut t = dirty_timers();
        let before_long = t.last_long;
        let now = Instant::now();
        record_scheduled_fire(
            &mut t,
            BreakKind::Micro,
            BreakDelivery::Overlay,
            now,
            Some(("2026-06-02".into(), 600)),
        );
        assert_eq!(t.last_micro, now);
        assert_eq!(t.last_long, before_long);
        assert!(!t.micro_warned);
        assert!(
            t.long_warned,
            "micro fire must not clear the long warn flag"
        );
        assert_eq!(t.micro_deferred_since, None);
        assert!(t.long_deferred_since.is_some());
        assert_eq!(t.active_break, Some(BreakKind::Micro));
        assert_eq!(t.last_micro_fixed_fire, Some(("2026-06-02".into(), 600)));
        assert_eq!(t.last_long_fixed_fire, None);
    }

    #[test]
    fn record_scheduled_fire_interval_leaves_fixed_keys_untouched() {
        let mut t = BreakTimers::new();
        t.last_long_fixed_fire = Some(("2026-06-01".into(), 100));
        record_scheduled_fire(
            &mut t,
            BreakKind::Long,
            BreakDelivery::Overlay,
            Instant::now(),
            None,
        );
        assert_eq!(t.last_long_fixed_fire, Some(("2026-06-01".into(), 100)));
    }

    #[test]
    fn record_scheduled_fire_notification_does_not_stash_active_break() {
        let mut t = BreakTimers::new();
        record_scheduled_fire(
            &mut t,
            BreakKind::Micro,
            BreakDelivery::Notification,
            Instant::now(),
            None,
        );
        assert_eq!(t.active_break, None);
    }

    #[test]
    fn record_scheduled_fire_windowed_stashes_active_break() {
        let mut t = BreakTimers::new();
        record_scheduled_fire(
            &mut t,
            BreakKind::Long,
            BreakDelivery::Windowed,
            Instant::now(),
            None,
        );
        assert_eq!(t.active_break, Some(BreakKind::Long));
    }

    #[test]
    fn record_scheduled_fire_sleep_is_a_noop_for_per_kind_state() {
        let mut t = dirty_timers();
        let before_micro = t.last_micro;
        let before_long = t.last_long;
        record_scheduled_fire(
            &mut t,
            BreakKind::Sleep,
            BreakDelivery::Notification,
            Instant::now(),
            None,
        );
        assert_eq!(t.last_micro, before_micro);
        assert_eq!(t.last_long, before_long);
        assert!(t.micro_warned);
        assert!(t.long_warned);
        assert_eq!(t.active_break, None);
    }
}
