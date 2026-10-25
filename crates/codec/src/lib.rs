//! Audio/video codecs behind small traits.
//!
//! * Audio: Opus, 48 kHz full-band, 20 ms frames, stereo 192 kbps by default ("ultra-HD voice").
//! * Video: `VideoEncoder`/`VideoDecoder` traits. `H264Encoder` is the portable software fallback
//!   (OpenH264). Hardware backends (VideoToolbox, MediaCodec, NVENC, VAAPI/QSV, Media Foundation)
//!   implement the same trait in the platform layer; the network stack does not care which is used.
pub mod audio;
pub mod testsrc;
pub mod video;

#[derive(Debug, thiserror::Error)]
#[error("codec error: {0}")]
pub struct CodecError(pub String);
pub type Result<T> = std::result::Result<T, CodecError>;

pub(crate) fn err<E: std::fmt::Display>(e: E) -> CodecError {
    CodecError(e.to_string())
}
