use serde::Serialize;

/// In-session break counters surfaced to the Insights tab.
///
/// Reset every time the scheduler starts; the persistent stats live in
/// the JSONL event log under `crate::stats`. `postponed` counts each
/// postpone, not unique breaks.
#[derive(Debug, Clone, Default, Serialize)]
pub struct BreakStats {
    pub taken: u32,
    pub skipped: u32,
    pub postponed: u32,
}

impl BreakStats {
    /// Skip ratio in `[0, 1]`, used to drive the overlay's "break
    /// health" vignette: 0 when every offered break is taken, 1 when
    /// every offered break is dismissed.
    pub fn intensity(&self) -> f32 {
        let total = self.taken + self.skipped;
        if total == 0 {
            return 0.0;
        }
        (self.skipped as f32 / total as f32).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn break_stats_intensity() {
        let mut s = BreakStats::default();
        assert_eq!(s.intensity(), 0.0);
        s.taken = 4;
        s.skipped = 1;
        let i = s.intensity();
        assert!((i - 0.2).abs() < 0.001);
        s.skipped = 10;
        s.taken = 0;
        assert_eq!(s.intensity(), 1.0);
    }
}
