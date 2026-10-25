//! Media session engine over the custom XMP UDP protocol.
//!
//! * Path management: starts on the server relay immediately (fast start), hole-punches P2P in
//!   the background and upgrades as soon as an authenticated `ProbeAck` arrives; falls back to the
//!   relay if the P2P path goes silent.
//! * Audio: FEC (small groups), adaptive jitter buffer, loss reporting for PLC.
//! * Video: fragmentation, paced sending, FEC + NACK retransmission, key-frame requests.
//! * Congestion control: receiver-side delay-gradient estimate (REMB) + sender loss controller.
//! * SFU mode: same engine, but all traffic goes to the server and the downlink estimate is
//!   reported so the SFU can pick spatial layers.

use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{net::UdpSocket, sync::mpsc};
use xmp_media::{
    cc::{video_target_bps, DelayBwe, LossController},
    crypto::{MediaCipher, ReplayWindow},
    fec::{adaptive_params, FecDecoder, FecEncoder},
    jitter::{JitterBuffer, Playout},
    pacer::Pacer,
    packetizer::{fragment, FrameAssembler, DEFAULT_MTU_PAYLOAD},
};
use xmp_proto::media::{flags, stream, stream_id, Feedback, Header, Nack, PacketType, Report, HEADER_LEN, TAG_LEN};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Path {
    None,
    Relay(SocketAddr),
    P2p(SocketAddr),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// 1:1 call: relay + optional P2P upgrade.
    Call,
    /// Conference through the SFU; never P2P.
    Sfu,
}

#[derive(Debug)]
pub enum MediaEvent {
    /// `data == None` means the packet was lost and the decoder should conceal (PLC).
    Audio { from: u32, ts: u32, data: Option<Vec<u8>> },
    Video { from: u32, layer: u8, ts: u32, keyframe: bool, data: Vec<u8> },
    /// Remote asked for a key frame.
    KeyframeRequest,
    PathChanged(Path),
    /// New send-side targets decided by congestion control.
    Bitrate { video_bps: u32 },
    /// Server-reflexive address learned from the server (use as a P2P candidate).
    Reflexive(SocketAddr),
}

#[derive(Clone)]
pub struct MediaConfig {
    pub key: [u8; 32],
    /// Participant id: encoded in stream ids so the shared key never reuses a nonce.
    pub pid: u32,
    pub server: Option<SocketAddr>,
    pub token: Option<[u8; 16]>,
    pub mode: Mode,
    pub audio_frame_ms: u32,
    pub audio_bps: u32,
    pub video_start_bps: u32,
    pub video_min_bps: u32,
    pub video_max_bps: u32,
    pub audio_min_delay: Duration,
    pub audio_max_delay: Duration,
    pub video_min_delay: Duration,
}

impl MediaConfig {
    pub fn new(key: [u8; 32], pid: u32) -> Self {
        Self {
            key,
            pid,
            server: None,
            token: None,
            mode: Mode::Call,
            audio_frame_ms: 20,
            audio_bps: 128_000,
            video_start_bps: 2_500_000,
            video_min_bps: 150_000,
            video_max_bps: 20_000_000,
            audio_min_delay: Duration::from_millis(20),
            audio_max_delay: Duration::from_millis(200),
            video_min_delay: Duration::from_millis(30),
        }
    }
}

#[derive(Default, Clone, Debug)]
pub struct SessionStats {
    pub tx_packets: u64,
    pub rx_packets: u64,
    pub retransmits: u64,
    pub fec_recovered: u64,
    pub dropped_frames: u64,
    pub video_target_bps: u32,
}

struct AudioRx {
    jb: JitterBuffer<Vec<u8>>,
    fec: FecDecoder,
    last: Option<(u32, u32)>, // (seq, ts)
}

struct VideoRx {
    jb: JitterBuffer<(u8, Vec<u8>)>,
    fec: FecDecoder,
    asm: FrameAssembler,
    nacked: HashMap<u32, (Instant, u8)>,
}

struct State {
    path: Path,
    candidates: Vec<SocketAddr>,
    punch_until: Option<Instant>,
    last_p2p_rx: Option<Instant>,
    hello_acked: bool,
    tx_seq: HashMap<(u32, u8), u32>,
    fec_tx: HashMap<(u32, u8), FecEncoder>,
    history: HashMap<(u32, u8), VecDeque<(Header, Vec<u8>)>>,
    pacer: Pacer,
    replay: HashMap<(u32, u8, u8, bool), ReplayWindow>,
    audio_rx: HashMap<u32, AudioRx>,
    video_rx: HashMap<(u32, u8), VideoRx>,
    bwe: HashMap<u32, DelayBwe>,
    loss_ctl: LossController,
    remb: u32,
    video_target: u32,
    control_seq: u32,
    last_counts: (u64, u64),
    last_hello: Option<Instant>,
    last_probe: Option<Instant>,
    last_nack: Instant,
    last_feedback: Instant,
    last_report: Instant,
    last_keepalive: Instant,
    last_pli: Option<Instant>,
    stats: SessionStats,
}

struct Inner {
    cfg: MediaConfig,
    sock: Arc<UdpSocket>,
    cipher: MediaCipher,
    events: mpsc::UnboundedSender<MediaEvent>,
    epoch: Instant,
    st: Mutex<State>,
}

#[derive(Clone)]
pub struct MediaSession {
    i: Arc<Inner>,
}

type Out = Vec<(Vec<u8>, SocketAddr)>;

fn audio_fec_params(loss: f32) -> (usize, usize) {
    if loss < 0.005 {
        (0, 0)
    } else if loss < 0.03 {
        (4, 1)
    } else if loss < 0.10 {
        (4, 2)
    } else {
        (3, 3)
    }
}

impl State {
    fn dest(&self) -> Option<SocketAddr> {
        match self.path {
            Path::None => None,
            Path::Relay(a) | Path::P2p(a) => Some(a),
        }
    }
    fn next_seq(&mut self, sid: u32, layer: u8) -> u32 {
        let s = self.tx_seq.entry((sid, layer)).or_insert(0);
        let v = *s;
        *s += 1;
        v
    }
}

impl Inner {
    fn now_ms(&self, now: Instant) -> u32 {
        now.duration_since(self.epoch).as_millis() as u32
    }

    fn control_packet(&self, st: &mut State, ptype: PacketType, payload: &[u8], now: Instant) -> Vec<u8> {
        let sid = stream_id(self.cfg.pid, stream::CONTROL);
        let seq = st.control_seq;
        st.control_seq += 1;
        self.cipher.seal(&Header::new(ptype, sid, seq, self.now_ms(now)), payload)
    }

    fn set_path(&self, st: &mut State, p: Path) {
        if st.path != p {
            st.path = p;
            let _ = self.events.send(MediaEvent::PathChanged(p));
        }
    }

    fn hello_packet(&self) -> Option<Vec<u8>> {
        let t = self.cfg.token?;
        let mut p = Header::new(PacketType::Hello, 0, 0, 0).to_bytes().to_vec();
        p.extend_from_slice(&t);
        Some(p)
    }

    fn on_packet(&self, pkt: &[u8], src: SocketAddr, now: Instant) -> Out {
        let mut out = Out::new();
        let Ok((h0, body)) = Header::parse(pkt) else { return out };
        if h0.ptype == PacketType::HelloAck {
            if Some(src) == self.cfg.server {
                if let Ok(a) = std::str::from_utf8(body).unwrap_or("").parse::<SocketAddr>() {
                    self.st.lock().unwrap().hello_acked = true;
                    let _ = self.events.send(MediaEvent::Reflexive(a));
                }
            }
            return out;
        }
        if h0.ptype.is_server_plain() {
            return out;
        }
        let Ok((h, pt)) = self.cipher.open(pkt) else { return out };
        let mut st = self.st.lock().unwrap();
        let key = (h.stream_id, h.layer, h.ptype as u8, h.is_parity());
        if !st.replay.entry(key).or_default().check_and_set(h.seq) {
            return out;
        }
        st.stats.rx_packets += 1;
        let via_server = Some(src) == self.cfg.server;
        if !via_server {
            st.last_p2p_rx = Some(now);
        }
        match h.ptype {
            PacketType::Probe => {
                if !via_server && self.cfg.mode == Mode::Call {
                    if !st.candidates.contains(&src) {
                        st.candidates.push(src); // peer-reflexive candidate
                    }
                    out.push((self.control_packet(&mut st, PacketType::ProbeAck, &pt, now), src));
                }
            }
            PacketType::ProbeAck => {
                if !via_server && self.cfg.mode == Mode::Call {
                    self.set_path(&mut st, Path::P2p(src));
                }
            }
            PacketType::Audio => self.on_audio(&mut st, &h, pt, now),
            PacketType::Video => self.on_video(&mut st, &h, pt, now),
            PacketType::Nack => {
                if let (Ok(n), Some(d)) = (Nack::decode(&pt), st.dest()) {
                    for seq in n.seqs.iter().take(64) {
                        let hit = st.history.get(&(n.stream_id, n.layer)).and_then(|q| q.iter().find(|(h, _)| h.seq == *seq).cloned());
                        if let Some((mut h, plain)) = hit {
                            h.flags |= flags::RETRANSMIT;
                            out.push((self.cipher.seal(&h, &plain), d));
                            st.stats.retransmits += 1;
                        }
                    }
                }
            }
            PacketType::Feedback => {
                if let Ok(fb) = Feedback::decode(&pt) {
                    self.on_feedback(&mut st, fb);
                }
            }
            _ => {}
        }
        out
    }

    fn on_audio(&self, st: &mut State, h: &Header, pt: Vec<u8>, now: Instant) {
        let pid = h.participant();
        let cfg = &self.cfg;
        let rx = st.audio_rx.entry(pid).or_insert_with(|| AudioRx {
            jb: JitterBuffer::new(cfg.audio_min_delay, cfg.audio_max_delay, now),
            fec: FecDecoder::new(),
            last: None,
        });
        let recovered = if h.is_parity() {
            rx.fec.on_parity(h.seq, h.fec, &pt)
        } else {
            let r = rx.fec.on_data(h.seq, h.fec, &pt);
            rx.jb.push(h.seq, h.ts, pt, now);
            if rx.last.map_or(true, |(s, _)| h.seq > s) {
                rx.last = Some((h.seq, h.ts));
            }
            r
        };
        for (seq, payload) in recovered {
            let ts = rx.last.map_or(0, |(ls, lt)| (lt as i64 + (seq as i64 - ls as i64) * self.cfg.audio_frame_ms as i64) as u32);
            if rx.jb.push(seq, ts, payload, now) {
                st.stats.fec_recovered += 1;
            }
        }
    }

    fn on_video(&self, st: &mut State, h: &Header, pt: Vec<u8>, now: Instant) {
        let pid = h.participant();
        let cfg = &self.cfg;
        let rx = st.video_rx.entry((pid, h.layer)).or_insert_with(|| VideoRx {
            jb: JitterBuffer::new(cfg.video_min_delay, Duration::from_millis(250), now),
            fec: FecDecoder::new(),
            asm: FrameAssembler::new(),
            nacked: HashMap::new(),
        });
        // video plaintext = flags(1) | ts(4) | fragment
        fn push(rx: &mut VideoRx, seq: u32, p: &[u8], now: Instant) -> bool {
            if p.len() < 5 {
                return false;
            }
            let ts = u32::from_be_bytes(p[1..5].try_into().unwrap());
            rx.jb.push(seq, ts, (p[0], p[5..].to_vec()), now)
        }
        let recovered = if h.is_parity() {
            rx.fec.on_parity(h.seq, h.fec, &pt)
        } else {
            let r = rx.fec.on_data(h.seq, h.fec, &pt);
            push(rx, h.seq, &pt, now);
            r
        };
        let mut n_rec = 0;
        for (seq, p) in recovered {
            if push(rx, seq, &p, now) {
                n_rec += 1;
            }
        }
        st.stats.fec_recovered += n_rec;
        if !h.is_parity() && h.flags & flags::RETRANSMIT == 0 {
            let arrival = now.duration_since(self.epoch).as_secs_f64() * 1000.0;
            let (start, min, max) = (cfg.video_start_bps, cfg.video_min_bps, cfg.video_max_bps);
            st.bwe
                .entry(pid)
                .or_insert_with(|| DelayBwe::new(start, min, max))
                .on_packet(arrival, h.ts, pt.len() + HEADER_LEN + TAG_LEN);
        }
    }

    fn on_feedback(&self, st: &mut State, fb: Feedback) {
        let loss = fb.loss as f32 / 255.0;
        let loss_rate = st.loss_ctl.on_loss(loss);
        st.remb = fb.remb_bps;
        let target = video_target_bps(st.remb, loss_rate, self.cfg.audio_bps, self.cfg.video_min_bps, self.cfg.video_max_bps);
        st.video_target = target;
        st.stats.video_target_bps = target;
        st.pacer.set_rate(target + self.cfg.audio_bps);
        let (ak, am) = audio_fec_params(loss);
        let (vk, vm) = adaptive_params(loss);
        for ((sid, _), enc) in st.fec_tx.iter_mut() {
            if sid & 0xff == stream::AUDIO as u32 {
                enc.set_params(ak, am);
            } else {
                enc.set_params(vk, vm);
            }
        }
        let _ = self.events.send(MediaEvent::Bitrate { video_bps: target });
        if fb.want_keyframe {
            let _ = self.events.send(MediaEvent::KeyframeRequest);
        }
    }

    fn tick(&self, now: Instant) -> Out {
        let mut out = Out::new();
        let mut st = self.st.lock().unwrap();

        // 1. playout
        for (pid, rx) in st.audio_rx.iter_mut() {
            while let Some(p) = rx.jb.pop(now) {
                match p {
                    Playout::Packet { ts, data, .. } => {
                        let _ = self.events.send(MediaEvent::Audio { from: *pid, ts, data: Some(data) });
                    }
                    Playout::Lost(seq) => {
                        let ts = rx.last.map_or(0, |(ls, lt)| (lt as i64 + (seq as i64 - ls as i64) * self.cfg.audio_frame_ms as i64) as u32);
                        let _ = self.events.send(MediaEvent::Audio { from: *pid, ts, data: None });
                    }
                }
            }
        }
        let mut want_key = false;
        for ((pid, layer), rx) in st.video_rx.iter_mut() {
            while let Some(p) = rx.jb.pop(now) {
                match p {
                    Playout::Packet { ts, data: (fl, frag), .. } => {
                        if let Some(f) = rx.asm.push(ts, fl & flags::MARKER != 0, fl & flags::KEYFRAME != 0, &frag) {
                            let _ = self.events.send(MediaEvent::Video { from: *pid, layer: *layer, ts: f.ts, keyframe: f.keyframe, data: f.data });
                        }
                    }
                    Playout::Lost(_) => rx.asm.on_lost(),
                }
            }
            want_key |= rx.asm.need_keyframe;
        }
        let dropped: u64 = st.video_rx.values().map(|r| r.asm.dropped_frames).sum();
        st.stats.dropped_frames = dropped;

        // 2. handshake with the server (also keeps NAT bindings alive)
        if let (Some(server), Some(hello)) = (self.cfg.server, self.hello_packet()) {
            let every = if st.hello_acked { Duration::from_secs(5) } else { Duration::from_millis(200) };
            if st.last_hello.map_or(true, |t| now.duration_since(t) >= every) {
                st.last_hello = Some(now);
                out.push((hello, server));
            }
        }

        // 3. P2P hole punching (1:1 only)
        if self.cfg.mode == Mode::Call {
            if let Some(until) = st.punch_until {
                let relay_or_none = !matches!(st.path, Path::P2p(_));
                if now < until && relay_or_none {
                    let fast = until.saturating_duration_since(now) > Duration::from_secs(3);
                    let every = if fast { Duration::from_millis(50) } else { Duration::from_millis(250) };
                    if st.last_probe.map_or(true, |t| now.duration_since(t) >= every) {
                        st.last_probe = Some(now);
                        let cands = st.candidates.clone();
                        for c in cands {
                            let p = self.control_packet(&mut st, PacketType::Probe, &rand::random::<u64>().to_be_bytes(), now);
                            out.push((p, c));
                        }
                    }
                }
            }
            // P2P path went silent: fall back to the relay and try punching again
            if let Path::P2p(_) = st.path {
                if st.last_p2p_rx.map_or(true, |t| now.duration_since(t) > Duration::from_secs(3)) {
                    let p = self.cfg.server.map_or(Path::None, Path::Relay);
                    self.set_path(&mut st, p);
                    st.punch_until = Some(now + Duration::from_secs(5));
                }
            }
        }

        // 4. NACKs for video gaps (every 20 ms, max 3 tries per seq, 30 ms apart)
        if now.duration_since(st.last_nack) >= Duration::from_millis(20) {
            st.last_nack = now;
            let mut nacks = vec![];
            for ((pid, layer), rx) in st.video_rx.iter_mut() {
                let missing = rx.jb.missing();
                rx.nacked.retain(|s, _| missing.contains(s));
                let mut want = vec![];
                for s in missing {
                    let e = rx.nacked.entry(s).or_insert((now - Duration::from_secs(1), 0));
                    if e.1 < 3 && now.duration_since(e.0) >= Duration::from_millis(30) {
                        *e = (now, e.1 + 1);
                        want.push(s);
                    }
                }
                if !want.is_empty() {
                    nacks.push(Nack { stream_id: stream_id(*pid, stream::VIDEO), layer: *layer, seqs: want });
                }
            }
            if self.cfg.mode == Mode::Call {
                if let Some(d) = st.dest() {
                    for n in nacks {
                        let p = self.control_packet(&mut st, PacketType::Nack, &n.encode(), now);
                        out.push((p, d));
                    }
                }
            }
        }

        // 5. receiver report every 200 ms (loss, REMB, PLI)
        if now.duration_since(st.last_feedback) >= Duration::from_millis(200) && self.cfg.mode == Mode::Call {
            st.last_feedback = now;
            let (mut recv, mut lost) = (0u64, 0u64);
            let mut jitter = 0.0f64;
            for rx in st.audio_rx.values() {
                recv += rx.jb.received;
                lost += rx.jb.lost;
                jitter = jitter.max(rx.jb.jitter_ms());
            }
            for rx in st.video_rx.values() {
                recv += rx.jb.received;
                lost += rx.jb.lost;
                jitter = jitter.max(rx.jb.jitter_ms());
            }
            let (dr, dl) = (recv - st.last_counts.0, lost - st.last_counts.1);
            st.last_counts = (recv, lost);
            if dr + dl > 0 {
                let remb = if st.bwe.is_empty() { u32::MAX } else { st.bwe.values().map(|b| b.estimate_bps() as u64).sum::<u64>().min(u32::MAX as u64) as u32 };
                let pli_ok = st.last_pli.map_or(true, |t| now.duration_since(t) > Duration::from_millis(500));
                let want_keyframe = want_key && pli_ok;
                if want_keyframe {
                    st.last_pli = Some(now);
                }
                let fb = Feedback { remb_bps: remb, loss: ((dl * 255) / (dr + dl)) as u8, jitter_ms: jitter as u16, want_keyframe };
                if let Some(d) = st.dest() {
                    let p = self.control_packet(&mut st, PacketType::Feedback, &fb.encode(), now);
                    out.push((p, d));
                }
            }
        }

        // 6. SFU downlink report every 500 ms
        if self.cfg.mode == Mode::Sfu && now.duration_since(st.last_report) >= Duration::from_millis(500) {
            st.last_report = now;
            if let (Some(server), false) = (self.cfg.server, st.bwe.is_empty()) {
                let est = st.bwe.values().map(|b| b.estimate_bps() as u64).sum::<u64>().min(u32::MAX as u64) as u32;
                let mut p = Header::new(PacketType::Report, 0, 0, 0).to_bytes().to_vec();
                p.extend_from_slice(&Report { est_bps: est, max_spatial: 0xff }.encode());
                out.push((p, server));
            }
        }

        // 7. keepalive on the active path (also lets the peer detect liveness)
        if now.duration_since(st.last_keepalive) >= Duration::from_secs(1) {
            st.last_keepalive = now;
            if let Some(d) = st.dest() {
                let p = self.control_packet(&mut st, PacketType::Keepalive, &[], now);
                out.push((p, d));
            }
        }
        out
    }
}

impl MediaSession {
    /// Start the engine on an already-bound UDP socket.
    pub fn start(cfg: MediaConfig, sock: UdpSocket) -> (MediaSession, mpsc::UnboundedReceiver<MediaEvent>) {
        let now = Instant::now();
        let (tx, rx) = mpsc::unbounded_channel();
        let path = if cfg.mode == Mode::Sfu { cfg.server.map_or(Path::None, Path::Relay) } else { cfg.server.map_or(Path::None, Path::Relay) };
        let st = State {
            path,
            candidates: vec![],
            punch_until: None,
            last_p2p_rx: None,
            hello_acked: false,
            tx_seq: HashMap::new(),
            fec_tx: HashMap::new(),
            history: HashMap::new(),
            pacer: Pacer::new(cfg.video_start_bps + cfg.audio_bps, 2.5, now),
            replay: HashMap::new(),
            audio_rx: HashMap::new(),
            video_rx: HashMap::new(),
            bwe: HashMap::new(),
            loss_ctl: LossController::new(cfg.video_start_bps * 2, cfg.video_min_bps, cfg.video_max_bps * 2),
            remb: u32::MAX,
            video_target: cfg.video_start_bps,
            control_seq: 0,
            last_counts: (0, 0),
            last_hello: None,
            last_probe: None,
            last_nack: now,
            last_feedback: now,
            last_report: now,
            last_keepalive: now,
            last_pli: None,
            stats: SessionStats { video_target_bps: cfg.video_start_bps, ..Default::default() },
        };
        let i = Arc::new(Inner { cipher: MediaCipher::new(&cfg.key), cfg, sock: Arc::new(sock), events: tx, epoch: now, st: Mutex::new(st) });

        let ri = i.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 2048];
            loop {
                let Ok((n, src)) = ri.sock.recv_from(&mut buf).await else { continue };
                let out = ri.on_packet(&buf[..n], src, Instant::now());
                for (p, d) in out {
                    let _ = ri.sock.send_to(&p, d).await;
                }
                if ri.events.is_closed() {
                    break;
                }
            }
        });
        let ti = i.clone();
        tokio::spawn(async move {
            let mut iv = tokio::time::interval(Duration::from_millis(5));
            iv.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                iv.tick().await;
                if ti.events.is_closed() {
                    break;
                }
                let out = ti.tick(Instant::now());
                for (p, d) in out {
                    let _ = ti.sock.send_to(&p, d).await;
                }
            }
        });
        (MediaSession { i }, rx)
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.i.sock.local_addr()
    }

    /// Provide the peer's candidate addresses and start hole punching.
    pub fn set_peer_candidates(&self, addrs: Vec<SocketAddr>) {
        let mut st = self.i.st.lock().unwrap();
        for a in addrs {
            if !st.candidates.contains(&a) {
                st.candidates.push(a);
            }
        }
        st.punch_until = Some(Instant::now() + Duration::from_secs(8));
    }

    pub fn path(&self) -> Path {
        self.i.st.lock().unwrap().path
    }
    pub fn stats(&self) -> SessionStats {
        self.i.st.lock().unwrap().stats.clone()
    }
    /// Current video bitrate target from congestion control.
    pub fn video_target_bps(&self) -> u32 {
        self.i.st.lock().unwrap().video_target
    }

    pub async fn send_audio(&self, ts: u32, data: &[u8]) {
        let (pkts, dst) = {
            let mut st = self.i.st.lock().unwrap();
            let sid = stream_id(self.i.cfg.pid, stream::AUDIO);
            let seq = st.next_seq(sid, 0);
            let enc = st.fec_tx.entry((sid, 0)).or_insert_with(|| FecEncoder::new(0, 0));
            let (fec, parity) = enc.push(seq, data);
            let mut h = Header::new(PacketType::Audio, sid, seq, ts);
            h.fec = fec;
            let mut pkts = vec![self.i.cipher.seal(&h, data)];
            for p in parity {
                let mut ph = Header::new(PacketType::Audio, sid, p.group_start, ts);
                ph.flags = flags::PARITY;
                ph.fec = p.fec;
                pkts.push(self.i.cipher.seal(&ph, &p.payload));
            }
            st.stats.tx_packets += pkts.len() as u64;
            (pkts, st.dest())
        };
        if let Some(d) = dst {
            for p in pkts {
                let _ = self.i.sock.send_to(&p, d).await;
            }
        }
    }

    /// Send one encoded video frame on `layer` (spatial<<4 | temporal). Returns `false` if the frame
    /// was dropped because the pacer queue is too deep (sender should then emit a key frame later).
    pub async fn send_video_frame(&self, layer: u8, ts: u32, keyframe: bool, data: &[u8]) -> bool {
        let sid = stream_id(self.i.cfg.pid, stream::VIDEO);
        let (plan, dst) = {
            let mut st = self.i.st.lock().unwrap();
            let now = Instant::now();
            if !keyframe && st.pacer.backlog(now) > Duration::from_millis(150) {
                return false;
            }
            let mut plan: Vec<(Instant, Vec<u8>)> = vec![];
            for (frag, last) in fragment(data, DEFAULT_MTU_PAYLOAD) {
                let fl = (if last { flags::MARKER } else { 0 }) | (if keyframe { flags::KEYFRAME } else { 0 });
                let mut plain = Vec::with_capacity(frag.len() + 5);
                plain.push(fl);
                plain.extend_from_slice(&ts.to_be_bytes());
                plain.extend_from_slice(frag);
                let seq = st.next_seq(sid, layer);
                let enc = st.fec_tx.entry((sid, layer)).or_insert_with(|| FecEncoder::new(0, 0));
                let (fec, parity) = enc.push(seq, &plain);
                let mut h = Header::new(PacketType::Video, sid, seq, ts);
                h.layer = layer;
                h.flags = fl;
                h.fec = fec;
                let sealed = self.i.cipher.seal(&h, &plain);
                let hist = st.history.entry((sid, layer)).or_default();
                hist.push_back((h, plain));
                if hist.len() > 1024 {
                    hist.pop_front();
                }
                let t = st.pacer.schedule(now, sealed.len());
                plan.push((t, sealed));
                for p in parity {
                    let mut ph = Header::new(PacketType::Video, sid, p.group_start, ts);
                    ph.layer = layer;
                    ph.flags = flags::PARITY;
                    ph.fec = p.fec;
                    let sealed = self.i.cipher.seal(&ph, &p.payload);
                    let t = st.pacer.schedule(now, sealed.len());
                    plan.push((t, sealed));
                }
            }
            st.stats.tx_packets += plan.len() as u64;
            (plan, st.dest())
        };
        if let Some(d) = dst {
            for (t, p) in plan {
                if t > Instant::now() {
                    tokio::time::sleep_until(tokio::time::Instant::from_std(t)).await;
                }
                let _ = self.i.sock.send_to(&p, d).await;
            }
        }
        true
    }
}
