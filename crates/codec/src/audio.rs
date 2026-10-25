use crate::{err, Result};
use opus::{Application, Bitrate, Channels};

pub const SAMPLE_RATE: u32 = 48_000;

#[derive(Clone, Debug)]
pub struct AudioConfig {
    pub channels: u8,
    pub bitrate_bps: i32,
    pub frame_ms: u32,
    /// Opus in-band FEC (LBRR). Off by default: the transport already has adaptive Reed-Solomon FEC.
    pub inband_fec: bool,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self { channels: 2, bitrate_bps: 192_000, frame_ms: 20, inband_fec: false }
    }
}

impl AudioConfig {
    /// Interleaved samples in one frame (all channels).
    pub fn frame_len(&self) -> usize {
        (SAMPLE_RATE / 1000 * self.frame_ms) as usize * self.channels as usize
    }
}

fn ch(n: u8) -> Channels {
    if n == 1 { Channels::Mono } else { Channels::Stereo }
}

pub struct AudioEncoder {
    enc: opus::Encoder,
    cfg: AudioConfig,
}

impl AudioEncoder {
    pub fn new(cfg: AudioConfig) -> Result<Self> {
        let mut enc = opus::Encoder::new(SAMPLE_RATE, ch(cfg.channels), Application::Audio).map_err(err)?;
        enc.set_bitrate(Bitrate::Bits(cfg.bitrate_bps)).map_err(err)?;
        enc.set_inband_fec(cfg.inband_fec).map_err(err)?;
        Ok(Self { enc, cfg })
    }

    pub fn config(&self) -> &AudioConfig {
        &self.cfg
    }

    /// Encode exactly one frame of interleaved i16 PCM.
    pub fn encode(&mut self, pcm: &[i16]) -> Result<Vec<u8>> {
        if pcm.len() != self.cfg.frame_len() {
            return Err(crate::CodecError(format!("expected {} samples, got {}", self.cfg.frame_len(), pcm.len())));
        }
        let mut out = vec![0u8; 4000];
        let n = self.enc.encode(pcm, &mut out).map_err(err)?;
        out.truncate(n);
        Ok(out)
    }

    pub fn set_bitrate(&mut self, bps: i32) -> Result<()> {
        self.cfg.bitrate_bps = bps;
        self.enc.set_bitrate(Bitrate::Bits(bps)).map_err(err)
    }

    /// Tell Opus the expected loss so it can tune in-band redundancy.
    pub fn set_expected_loss(&mut self, pct: u8) -> Result<()> {
        self.enc.set_packet_loss_perc(pct.min(100) as i32).map_err(err)
    }
}

pub struct AudioDecoder {
    dec: opus::Decoder,
    cfg: AudioConfig,
}

impl AudioDecoder {
    pub fn new(cfg: AudioConfig) -> Result<Self> {
        Ok(Self { dec: opus::Decoder::new(SAMPLE_RATE, ch(cfg.channels)).map_err(err)?, cfg })
    }

    /// `Some(packet)` decodes; `None` runs packet-loss concealment for one frame.
    pub fn decode(&mut self, packet: Option<&[u8]>) -> Result<Vec<i16>> {
        let mut out = vec![0i16; self.cfg.frame_len()];
        let n = self.dec.decode(packet.unwrap_or(&[]), &mut out, false).map_err(err)?;
        out.truncate(n * self.cfg.channels as usize);
        Ok(out)
    }

    /// Recover the *previous* (lost) frame from the in-band FEC data of `next_packet`.
    pub fn decode_lost_from_next(&mut self, next_packet: &[u8]) -> Result<Vec<i16>> {
        let mut out = vec![0i16; self.cfg.frame_len()];
        let n = self.dec.decode(next_packet, &mut out, true).map_err(err)?;
        out.truncate(n * self.cfg.channels as usize);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testsrc::sine_frame;

    fn snr_db(a: &[i16], b: &[i16]) -> f64 {
        let sig: f64 = a.iter().map(|&x| (x as f64).powi(2)).sum();
        let noise: f64 = a.iter().zip(b).map(|(&x, &y)| ((x - y) as f64).powi(2)).sum();
        10.0 * (sig / noise.max(1.0)).log10()
    }

    #[test]
    fn roundtrip_quality_and_size() {
        let cfg = AudioConfig::default();
        let (mut e, mut d) = (AudioEncoder::new(cfg.clone()).unwrap(), AudioDecoder::new(cfg.clone()).unwrap());
        let (mut orig, mut dec, mut bytes) = (vec![], vec![], 0usize);
        for i in 0..100 {
            let pcm = sine_frame(i, &cfg);
            let pkt = e.encode(&pcm).unwrap();
            bytes += pkt.len();
            let out = d.decode(Some(&pkt)).unwrap();
            assert_eq!(out.len(), pcm.len());
            if i >= 10 {
                // skip codec warm-up; opus has ~6.5 ms lookahead so compare with delay compensation below
                orig.extend(pcm);
                dec.extend(out);
            }
        }
        // find the best alignment (lookahead delay) and require good SNR
        let best = (0..400).map(|lag| snr_db(&orig[..orig.len() - 800], &dec[lag..lag + orig.len() - 800])).fold(f64::MIN, f64::max);
        assert!(best > 20.0, "SNR {best:.1} dB too low");
        let kbps = bytes * 8 / 2 / 1000; // 100 frames = 2 s
        assert!(kbps <= 200, "{kbps} kbps");
        println!("opus: SNR {best:.1} dB at {kbps} kbps");
    }

    #[test]
    fn plc_and_bitrate_change() {
        let cfg = AudioConfig::default();
        let (mut e, mut d) = (AudioEncoder::new(cfg.clone()).unwrap(), AudioDecoder::new(cfg.clone()).unwrap());
        for i in 0..20 {
            let p = e.encode(&sine_frame(i, &cfg)).unwrap();
            d.decode(Some(&p)).unwrap();
        }
        let concealed = d.decode(None).unwrap();
        assert_eq!(concealed.len(), cfg.frame_len());
        assert!(concealed.iter().any(|&s| s != 0), "PLC should extrapolate the tone, not output silence");
        e.set_bitrate(32_000).unwrap();
        e.set_expected_loss(10).unwrap();
        let small = e.encode(&sine_frame(21, &cfg)).unwrap();
        assert!(small.len() < 200);
        assert!(e.encode(&[0; 10]).is_err());
    }

    #[test]
    fn inband_fec_recovers_previous_frame() {
        let cfg = AudioConfig { inband_fec: true, ..Default::default() };
        let (mut e, mut d) = (AudioEncoder::new(cfg.clone()).unwrap(), AudioDecoder::new(cfg.clone()).unwrap());
        e.set_expected_loss(20).unwrap();
        let mut pkts = vec![];
        for i in 0..30 {
            pkts.push(e.encode(&sine_frame(i, &cfg)).unwrap());
        }
        for p in &pkts[..20] {
            d.decode(Some(p)).unwrap();
        }
        // packet 20 lost; frame 21 carries redundancy for it
        let rec = d.decode_lost_from_next(&pkts[21]).unwrap();
        assert_eq!(rec.len(), cfg.frame_len());
    }
}
