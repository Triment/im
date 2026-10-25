//! XMP wire formats.
//!
//! * `control`: binary frames carried on the QUIC control stream.
//! * `media`: the 16-byte header of the custom UDP media protocol.
pub mod codec;
pub mod control;
pub mod media;

pub use codec::{DecodeError, Reader, Writer};
