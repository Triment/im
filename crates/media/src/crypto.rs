//! AEAD packet protection and Noise-based key agreement.
//!
//! The 16-byte header is authenticated (AAD) but left in clear so relays and
//! SFUs can route on it. The nonce is derived from header fields that are unique
//! per packet, so a single session key can be shared by all streams as long as
//! `stream_id`s are unique per sender (`pid << 8 | index`).

use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Nonce,
};
use xmp_proto::media::{Header, HEADER_LEN};

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("authentication failed")]
    Auth,
    #[error("malformed packet")]
    Malformed,
    #[error("noise: {0}")]
    Noise(String),
}

pub struct MediaCipher {
    c: ChaCha20Poly1305,
}

fn nonce(h: &Header) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[0..4].copy_from_slice(&h.stream_id.to_be_bytes());
    n[4..8].copy_from_slice(&h.seq.to_be_bytes());
    n[8] = h.fec;
    n[9] = h.layer;
    n[10] = h.flags;
    n[11] = h.ptype as u8;
    n
}

impl MediaCipher {
    pub fn new(key: &[u8; 32]) -> Self {
        Self { c: ChaCha20Poly1305::new(key.into()) }
    }

    /// Returns the full wire packet: `header | ciphertext | tag`.
    pub fn seal(&self, h: &Header, plaintext: &[u8]) -> Vec<u8> {
        let hb = h.to_bytes();
        let ct = self
            .c
            .encrypt(Nonce::from_slice(&nonce(h)), Payload { msg: plaintext, aad: &hb })
            .expect("chacha20poly1305 encrypt cannot fail");
        let mut out = Vec::with_capacity(HEADER_LEN + ct.len());
        out.extend_from_slice(&hb);
        out.extend_from_slice(&ct);
        out
    }

    pub fn open(&self, packet: &[u8]) -> Result<(Header, Vec<u8>), CryptoError> {
        let (h, body) = Header::parse(packet).map_err(|_| CryptoError::Malformed)?;
        let pt = self
            .c
            .decrypt(Nonce::from_slice(&nonce(&h)), Payload { msg: body, aad: &packet[..HEADER_LEN] })
            .map_err(|_| CryptoError::Auth)?;
        Ok((h, pt))
    }
}

/// 1024-packet sliding anti-replay window.
pub struct ReplayWindow {
    highest: Option<u32>,
    bits: [u64; 16],
}

impl Default for ReplayWindow {
    fn default() -> Self {
        Self { highest: None, bits: [0; 16] }
    }
}

impl ReplayWindow {
    const SIZE: u32 = 1024;

    /// Returns `true` if the sequence number is fresh (and records it).
    pub fn check_and_set(&mut self, seq: u32) -> bool {
        let Some(hi) = self.highest else {
            self.highest = Some(seq);
            self.set(seq);
            return true;
        };
        if seq > hi {
            let shift = seq - hi;
            if shift >= Self::SIZE {
                self.bits = [0; 16];
            } else {
                for s in (hi + 1)..=seq {
                    self.clear(s);
                }
            }
            self.highest = Some(seq);
            self.set(seq);
            true
        } else if hi - seq >= Self::SIZE {
            false
        } else if self.get(seq) {
            false
        } else {
            self.set(seq);
            true
        }
    }
    fn idx(seq: u32) -> (usize, u64) {
        let i = (seq % Self::SIZE) as usize;
        (i / 64, 1u64 << (i % 64))
    }
    fn set(&mut self, seq: u32) {
        let (w, m) = Self::idx(seq);
        self.bits[w] |= m;
    }
    fn clear(&mut self, seq: u32) {
        let (w, m) = Self::idx(seq);
        self.bits[w] &= !m;
    }
    fn get(&self, seq: u32) -> bool {
        let (w, m) = Self::idx(seq);
        self.bits[w] & m != 0
    }
}

const NOISE_PATTERN: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2s";

/// Generate a long-term X25519 device key (private half).
pub fn generate_static_key() -> Vec<u8> {
    let b = snow::Builder::new(NOISE_PATTERN.parse().unwrap());
    b.generate_keypair().expect("keypair").private
}

pub struct Established {
    pub media_key: [u8; 32],
    pub remote_static: Vec<u8>,
    /// 6-digit short authentication string; both users can compare it out-of-band.
    pub sas: String,
}

/// Noise XX handshake carried over the (server-relayed) signalling channel.
/// The server only ever sees opaque handshake bytes.
pub struct Handshake {
    hs: snow::HandshakeState,
}

impl Handshake {
    pub fn new(initiator: bool, static_private: &[u8]) -> Result<Self, CryptoError> {
        let b = snow::Builder::new(NOISE_PATTERN.parse().unwrap()).local_private_key(static_private);
        let hs = if initiator { b.build_initiator() } else { b.build_responder() }
            .map_err(|e| CryptoError::Noise(e.to_string()))?;
        Ok(Self { hs })
    }

    pub fn write(&mut self) -> Result<Vec<u8>, CryptoError> {
        let mut buf = vec![0u8; 512];
        let n = self.hs.write_message(&[], &mut buf).map_err(|e| CryptoError::Noise(e.to_string()))?;
        buf.truncate(n);
        Ok(buf)
    }

    pub fn read(&mut self, msg: &[u8]) -> Result<(), CryptoError> {
        let mut buf = vec![0u8; 512];
        self.hs.read_message(msg, &mut buf).map_err(|e| CryptoError::Noise(e.to_string()))?;
        Ok(())
    }

    pub fn is_finished(&self) -> bool {
        self.hs.is_handshake_finished()
    }

    pub fn finish(self) -> Result<Established, CryptoError> {
        if !self.hs.is_handshake_finished() {
            return Err(CryptoError::Noise("handshake not finished".into()));
        }
        let hash = self.hs.get_handshake_hash().to_vec();
        let remote_static = self.hs.get_remote_static().map(|s| s.to_vec()).unwrap_or_default();
        let media_key = blake3::derive_key("xmp media key v1", &hash);
        let sas_num = u32::from_be_bytes(hash[..4].try_into().unwrap()) % 1_000_000;
        Ok(Established { media_key, remote_static, sas: format!("{sas_num:06}") })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xmp_proto::media::PacketType;

    #[test]
    fn seal_open_and_tamper() {
        let c = MediaCipher::new(&[7; 32]);
        let h = Header::new(PacketType::Audio, 256, 5, 100);
        let mut pkt = c.seal(&h, b"hello opus");
        let (h2, pt) = c.open(&pkt).unwrap();
        assert_eq!(h, h2);
        assert_eq!(pt, b"hello opus");
        // tamper with header (authenticated) and payload
        let mut p2 = pkt.clone();
        p2[9] ^= 1;
        assert!(c.open(&p2).is_err());
        let n = pkt.len();
        pkt[n - 1] ^= 1;
        assert!(c.open(&pkt).is_err());
        // wrong key
        assert!(MediaCipher::new(&[8; 32]).open(&c.seal(&h, b"x")).is_err());
    }

    #[test]
    fn replay_window() {
        let mut w = ReplayWindow::default();
        assert!(w.check_and_set(10));
        assert!(!w.check_and_set(10));
        assert!(w.check_and_set(8));
        assert!(!w.check_and_set(8));
        assert!(w.check_and_set(2000));
        assert!(!w.check_and_set(10)); // too old
        assert!(w.check_and_set(1999));
    }

    #[test]
    fn noise_xx_agrees() {
        let (ka, kb) = (generate_static_key(), generate_static_key());
        let mut a = Handshake::new(true, &ka).unwrap();
        let mut b = Handshake::new(false, &kb).unwrap();
        let m1 = a.write().unwrap();
        b.read(&m1).unwrap();
        let m2 = b.write().unwrap();
        a.read(&m2).unwrap();
        let m3 = a.write().unwrap();
        b.read(&m3).unwrap();
        let (ea, eb) = (a.finish().unwrap(), b.finish().unwrap());
        assert_eq!(ea.media_key, eb.media_key);
        assert_eq!(ea.sas, eb.sas);
        assert_eq!(ea.sas.len(), 6);
        assert_eq!(ea.remote_static.len(), 32);
    }
}
