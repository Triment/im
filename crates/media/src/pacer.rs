//! Send pacer: spreads packets out in time instead of emitting a whole video frame as one burst.

use std::time::{Duration, Instant};

pub struct Pacer {
    rate_bps: f64,
    factor: f64,
    next: Instant,
}

impl Pacer {
    /// `factor` > 1 lets the pacer drain slightly faster than the target rate (WebRTC uses 2.5).
    pub fn new(rate_bps: u32, factor: f64, now: Instant) -> Self {
        Self { rate_bps: rate_bps.max(1) as f64, factor, next: now }
    }

    pub fn set_rate(&mut self, rate_bps: u32) {
        self.rate_bps = rate_bps.max(1) as f64;
    }

    /// Returns the instant at which a packet of `bytes` may be sent.
    pub fn schedule(&mut self, now: Instant, bytes: usize) -> Instant {
        let t = self.next.max(now);
        let secs = bytes as f64 * 8.0 / (self.rate_bps * self.factor);
        self.next = t + Duration::from_secs_f64(secs);
        t
    }

    /// Current queueing delay if a packet were scheduled now.
    pub fn backlog(&self, now: Instant) -> Duration {
        self.next.saturating_duration_since(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spreads_burst() {
        let t0 = Instant::now();
        // 8 Mbps, factor 1 => 1000 B takes 1ms
        let mut p = Pacer::new(8_000_000, 1.0, t0);
        let times: Vec<_> = (0..5).map(|_| p.schedule(t0, 1000)).collect();
        for (i, t) in times.iter().enumerate() {
            let d = t.duration_since(t0).as_micros() as i64;
            assert!((d - i as i64 * 1000).abs() < 50, "slot {i}: {d}us");
        }
        assert!(p.backlog(t0) >= Duration::from_micros(4900));
    }

    #[test]
    fn idle_gives_no_credit() {
        let t0 = Instant::now();
        let mut p = Pacer::new(8_000_000, 1.0, t0);
        let late = t0 + Duration::from_secs(10);
        assert_eq!(p.schedule(late, 1000), late);
    }
}
