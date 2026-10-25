//! Adaptive jitter buffer: reorders packets, absorbs jitter, declares losses.
//!
//! Playout delay = `min_delay + 3 * jitter` (RFC 3550 interarrival jitter), clamped to
//! `max_delay`. A packet is released once it is the next expected seq and has been
//! buffered for the target delay; a missing seq is declared lost once the oldest
//! buffered packet has waited the target delay.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq, Eq)]
pub enum Playout<T> {
    Packet { seq: u32, ts: u32, data: T },
    Lost(u32),
}

struct Entry<T> {
    data: T,
    ts: u32,
    arrival: Instant,
}

pub struct JitterBuffer<T> {
    buf: BTreeMap<u32, Entry<T>>,
    next: Option<u32>,
    min_delay: Duration,
    max_delay: Duration,
    jitter_ms: f64,
    last_transit: Option<f64>,
    epoch: Instant,
    pub late: u64,
    pub lost: u64,
    pub received: u64,
}

const MAX_BUFFERED: usize = 1024;

impl<T> JitterBuffer<T> {
    pub fn new(min_delay: Duration, max_delay: Duration, now: Instant) -> Self {
        Self {
            buf: BTreeMap::new(),
            next: None,
            min_delay,
            max_delay,
            jitter_ms: 0.0,
            last_transit: None,
            epoch: now,
            late: 0,
            lost: 0,
            received: 0,
        }
    }

    pub fn jitter_ms(&self) -> f64 {
        self.jitter_ms
    }

    pub fn target_delay(&self) -> Duration {
        let d = self.min_delay + Duration::from_secs_f64(3.0 * self.jitter_ms / 1000.0);
        d.min(self.max_delay)
    }

    /// Returns `false` if the packet was a duplicate or too late.
    pub fn push(&mut self, seq: u32, ts: u32, data: T, now: Instant) -> bool {
        let next = *self.next.get_or_insert(seq);
        if (seq.wrapping_sub(next) as i32) < 0 || self.buf.contains_key(&seq) {
            self.late += 1;
            return false;
        }
        self.received += 1;
        let transit = now.duration_since(self.epoch).as_secs_f64() * 1000.0 - ts as f64;
        if let Some(prev) = self.last_transit {
            let d = (transit - prev).abs();
            // ignore ts wrap / absurd values
            if d < 5000.0 {
                self.jitter_ms += (d - self.jitter_ms) / 16.0;
            }
        }
        self.last_transit = Some(transit);
        self.buf.insert(seq, Entry { data, ts, arrival: now });
        true
    }

    pub fn pop(&mut self, now: Instant) -> Option<Playout<T>> {
        let next = self.next?;
        let target = self.target_delay();
        if let Some(e) = self.buf.get(&next) {
            if now.duration_since(e.arrival) >= target {
                let e = self.buf.remove(&next).unwrap();
                self.next = Some(next.wrapping_add(1));
                return Some(Playout::Packet { seq: next, ts: e.ts, data: e.data });
            }
            return None;
        }
        let oldest = self.buf.values().map(|e| e.arrival).min()?;
        if now.duration_since(oldest) >= target || self.buf.len() >= MAX_BUFFERED {
            self.next = Some(next.wrapping_add(1));
            self.lost += 1;
            return Some(Playout::Lost(next));
        }
        None
    }

    /// Sequence numbers between the playout point and the highest received that are still missing.
    pub fn missing(&self) -> Vec<u32> {
        let (Some(next), Some((&hi, _))) = (self.next, self.buf.iter().next_back()) else {
            return vec![];
        };
        (next..hi).filter(|s| !self.buf.contains_key(s)).take(64).collect()
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn reorders() {
        let t0 = Instant::now();
        let mut jb = JitterBuffer::new(ms(20), ms(200), t0);
        jb.push(1, 0, "a", t0);
        jb.push(3, 40, "c", t0 + ms(1));
        jb.push(2, 20, "b", t0 + ms(2));
        let mut out = vec![];
        let mut t = t0;
        while out.len() < 3 {
            t += ms(5);
            if let Some(Playout::Packet { data, .. }) = jb.pop(t) {
                out.push(data);
            }
            assert!(t < t0 + ms(500));
        }
        assert_eq!(out, vec!["a", "b", "c"]);
    }

    #[test]
    fn declares_loss_after_wait() {
        let t0 = Instant::now();
        let mut jb = JitterBuffer::new(ms(20), ms(200), t0);
        jb.push(1, 0, 1, t0);
        jb.push(3, 40, 3, t0 + ms(40));
        assert!(matches!(jb.pop(t0 + ms(25)), Some(Playout::Packet { seq: 1, .. })));
        assert_eq!(jb.missing(), vec![2]);
        let mut got_lost = false;
        let mut t = t0 + ms(25);
        for _ in 0..20 {
            t += ms(5);
            match jb.pop(t) {
                Some(Playout::Lost(2)) => got_lost = true,
                Some(Playout::Packet { seq: 3, .. }) => break,
                _ => {}
            }
        }
        assert!(got_lost);
        assert_eq!(jb.lost, 1);
    }

    #[test]
    fn drops_late_and_dupes() {
        let t0 = Instant::now();
        let mut jb = JitterBuffer::new(ms(0), ms(200), t0);
        assert!(jb.push(10, 0, (), t0));
        assert!(!jb.push(10, 0, (), t0));
        assert!(jb.pop(t0 + ms(1)).is_some());
        assert!(!jb.push(9, 0, (), t0 + ms(2)));
    }

    #[test]
    fn delay_adapts_to_jitter() {
        let t0 = Instant::now();
        let mut jb = JitterBuffer::new(ms(20), ms(200), t0);
        for i in 0..200u32 {
            let wobble = if i % 2 == 0 { 0 } else { 30 };
            jb.push(i, i * 20, (), t0 + ms(i as u64 * 20 + wobble));
        }
        assert!(jb.jitter_ms() > 10.0);
        assert!(jb.target_delay() > ms(50));
        assert!(jb.target_delay() <= ms(200));
    }
}
