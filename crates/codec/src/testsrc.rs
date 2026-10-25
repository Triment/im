//! Synthetic sources for tests and the CLI demo.
use crate::audio::{AudioConfig, SAMPLE_RATE};
use crate::video::Yuv420;

/// Stereo sine mix (440 Hz left / 660 Hz right), frame index `n`.
pub fn sine_frame(n: usize, cfg: &AudioConfig) -> Vec<i16> {
    let per_ch = cfg.frame_len() / cfg.channels as usize;
    let mut v = Vec::with_capacity(cfg.frame_len());
    for i in 0..per_ch {
        let t = (n * per_ch + i) as f64 / SAMPLE_RATE as f64;
        for c in 0..cfg.channels {
            let f = if c == 0 { 440.0 } else { 660.0 };
            v.push(((t * f * std::f64::consts::TAU).sin() * 12000.0) as i16);
        }
    }
    v
}

/// Moving gradient with a bouncing square, so inter-frame prediction has something to do.
pub fn moving_frame(w: usize, h: usize, n: usize) -> Yuv420 {
    let mut f = Yuv420::new(w, h);
    for y in 0..h {
        for x in 0..w {
            f.y[y * w + x] = (((x + y) * 255 / (w + h)) as u8).wrapping_add((n * 2) as u8 / 4);
        }
    }
    let (sx, sy) = ((n * 7) % (w - 64), (n * 3) % (h - 64));
    for y in sy..sy + 64 {
        for x in sx..sx + 64 {
            f.y[y * w + x] = 235;
        }
    }
    f.u.fill(128);
    f.v.fill(128);
    f
}
