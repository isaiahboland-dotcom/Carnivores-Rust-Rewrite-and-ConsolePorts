//! Summary statistics and process measurements. A value that cannot be
//! measured is reported as unavailable with a reason, never as zero.

use serde_json::{json, Value};

/// A measured number, or why there is none.
pub fn unavailable(reason: &str) -> Value {
    json!({ "unavailable": reason })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stats {
    pub count: usize,
    pub mean: f64,
    pub median: f64,
    pub p95: f64,
    pub p99: f64,
    pub max: f64,
    /// Mean of the slowest 1% (at least one sample).
    pub worst_1pct_mean: f64,
}

/// Nearest-rank percentile of sorted `v` (`p` in 0..=100).
fn percentile(sorted: &[f64], p: f64) -> f64 {
    let rank = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

impl Stats {
    pub fn of(values: &[f64]) -> Option<Self> {
        if values.is_empty() {
            return None;
        }
        let mut s = values.to_vec();
        s.sort_by(f64::total_cmp);
        let n = s.len();
        let worst = (n / 100).max(1);
        Some(Self {
            count: n,
            mean: s.iter().sum::<f64>() / n as f64,
            median: percentile(&s, 50.0),
            p95: percentile(&s, 95.0),
            p99: percentile(&s, 99.0),
            max: s[n - 1],
            worst_1pct_mean: s[n - worst..].iter().sum::<f64>() / worst as f64,
        })
    }

    /// Times in ms as a JSON object.
    pub fn json_ms(&self) -> Value {
        json!({
            "samples": self.count,
            "mean_ms": self.mean,
            "median_ms": self.median,
            "p95_ms": self.p95,
            "p99_ms": self.p99,
            "max_ms": self.max,
        })
    }

    /// Frame times in ms as FPS figures: the mean rate, and the "1% low"
    /// (the rate of the slowest 1% of frames).
    pub fn json_fps(&self) -> Value {
        json!({
            "average_fps": 1000.0 / self.mean,
            "one_percent_low_fps": 1000.0 / self.worst_1pct_mean,
            "frame_time": self.json_ms(),
        })
    }
}

/// Resident and peak resident memory of this process, bytes (Linux).
pub fn process_memory() -> (Value, Value) {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        let why = "no /proc/self/status on this platform";
        return (unavailable(why), unavailable(why));
    };
    let field = |name: &str| {
        status
            .lines()
            .find_map(|l| l.strip_prefix(name))
            .and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok())
            .map_or_else(
                || unavailable(&format!("{name} missing")),
                |kb| json!(kb * 1024),
            )
    };
    (field("VmRSS:"), field("VmHWM:"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_by_nearest_rank() {
        let v: Vec<f64> = (1..=100).map(f64::from).collect();
        let s = Stats::of(&v).unwrap();
        assert_eq!((s.median, s.p95, s.p99, s.max), (50.0, 95.0, 99.0, 100.0));
        assert_eq!(s.worst_1pct_mean, 100.0);
        assert!((s.mean - 50.5).abs() < 1e-9);
        let one = Stats::of(&[4.0]).unwrap();
        assert_eq!((one.median, one.p99, one.worst_1pct_mean), (4.0, 4.0, 4.0));
        assert!(Stats::of(&[]).is_none());
    }

    #[test]
    fn fps_from_frame_times() {
        let mut v = vec![10.0; 99];
        v.push(40.0);
        let s = Stats::of(&v).unwrap();
        let j = s.json_fps();
        assert!((j["one_percent_low_fps"].as_f64().unwrap() - 25.0).abs() < 1e-9);
        assert!(j["average_fps"].as_f64().unwrap() < 100.0);
    }
}
