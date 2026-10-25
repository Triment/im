//! XMP media packet header (16 bytes, big-endian).
//!
//! ```text
//!  0       1       2       3
//! +-------+-------+-------+-------+
//! |ver|typ| flags | layer |  fec  |
//! +-------+-------+-------+-------+
//! |           stream_id           |
//! +-------------------------------+
//! |              seq              |   per (stream_id, layer)
//! +-------------------------------+
//! |          ts (ms, u32)         |
//! +-------------------------------+
//! ```
//! * `layer`: spatial layer in the high nibble, temporal layer in the low nibble.
//! * `fec`: high nibble = group size `k`, low nibble = index inside the group
//!   (data packet) or parity index (parity packet, flag `PARITY`).
//!
//! The header is authenticated (AEAD associated data) but not encrypted, so an
//! SFU can route and select layers without seeing media.

use crate::codec::DecodeError;

pub const VERSION: u8 = 1;
pub const HEADER_LEN: usize = 16;
pub const TAG_LEN: usize = 16;
pub const MAX_PACKET: usize = 1200;

pub mod flags {
    /// Last packet of a video frame / audio talk-spurt boundary.
    pub const MARKER: u8 = 1;
    pub const KEYFRAME: u8 = 2;
    pub const PARITY: u8 = 4;
    pub const RETRANSMIT: u8 = 8;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PacketType {
    /// client -> server, plaintext: `token[16]`
    Hello = 1,
    /// server -> client, plaintext: observed `ip:port` (STUN-like reflexive address)
    HelloAck = 2,
    /// peer <-> peer hole-punch probe (encrypted)
    Probe = 3,
    ProbeAck = 4,
    Audio = 0x10,
    Video = 0x11,
    /// encrypted: list of missing seqs
    Nack = 0x20,
    /// encrypted: receiver report (REMB-style)
    Feedback = 0x21,
    /// client -> server only, plaintext: SFU subscription / bandwidth report
    Report = 0x30,
    Keepalive = 0x40,
}

impl PacketType {
    pub fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            1 => Self::Hello,
            2 => Self::HelloAck,
            3 => Self::Probe,
            4 => Self::ProbeAck,
            0x10 => Self::Audio,
            0x11 => Self::Video,
            0x20 => Self::Nack,
            0x21 => Self::Feedback,
            0x30 => Self::Report,
            0x40 => Self::Keepalive,
            _ => return None,
        })
    }
    /// Plaintext control packets handled by the server itself.
    pub fn is_server_plain(self) -> bool {
        matches!(self, Self::Hello | Self::HelloAck | Self::Report)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub ptype: PacketType,
    pub flags: u8,
    pub layer: u8,
    pub fec: u8,
    pub stream_id: u32,
    pub seq: u32,
    pub ts: u32,
}

impl Header {
    pub fn new(ptype: PacketType, stream_id: u32, seq: u32, ts: u32) -> Self {
        Self { ptype, flags: 0, layer: 0, fec: 0, stream_id, seq, ts }
    }

    pub fn to_bytes(&self) -> [u8; HEADER_LEN] {
        let mut b = [0u8; HEADER_LEN];
        b[0] = (VERSION << 6) | (self.ptype as u8 & 0x3f);
        b[1] = self.flags;
        b[2] = self.layer;
        b[3] = self.fec;
        b[4..8].copy_from_slice(&self.stream_id.to_be_bytes());
        b[8..12].copy_from_slice(&self.seq.to_be_bytes());
        b[12..16].copy_from_slice(&self.ts.to_be_bytes());
        b
    }

    /// Parse the header and return the remaining payload.
    pub fn parse(buf: &[u8]) -> Result<(Header, &[u8]), DecodeError> {
        if buf.len() < HEADER_LEN {
            return Err(DecodeError::Eof);
        }
        if buf[0] >> 6 != VERSION {
            return Err(DecodeError::Invalid("version"));
        }
        let ptype = PacketType::from_u8(buf[0] & 0x3f).ok_or(DecodeError::Invalid("packet type"))?;
        let h = Header {
            ptype,
            flags: buf[1],
            layer: buf[2],
            fec: buf[3],
            stream_id: u32::from_be_bytes(buf[4..8].try_into().unwrap()),
            seq: u32::from_be_bytes(buf[8..12].try_into().unwrap()),
            ts: u32::from_be_bytes(buf[12..16].try_into().unwrap()),
        };
        Ok((h, &buf[HEADER_LEN..]))
    }

    pub fn spatial(&self) -> u8 {
        self.layer >> 4
    }
    pub fn temporal(&self) -> u8 {
        self.layer & 0x0f
    }
    pub fn is_parity(&self) -> bool {
        self.flags & flags::PARITY != 0
    }
    /// Participant id encoded in the stream id (`pid << 8 | stream_index`).
    pub fn participant(&self) -> u32 {
        self.stream_id >> 8
    }
}

pub fn stream_id(participant: u32, index: u8) -> u32 {
    (participant << 8) | index as u32
}

/// Well-known stream indices.
pub mod stream {
    pub const AUDIO: u8 = 0;
    pub const VIDEO: u8 = 1;
    pub const SCREEN: u8 = 2;
    pub const CONTROL: u8 = 0xff;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_roundtrip() {
        let mut h = Header::new(PacketType::Video, stream_id(3, stream::VIDEO), 123456, 987654);
        h.flags = flags::KEYFRAME | flags::MARKER;
        h.layer = 0x21;
        h.fec = 0x42;
        let mut pkt = h.to_bytes().to_vec();
        pkt.extend_from_slice(b"payload");
        let (h2, p) = Header::parse(&pkt).unwrap();
        assert_eq!(h, h2);
        assert_eq!(p, b"payload");
        assert_eq!(h2.spatial(), 2);
        assert_eq!(h2.temporal(), 1);
        assert_eq!(h2.participant(), 3);
    }

    #[test]
    fn bad_headers() {
        assert!(Header::parse(&[0; 5]).is_err());
        let mut b = Header::new(PacketType::Audio, 1, 1, 1).to_bytes();
        b[0] = 0x80 | 0x10;
        assert!(Header::parse(&b).is_err());
    }
}

/// Receiver report (encrypted, `PacketType::Feedback`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Feedback {
    /// Receiver-side bandwidth estimate (REMB style).
    pub remb_bps: u32,
    /// Loss fraction since last report, 0..=255.
    pub loss: u8,
    pub jitter_ms: u16,
    /// Ask the sender for a key frame (PLI).
    pub want_keyframe: bool,
}

impl Feedback {
    pub fn encode(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(8);
        v.extend_from_slice(&self.remb_bps.to_be_bytes());
        v.push(self.loss);
        v.extend_from_slice(&self.jitter_ms.to_be_bytes());
        v.push(self.want_keyframe as u8);
        v
    }
    pub fn decode(b: &[u8]) -> Result<Self, DecodeError> {
        if b.len() < 8 {
            return Err(DecodeError::Eof);
        }
        Ok(Self {
            remb_bps: u32::from_be_bytes(b[0..4].try_into().unwrap()),
            loss: b[4],
            jitter_ms: u16::from_be_bytes(b[5..7].try_into().unwrap()),
            want_keyframe: b[7] != 0,
        })
    }
}

/// Retransmission request (encrypted, `PacketType::Nack`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nack {
    pub stream_id: u32,
    pub layer: u8,
    pub seqs: Vec<u32>,
}

impl Nack {
    pub fn encode(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(7 + self.seqs.len() * 4);
        v.extend_from_slice(&self.stream_id.to_be_bytes());
        v.push(self.layer);
        v.extend_from_slice(&(self.seqs.len() as u16).to_be_bytes());
        for s in &self.seqs {
            v.extend_from_slice(&s.to_be_bytes());
        }
        v
    }
    pub fn decode(b: &[u8]) -> Result<Self, DecodeError> {
        if b.len() < 7 {
            return Err(DecodeError::Eof);
        }
        let stream_id = u32::from_be_bytes(b[0..4].try_into().unwrap());
        let layer = b[4];
        let n = u16::from_be_bytes(b[5..7].try_into().unwrap()) as usize;
        if b.len() < 7 + n * 4 {
            return Err(DecodeError::Eof);
        }
        let seqs = (0..n).map(|i| u32::from_be_bytes(b[7 + i * 4..11 + i * 4].try_into().unwrap())).collect();
        Ok(Self { stream_id, layer, seqs })
    }
}

/// Subscriber -> SFU report (plaintext, `PacketType::Report`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Report {
    /// Estimated downlink bandwidth in bps; the SFU picks the highest layer that fits.
    pub est_bps: u32,
    /// Optional cap on the highest spatial layer (0xff = no cap).
    pub max_spatial: u8,
}

impl Report {
    pub fn encode(&self) -> Vec<u8> {
        let mut v = self.est_bps.to_be_bytes().to_vec();
        v.push(self.max_spatial);
        v
    }
    pub fn decode(b: &[u8]) -> Result<Self, DecodeError> {
        if b.len() < 5 {
            return Err(DecodeError::Eof);
        }
        Ok(Self { est_bps: u32::from_be_bytes(b[0..4].try_into().unwrap()), max_spatial: b[4] })
    }
}

#[cfg(test)]
mod payload_tests {
    use super::*;
    #[test]
    fn payloads() {
        let f = Feedback { remb_bps: 5_000_000, loss: 12, jitter_ms: 7, want_keyframe: true };
        assert_eq!(Feedback::decode(&f.encode()).unwrap(), f);
        let n = Nack { stream_id: 258, layer: 1, seqs: vec![1, 2, 99] };
        assert_eq!(Nack::decode(&n.encode()).unwrap(), n);
        let r = Report { est_bps: 1234, max_spatial: 2 };
        assert_eq!(Report::decode(&r.encode()).unwrap(), r);
    }
}
