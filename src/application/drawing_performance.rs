//! Bounded, transient timing counters; never record diagnostics into document history.
use std::time::Duration;

#[derive(Default)]
pub(crate) struct Timings {
    count: u64,
    total_us: u64,
    max_us: u64,
    over_8ms: u64,
}
impl Timings {
    pub(crate) fn observe(&mut self, elapsed: Duration) {
        let us = elapsed.as_micros().min(u64::MAX as u128) as u64;
        self.count = self.count.saturating_add(1);
        self.total_us = self.total_us.saturating_add(us);
        self.max_us = self.max_us.max(us);
        self.over_8ms = self.over_8ms.saturating_add(u64::from(us >= 8_000));
    }
    pub(crate) fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"count":self.count,"total_us":self.total_us,
            "max_us":self.max_us,"over_8ms":self.over_8ms})
    }
}

#[derive(Default)]
pub(crate) struct DrawingPerformance {
    pub(crate) tool_input: Timings,
    pub(crate) replay_flush: Timings,
}
impl DrawingPerformance {
    pub(crate) fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"tool_input":self.tool_input.snapshot(),
            "replay_flush":self.replay_flush.snapshot(),
            "scope":"Cumulative Session wall time, not end-to-end tablet latency"})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counters_are_bounded_and_count_spikes_without_retaining_samples() {
        let mut timings = Timings::default();
        for us in [25, 7_999, 8_000, 12_000] {
            timings.observe(Duration::from_micros(us));
        }
        let result = timings.snapshot();
        assert_eq!(result["count"], 4);
        assert_eq!(result["total_us"], 28_024);
        assert_eq!(result["max_us"], 12_000);
        assert_eq!(result["over_8ms"], 2);
    }
}
