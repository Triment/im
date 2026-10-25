//! Congestion control: receiver-side delay-gradient bandwidth estimator (GCC style)
//! plus a loss-based controller. Timestamps are the sender's capture ms from the
//! packet header, arrivals are local ms.

use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Usage {
    Normal,
    Overusing,
    Underusing,
}

/// Sliding-window throughput meter.
pub struct RateMeter {
    window_ms: f64,
    samples: VecDeque<(f64, usize)>,
    bytes: usize,
}

impl RateMeter {
    pub fn new(window_ms: f64) -> Self {
        Self { window_ms, samples: VecDeque::new(), bytes: 0 }
    }
    pub fn add(&mut self, now_ms: f64, size: usize) {
        self.samples.push_back((now_ms, size));
        self.bytes += size;
        self.expire(now_ms);
    }
    fn expire(&mut self, now_ms: f64) {
        while let Some(&(t, s)) = self.samples.front() {
            if now_ms - t > self.window_ms {
                self.bytes -= s;
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }
    pub fn bps(&mut self, now_ms: f64) -> f64 {
        self.expire(now_ms);
        self.bytes as f64 * 8.0 * 1000.0 / self.window_ms
    }
}

struct Group {
    first_ts: u32,
    last_ts: u32,
    last_arrival: f64,
}

pub struct DelayBwe {
    cur: Option<Group>,
    prev: Option<Group>,
    num_deltas: u32,
    accumulated: f64,
    smoothed: f64,
    window: VecDeque<(f64, f64)>,
    first_arrival: Option<f64>,
    threshold: f64,
    last_threshold_update: f64,
    overuse_count: u32,
    usage: Usage,
    rate: f64,
    min_rate: f64,
    max_rate: f64,
    last_update: f64,
    hold_until: f64,
    meter: RateMeter,
}

impl DelayBwe {
    pub fn new(start_bps: u32, min_bps: u32, max_bps: u32) -> Self {
        Self {
            cur: None,
            prev: None,
            num_deltas: 0,
            accumulated: 0.0,
            smoothed: 0.0,
            window: VecDeque::new(),
            first_arrival: None,
            threshold: 12.5,
            last_threshold_update: 0.0,
            overuse_count: 0,
            usage: Usage::Normal,
            rate: start_bps as f64,
            min_rate: min_bps as f64,
            max_rate: max_bps as f64,
            last_update: 0.0,
            hold_until: 0.0,
            meter: RateMeter::new(1000.0),
        }
    }

    pub fn usage(&self) -> Usage {
        self.usage
    }
    pub fn estimate_bps(&self) -> u32 {
        self.rate as u32
    }
    pub fn received_bps(&mut self, now_ms: f64) -> f64 {
        self.meter.bps(now_ms)
    }

    pub fn on_packet(&mut self, arrival_ms: f64, ts: u32, size: usize) {
        self.meter.add(arrival_ms, size);
        self.first_arrival.get_or_insert(arrival_ms);
        match &mut self.cur {
            None => self.cur = Some(Group { first_ts: ts, last_ts: ts, last_arrival: arrival_ms }),
            Some(g) => {
                let diff = ts.wrapping_sub(g.first_ts) as i32;
                if diff.abs() <= 5 {
                    g.last_arrival = g.last_arrival.max(arrival_ms);
                    if (ts.wrapping_sub(g.last_ts) as i32) > 0 {
                        g.last_ts = ts;
                    }
                } else if diff < 0 {
                    // reordered across groups, ignore for gradient purposes
                } else {
                    let done = self.cur.take().unwrap();
                    if let Some(p) = &self.prev {
                        let send_delta = done.last_ts.wrapping_sub(p.last_ts) as i32 as f64;
                        let arr_delta = done.last_arrival - p.last_arrival;
                        self.update_trend(arr_delta - send_delta, done.last_arrival);
                    }
                    self.prev = Some(done);
                    self.cur = Some(Group { first_ts: ts, last_ts: ts, last_arrival: arrival_ms });
                }
            }
        }
        self.update_rate(arrival_ms);
    }

    fn update_trend(&mut self, delta: f64, arrival: f64) {
        self.num_deltas = (self.num_deltas + 1).min(60);
        self.accumulated += delta;
        self.smoothed = 0.9 * self.smoothed + 0.1 * self.accumulated;
        let t0 = self.first_arrival.unwrap_or(0.0);
        self.window.push_back((arrival - t0, self.smoothed));
        if self.window.len() > 20 {
            self.window.pop_front();
        }
        if self.window.len() < 20 {
            return;
        }
        let n = self.window.len() as f64;
        let mx = self.window.iter().map(|p| p.0).sum::<f64>() / n;
        let my = self.window.iter().map(|p| p.1).sum::<f64>() / n;
        let (mut num, mut den) = (0.0, 0.0);
        for &(x, y) in &self.window {
            num += (x - mx) * (y - my);
            den += (x - mx) * (x - mx);
        }
        let slope = if den > 0.0 { num / den } else { 0.0 };
        let modified = self.num_deltas as f64 * slope * 4.0;

        if modified > self.threshold {
            self.overuse_count += 1;
            if self.overuse_count >= 2 {
                self.usage = Usage::Overusing;
            }
        } else if modified < -self.threshold {
            self.overuse_count = 0;
            self.usage = Usage::Underusing;
        } else {
            self.overuse_count = 0;
            self.usage = Usage::Normal;
        }
        // adaptive threshold
        let dt = (arrival - self.last_threshold_update).clamp(0.0, 100.0);
        self.last_threshold_update = arrival;
        if modified.abs() < self.threshold + 15.0 {
            let k = if modified.abs() < self.threshold { 0.039 } else { 0.0087 };
            self.threshold = (self.threshold + k * (modified.abs() - self.threshold) * dt).clamp(6.0, 600.0);
        }
    }

    fn update_rate(&mut self, now: f64) {
        let dt = ((now - self.last_update) / 1000.0).clamp(0.0, 1.0);
        if self.last_update == 0.0 {
            self.last_update = now;
            return;
        }
        let acked = self.meter.bps(now);
        match self.usage {
            Usage::Overusing => {
                if now >= self.hold_until && acked > 0.0 {
                    self.rate = (0.85 * acked).min(self.rate).max(self.min_rate);
                    self.hold_until = now + 300.0;
                    self.usage = Usage::Underusing; // drain queue before probing up again
                    self.overuse_count = 0;
                }
            }
            Usage::Normal => {
                if now >= self.hold_until {
                    let grown = self.rate * 1.08f64.powf(dt);
                    let cap = (acked * 1.5 + 10_000.0).max(self.rate);
                    self.rate = grown.min(cap).min(self.max_rate);
                }
            }
            Usage::Underusing => {}
        }
        self.last_update = now;
    }
}

/// Sender-side loss based controller.
pub struct LossController {
    rate: f64,
    min: f64,
    max: f64,
}

impl LossController {
    pub fn new(start_bps: u32, min_bps: u32, max_bps: u32) -> Self {
        Self { rate: start_bps as f64, min: min_bps as f64, max: max_bps as f64 }
    }
    /// `loss` in 0.0..=1.0, called once per feedback interval.
    pub fn on_loss(&mut self, loss: f32) -> u32 {
        let l = loss as f64;
        if l > 0.10 {
            self.rate *= 1.0 - 0.5 * l;
        } else if l < 0.02 {
            self.rate *= 1.05;
        }
        self.rate = self.rate.clamp(self.min, self.max);
        self.rate as u32
    }
    pub fn rate_bps(&self) -> u32 {
        self.rate as u32
    }
}

/// Combines receiver estimate and loss controller into a video target (audio is reserved first).
pub fn video_target_bps(remb: u32, loss_rate: u32, audio_bps: u32, min_video: u32, max_video: u32) -> u32 {
    remb.min(loss_rate).saturating_sub(audio_bps).clamp(min_video, max_video)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feed 30fps frames of 5 packets. `link_bps` is the bottleneck; frames are sent at `send_bps`.
    fn run(send_bps: f64, link_bps: f64, secs: usize) -> DelayBwe {
        let mut bwe = DelayBwe::new(1_000_000, 100_000, 20_000_000);
        let frame_interval = 1000.0 / 30.0;
        let frame_bytes = send_bps / 8.0 / 30.0;
        let mut queue_free_at = 0.0f64; // bottleneck link busy-until (ms)
        for f in 0..(30 * secs) {
            let send_t = f as f64 * frame_interval;
            let pkt_bytes = frame_bytes / 5.0;
            for _ in 0..5 {
                let tx = pkt_bytes * 8.0 / link_bps * 1000.0;
                let start = queue_free_at.max(send_t);
                queue_free_at = start + tx;
                bwe.on_packet(queue_free_at + 20.0, send_t as u32, pkt_bytes as usize);
            }
        }
        bwe
    }

    #[test]
    fn stable_link_no_overuse_and_grows() {
        let mut bwe = run(2_000_000.0, 10_000_000.0, 10);
        assert_eq!(bwe.usage(), Usage::Normal);
        assert!(bwe.estimate_bps() >= 1_000_000);
        assert!(bwe.received_bps(10_000.0) < 3_000_000.0);
    }

    #[test]
    fn congestion_detected_and_rate_reduced() {
        // sending 4 Mbps into a 2 Mbps bottleneck builds a queue: delay gradient rises
        let mut bwe = DelayBwe::new(4_000_000, 100_000, 20_000_000);
        let frame_interval = 1000.0 / 30.0;
        let pkt = 4_000_000.0 / 8.0 / 30.0 / 5.0;
        let mut free = 0.0f64;
        let mut saw_overuse = false;
        for f in 0..300 {
            let t = f as f64 * frame_interval;
            for _ in 0..5 {
                let start = free.max(t);
                free = start + pkt * 8.0 / 2_000_000.0 * 1000.0;
                bwe.on_packet(free + 20.0, t as u32, pkt as usize);
                saw_overuse |= bwe.usage() == Usage::Overusing || bwe.usage() == Usage::Underusing;
            }
        }
        assert!(saw_overuse, "gradient never flagged congestion");
        assert!(bwe.estimate_bps() < 3_600_000, "estimate {}", bwe.estimate_bps());
    }

    #[test]
    fn loss_controller() {
        let mut l = LossController::new(2_000_000, 100_000, 10_000_000);
        let after_loss = l.on_loss(0.2);
        assert!(after_loss < 2_000_000);
        let mut r = after_loss;
        for _ in 0..30 {
            r = l.on_loss(0.0);
        }
        assert!(r > after_loss);
        assert_eq!(video_target_bps(3_000_000, 2_000_000, 256_000, 150_000, 8_000_000), 1_744_000);
    }
}
