//! Video frame fragmentation and reassembly.

pub const DEFAULT_MTU_PAYLOAD: usize = 1100;

/// Split an encoded frame into packet payloads; the bool marks the last fragment.
pub fn fragment(frame: &[u8], max_payload: usize) -> Vec<(&[u8], bool)> {
    if frame.is_empty() {
        return vec![(frame, true)];
    }
    let n = frame.len().div_ceil(max_payload);
    frame.chunks(max_payload).enumerate().map(|(i, c)| (c, i + 1 == n)).collect()
}

#[derive(Debug, PartialEq, Eq)]
pub struct Frame {
    pub ts: u32,
    pub keyframe: bool,
    pub data: Vec<u8>,
}

/// Reassembles in-order packets (post jitter buffer) into frames. A hole (reported via
/// `on_lost`) or an unterminated frame discards the frame and asks for a key frame.
#[derive(Default)]
pub struct FrameAssembler {
    cur_ts: Option<u32>,
    cur: Vec<u8>,
    keyframe: bool,
    corrupt: bool,
    pub need_keyframe: bool,
    pub dropped_frames: u64,
}

impl FrameAssembler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on_lost(&mut self) {
        self.corrupt = true;
    }

    pub fn push(&mut self, ts: u32, marker: bool, keyframe: bool, data: &[u8]) -> Option<Frame> {
        if self.cur_ts != Some(ts) {
            if self.cur_ts.is_some() && !self.cur.is_empty() {
                // previous frame never finished
                self.dropped_frames += 1;
                self.need_keyframe = true;
            }
            self.cur_ts = Some(ts);
            self.cur.clear();
            self.keyframe = false;
            // a loss reported before the first packet of this frame still taints it
        }
        self.cur.extend_from_slice(data);
        self.keyframe |= keyframe;
        if !marker {
            return None;
        }
        let frame_ok = !self.corrupt;
        let out = Frame { ts, keyframe: self.keyframe, data: std::mem::take(&mut self.cur) };
        self.cur_ts = None;
        self.corrupt = false;
        if frame_ok {
            if out.keyframe {
                self.need_keyframe = false;
            }
            Some(out)
        } else {
            self.dropped_frames += 1;
            self.need_keyframe = true;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let frame: Vec<u8> = (0..5000u32).map(|x| x as u8).collect();
        let frags = fragment(&frame, 1100);
        assert_eq!(frags.len(), 5);
        assert!(frags.last().unwrap().1);
        let mut a = FrameAssembler::new();
        let mut out = None;
        for (c, m) in frags {
            out = a.push(77, m, true, c);
        }
        let f = out.unwrap();
        assert_eq!(f.data, frame);
        assert!(f.keyframe);
    }

    #[test]
    fn loss_drops_frame_and_requests_keyframe() {
        let mut a = FrameAssembler::new();
        assert!(a.push(1, false, false, b"aa").is_none());
        a.on_lost();
        assert!(a.push(1, true, false, b"cc").is_none());
        assert!(a.need_keyframe);
        assert_eq!(a.dropped_frames, 1);
        // recovers on next keyframe
        let f = a.push(2, true, true, b"k").unwrap();
        assert!(f.keyframe);
        assert!(!a.need_keyframe);
    }
}
