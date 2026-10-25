use crate::{err, CodecError, Result};
use openh264::{
    decoder::Decoder,
    encoder::{BitRate, Encoder, EncoderConfig, FrameRate, FrameType, IntraFramePeriod, RateControlMode, UsageType},
    formats::YUVSource,
    OpenH264API,
};

/// Planar I420 frame, tightly packed (strides = width, width/2, width/2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Yuv420 {
    pub width: usize,
    pub height: usize,
    pub y: Vec<u8>,
    pub u: Vec<u8>,
    pub v: Vec<u8>,
}

impl Yuv420 {
    pub fn new(width: usize, height: usize) -> Self {
        Self { width, height, y: vec![0; width * height], u: vec![128; width * height / 4], v: vec![128; width * height / 4] }
    }
}

impl YUVSource for Yuv420 {
    fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }
    fn strides(&self) -> (usize, usize, usize) {
        (self.width, self.width / 2, self.width / 2)
    }
    fn y(&self) -> &[u8] {
        &self.y
    }
    fn u(&self) -> &[u8] {
        &self.u
    }
    fn v(&self) -> &[u8] {
        &self.v
    }
}

pub struct EncodedFrame {
    /// Annex-B H.264 access unit.
    pub data: Vec<u8>,
    pub keyframe: bool,
}

/// Implemented by software and hardware encoders alike.
pub trait VideoEncoder: Send {
    fn name(&self) -> &'static str;
    /// Returns `None` if the encoder skipped the frame (rate control).
    fn encode(&mut self, frame: &Yuv420, force_key: bool) -> Result<Option<EncodedFrame>>;
    /// Change target bitrate without restarting the stream (no forced key frame).
    fn set_bitrate(&mut self, bps: u32) -> Result<()>;
}

pub trait VideoDecoder: Send {
    fn decode(&mut self, data: &[u8]) -> Result<Option<Yuv420>>;
}

#[repr(C)]
struct BitrateInfo {
    layer: i32,
    bitrate: i32,
}
const SPATIAL_LAYER_ALL: i32 = 3;

/// Portable software H.264 (OpenH264, constrained baseline).
pub struct H264Encoder {
    enc: Encoder,
}

impl H264Encoder {
    pub fn new(bitrate_bps: u32, fps: f32, gop_frames: u32) -> Result<Self> {
        let cfg = EncoderConfig::new()
            .bitrate(BitRate::from_bps(bitrate_bps))
            .max_frame_rate(FrameRate::from_hz(fps))
            .usage_type(UsageType::CameraVideoRealTime)
            .rate_control_mode(RateControlMode::Bitrate)
            .skip_frames(true)
            .intra_frame_period(IntraFramePeriod::from_num_frames(gop_frames));
        Ok(Self { enc: Encoder::with_api_config(OpenH264API::from_source(), cfg).map_err(err)? })
    }
}

impl VideoEncoder for H264Encoder {
    fn name(&self) -> &'static str {
        "openh264 (software)"
    }

    fn encode(&mut self, frame: &Yuv420, force_key: bool) -> Result<Option<EncodedFrame>> {
        if force_key {
            self.enc.force_intra_frame();
        }
        let bs = self.enc.encode(frame).map_err(err)?;
        let keyframe = matches!(bs.frame_type(), FrameType::IDR | FrameType::I);
        if matches!(bs.frame_type(), FrameType::Skip | FrameType::Invalid) {
            return Ok(None);
        }
        Ok(Some(EncodedFrame { data: bs.to_vec(), keyframe }))
    }

    fn set_bitrate(&mut self, bps: u32) -> Result<()> {
        let mut info = BitrateInfo { layer: SPATIAL_LAYER_ALL, bitrate: bps as i32 };
        // SAFETY: SBitrateInfo is {int iLayer; int iBitrate}; ENCODER_OPTION_BITRATE with layer ALL is
        // the documented way to retarget the rate controller on a live encoder.
        let rc = unsafe { self.enc.raw_api().set_option(openh264_sys2::ENCODER_OPTION_BITRATE, &mut info as *mut _ as *mut std::ffi::c_void) };
        if rc != 0 {
            return Err(CodecError(format!("set bitrate failed: {rc}")));
        }
        Ok(())
    }
}

pub struct H264Decoder {
    dec: Decoder,
}

impl H264Decoder {
    pub fn new() -> Result<Self> {
        Ok(Self { dec: Decoder::new().map_err(err)? })
    }
}

impl VideoDecoder for H264Decoder {
    fn decode(&mut self, data: &[u8]) -> Result<Option<Yuv420>> {
        let Some(d) = self.dec.decode(data).map_err(err)? else { return Ok(None) };
        let (w, h) = d.dimensions();
        let (sy, su, sv) = d.strides();
        let copy = |src: &[u8], stride: usize, cols: usize, rows: usize| -> Vec<u8> {
            let mut out = Vec::with_capacity(cols * rows);
            for r in 0..rows {
                out.extend_from_slice(&src[r * stride..r * stride + cols]);
            }
            out
        };
        Ok(Some(Yuv420 { width: w, height: h, y: copy(d.y(), sy, w, h), u: copy(d.u(), su, w / 2, h / 2), v: copy(d.v(), sv, w / 2, h / 2) }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testsrc::moving_frame;

    fn psnr(a: &Yuv420, b: &Yuv420) -> f64 {
        let mse: f64 = a.y.iter().zip(&b.y).map(|(&x, &y)| (x as f64 - y as f64).powi(2)).sum::<f64>() / a.y.len() as f64;
        10.0 * (255.0f64 * 255.0 / mse.max(1e-9)).log10()
    }

    #[test]
    fn encode_decode_720p() {
        let mut e = H264Encoder::new(2_000_000, 30.0, 0).unwrap();
        let mut d = H264Decoder::new().unwrap();
        let (mut decoded, mut key_bytes, mut p_bytes, mut p_n, mut worst) = (0, 0, 0, 0, f64::MAX);
        for n in 0..30 {
            let src = moving_frame(1280, 720, n);
            let Some(f) = e.encode(&src, false).unwrap() else { continue };
            assert_eq!(f.keyframe, n == 0, "only the first frame should be a key frame (n={n})");
            if f.keyframe { key_bytes = f.data.len() } else { p_bytes += f.data.len(); p_n += 1 }
            if let Some(out) = d.decode(&f.data).unwrap() {
                assert_eq!((out.width, out.height), (1280, 720));
                worst = worst.min(psnr(&src, &out));
                decoded += 1;
            }
        }
        assert!(decoded >= 28, "decoded {decoded}");
        assert!(worst > 30.0, "worst PSNR {worst:.1} dB");
        println!("h264 720p: key {key_bytes} B, avg P {} B, worst PSNR {worst:.1} dB", p_bytes / p_n.max(1));
    }

    #[test]
    fn keyframe_request_and_live_bitrate_change() {
        let mut e = H264Encoder::new(4_000_000, 30.0, 0).unwrap();
        let size = |e: &mut H264Encoder, range: std::ops::Range<usize>| -> usize {
            range.map(|n| e.encode(&moving_frame(640, 360, n), false).unwrap().map_or(0, |f| f.data.len())).sum()
        };
        let before = size(&mut e, 0..30);
        e.set_bitrate(300_000).unwrap();
        let _warm = size(&mut e, 30..60);
        let after = size(&mut e, 60..90);
        assert!(after < before, "bitrate drop had no effect: {before} -> {after}");
        let f = e.encode(&moving_frame(640, 360, 91), true).unwrap().unwrap();
        assert!(f.keyframe, "force_key must produce an IDR");
    }

    #[test]
    fn decoder_resyncs_on_keyframe_after_loss() {
        let mut e = H264Encoder::new(2_000_000, 30.0, 0).unwrap();
        let mut d = H264Decoder::new().unwrap();
        let mut frames = vec![];
        for n in 0..20 {
            frames.push(e.encode(&moving_frame(640, 360, n), n == 10).unwrap().unwrap());
        }
        for f in &frames[..5] {
            d.decode(&f.data).unwrap();
        }
        // frames 5..10 lost; decoding resumes at the forced IDR (frame 10)
        let out = d.decode(&frames[10].data).unwrap();
        assert!(frames[10].keyframe);
        assert!(out.is_some(), "decoder should output a picture from an IDR");
    }
}
