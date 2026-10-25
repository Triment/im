//! UDP media plane: STUN-like address reflection, 1:1 relay and SFU forwarding.
//!
//! The server never decrypts media: it routes on the 16-byte clear header.
//!  * `Hello{token}` binds a UDP source address to a token minted over the control channel.
//!  * 1:1 call: every media packet from one bound endpoint is forwarded to the peer's endpoint.
//!  * Room (SFU): audio goes to everyone, video only up to each subscriber's selected spatial layer.
//! The hot path takes a read lock and uses non-blocking `try_send_to`; a full socket buffer
//! simply drops the packet, which is the right behaviour for real-time media.

use rand::RngCore;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        atomic::{AtomicU64, AtomicU8, Ordering},
        Arc, RwLock,
    },
};
use tokio::net::UdpSocket;
use xmp_proto::{
    control::Participant,
    media::{Header, PacketType, Report},
};

pub type Token = [u8; 16];

/// Cumulative downlink bitrate needed to receive spatial layers `0..=i` of one publisher.
pub const LAYER_BPS: [u32; 4] = [250_000, 700_000, 2_000_000, 5_000_000];
pub const MAX_ROOM_SIZE: usize = 64;

pub fn pick_layer(per_stream_bps: u32) -> u8 {
    LAYER_BPS.iter().rposition(|&b| b <= per_stream_bps).unwrap_or(0) as u8
}

#[derive(Clone)]
enum Bind {
    Call { call_id: u64, user: i64 },
    Room { room_id: u64, pid: u32 },
}

struct Slot {
    bind: Bind,
    addr: Option<SocketAddr>,
}

struct Member {
    user: i64,
    token: Token,
    /// Highest spatial layer this member wants to receive.
    max_spatial: Arc<AtomicU8>,
}

struct Room {
    owner: i64,
    key: [u8; 32],
    next_pid: u32,
    members: HashMap<u32, Member>,
}

struct Call {
    a: i64,
    b: i64,
    tokens: HashMap<i64, Token>,
}

#[derive(Default)]
struct Inner {
    slots: HashMap<Token, Slot>,
    by_addr: HashMap<SocketAddr, Token>,
    calls: HashMap<u64, Call>,
    rooms: HashMap<u64, Room>,
}

#[derive(Default)]
pub struct Stats {
    pub relayed: AtomicU64,
    pub sfu_forwarded: AtomicU64,
    pub dropped: AtomicU64,
    pub bytes: AtomicU64,
}

#[derive(Default)]
pub struct MediaState {
    inner: RwLock<Inner>,
    pub stats: Stats,
}

pub struct RoomJoin {
    pub pid: u32,
    pub token: Token,
    pub key: [u8; 32],
    pub participants: Vec<Participant>,
    pub others: Vec<i64>,
}

#[derive(Default)]
pub struct Cleanup {
    /// (call_id, peer to notify)
    pub calls: Vec<(u64, i64)>,
    /// (room_id, pid, users to notify, room now empty)
    pub rooms: Vec<(u64, u32, Vec<i64>, bool)>,
}

fn rand_token() -> Token {
    let mut t = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut t);
    t
}

impl MediaState {
    // ---- 1:1 calls ----------------------------------------------------------------------

    pub fn call_create(&self, call_id: u64, a: i64, b: i64) -> bool {
        let mut g = self.inner.write().unwrap();
        if g.calls.contains_key(&call_id) {
            return false;
        }
        g.calls.insert(call_id, Call { a, b, tokens: HashMap::new() });
        true
    }

    pub fn call_peer(&self, call_id: u64, user: i64) -> Option<i64> {
        let g = self.inner.read().unwrap();
        let c = g.calls.get(&call_id)?;
        if c.a == user {
            Some(c.b)
        } else if c.b == user {
            Some(c.a)
        } else {
            None
        }
    }

    pub fn call_alloc(&self, call_id: u64, user: i64) -> Option<Token> {
        let mut g = self.inner.write().unwrap();
        let c = g.calls.get_mut(&call_id)?;
        if c.a != user && c.b != user {
            return None;
        }
        if let Some(t) = c.tokens.get(&user) {
            return Some(*t);
        }
        let t = rand_token();
        c.tokens.insert(user, t);
        g.slots.insert(t, Slot { bind: Bind::Call { call_id, user }, addr: None });
        Some(t)
    }

    pub fn call_end(&self, call_id: u64) -> bool {
        let mut g = self.inner.write().unwrap();
        let Some(c) = g.calls.remove(&call_id) else { return false };
        for t in c.tokens.values() {
            if let Some(s) = g.slots.remove(t) {
                if let Some(a) = s.addr {
                    g.by_addr.remove(&a);
                }
            }
        }
        true
    }

    // ---- rooms (SFU) -------------------------------------------------------------------

    pub fn room_create(&self, owner: i64) -> u64 {
        let mut g = self.inner.write().unwrap();
        loop {
            let id = rand::thread_rng().next_u64() | 1;
            if let std::collections::hash_map::Entry::Vacant(e) = g.rooms.entry(id) {
                let mut key = [0u8; 32];
                rand::thread_rng().fill_bytes(&mut key);
                e.insert(Room { owner, key, next_pid: 1, members: HashMap::new() });
                return id;
            }
        }
    }

    pub fn room_owner(&self, room_id: u64) -> Option<i64> {
        self.inner.read().unwrap().rooms.get(&room_id).map(|r| r.owner)
    }

    pub fn room_join(&self, room_id: u64, user: i64) -> Result<RoomJoin, &'static str> {
        let mut g = self.inner.write().unwrap();
        let room = g.rooms.get_mut(&room_id).ok_or("no such room")?;
        if room.members.len() >= MAX_ROOM_SIZE {
            return Err("room full");
        }
        if room.members.values().any(|m| m.user == user) {
            return Err("already in room");
        }
        let pid = room.next_pid;
        room.next_pid += 1;
        let token = rand_token();
        let participants = room.members.iter().map(|(pid, m)| Participant { pid: *pid, user_id: m.user }).collect();
        let others = room.members.values().map(|m| m.user).collect();
        let key = room.key;
        room.members.insert(pid, Member { user, token, max_spatial: Arc::new(AtomicU8::new(0xff)) });
        g.slots.insert(token, Slot { bind: Bind::Room { room_id, pid }, addr: None });
        Ok(RoomJoin { pid, token, key, participants, others })
    }

    /// Returns `(pid, users to notify, room_is_empty)`.
    pub fn room_leave(&self, room_id: u64, user: i64) -> Option<(u32, Vec<i64>, bool)> {
        let mut g = self.inner.write().unwrap();
        let room = g.rooms.get_mut(&room_id)?;
        let pid = *room.members.iter().find(|(_, m)| m.user == user)?.0;
        let m = room.members.remove(&pid).unwrap();
        let others: Vec<i64> = room.members.values().map(|m| m.user).collect();
        let empty = room.members.is_empty();
        if empty {
            g.rooms.remove(&room_id);
        }
        if let Some(s) = g.slots.remove(&m.token) {
            if let Some(a) = s.addr {
                g.by_addr.remove(&a);
            }
        }
        Some((pid, others, empty))
    }

    /// Remove a user from everything (last control connection dropped).
    pub fn user_cleanup(&self, user: i64) -> Cleanup {
        let (calls, rooms): (Vec<u64>, Vec<u64>) = {
            let g = self.inner.read().unwrap();
            (
                g.calls.iter().filter(|(_, c)| c.a == user || c.b == user).map(|(id, _)| *id).collect(),
                g.rooms.iter().filter(|(_, r)| r.members.values().any(|m| m.user == user)).map(|(id, _)| *id).collect(),
            )
        };
        let mut out = Cleanup::default();
        for id in calls {
            if let Some(peer) = self.call_peer(id, user) {
                if self.call_end(id) {
                    out.calls.push((id, peer));
                }
            }
        }
        for id in rooms {
            if let Some((pid, others, empty)) = self.room_leave(id, user) {
                out.rooms.push((id, pid, others, empty));
            }
        }
        out
    }

    // ---- UDP hot path --------------------------------------------------------------------

    pub fn handle_packet(&self, sock: &UdpSocket, pkt: &[u8], src: SocketAddr) {
        let Ok((h, payload)) = Header::parse(pkt) else {
            self.stats.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        };
        match h.ptype {
            PacketType::Hello => self.on_hello(sock, payload, src),
            PacketType::Report => self.on_report(payload, src),
            PacketType::HelloAck => {}
            _ => self.forward(sock, &h, pkt, src),
        }
    }

    fn on_hello(&self, sock: &UdpSocket, payload: &[u8], src: SocketAddr) {
        let Ok(token) = <Token>::try_from(payload.get(..16).unwrap_or(&[])) else { return };
        {
            let mut g = self.inner.write().unwrap();
            let Some(slot) = g.slots.get_mut(&token) else { return };
            let old = slot.addr.replace(src);
            if let Some(o) = old {
                g.by_addr.remove(&o);
            }
            g.by_addr.insert(src, token);
        }
        let mut ack = Header::new(PacketType::HelloAck, 0, 0, 0).to_bytes().to_vec();
        ack.extend_from_slice(src.to_string().as_bytes());
        let _ = sock.try_send_to(&ack, src);
    }

    fn on_report(&self, payload: &[u8], src: SocketAddr) {
        let Ok(rep) = Report::decode(payload) else { return };
        let g = self.inner.read().unwrap();
        let Some(Bind::Room { room_id, pid }) = g.by_addr.get(&src).and_then(|t| g.slots.get(t)).map(|s| s.bind.clone()) else { return };
        let Some(room) = g.rooms.get(&room_id) else { return };
        let Some(me) = room.members.get(&pid) else { return };
        let publishers = room.members.len().saturating_sub(1).max(1) as u32;
        let layer = pick_layer(rep.est_bps / publishers).min(rep.max_spatial);
        me.max_spatial.store(layer, Ordering::Relaxed);
    }

    fn forward(&self, sock: &UdpSocket, h: &Header, pkt: &[u8], src: SocketAddr) {
        let g = self.inner.read().unwrap();
        let Some(slot) = g.by_addr.get(&src).and_then(|t| g.slots.get(t)) else {
            self.stats.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        };
        match &slot.bind {
            Bind::Call { call_id, user } => {
                let dest = g.calls.get(call_id).and_then(|c| {
                    let peer = if c.a == *user { c.b } else { c.a };
                    let tok = c.tokens.get(&peer)?;
                    g.slots.get(tok)?.addr
                });
                match dest {
                    Some(d) if sock.try_send_to(pkt, d).is_ok() => {
                        self.stats.relayed.fetch_add(1, Ordering::Relaxed);
                        self.stats.bytes.fetch_add(pkt.len() as u64, Ordering::Relaxed);
                    }
                    _ => {
                        self.stats.dropped.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            Bind::Room { room_id, pid } => {
                if !matches!(h.ptype, PacketType::Audio | PacketType::Video) {
                    return; // keepalives etc. only refresh the NAT mapping
                }
                if h.participant() != *pid {
                    self.stats.dropped.fetch_add(1, Ordering::Relaxed); // spoofed stream id
                    return;
                }
                let Some(room) = g.rooms.get(room_id) else { return };
                for (opid, m) in &room.members {
                    if opid == pid {
                        continue;
                    }
                    if h.ptype == PacketType::Video && h.spatial() > m.max_spatial.load(Ordering::Relaxed) {
                        continue;
                    }
                    if let Some(addr) = g.slots.get(&m.token).and_then(|s| s.addr) {
                        if sock.try_send_to(pkt, addr).is_ok() {
                            self.stats.sfu_forwarded.fetch_add(1, Ordering::Relaxed);
                            self.stats.bytes.fetch_add(pkt.len() as u64, Ordering::Relaxed);
                        } else {
                            self.stats.dropped.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            }
        }
    }
}

pub async fn run_udp(state: Arc<crate::AppState>, sock: Arc<UdpSocket>) -> anyhow::Result<()> {
    let st = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            let s = &st.media.stats;
            tracing::info!(
                relayed = s.relayed.load(Ordering::Relaxed),
                sfu = s.sfu_forwarded.load(Ordering::Relaxed),
                dropped = s.dropped.load(Ordering::Relaxed),
                mbytes = s.bytes.load(Ordering::Relaxed) / 1_000_000,
                "media stats"
            );
        }
    });
    let mut buf = vec![0u8; 2048];
    loop {
        let (n, src) = sock.recv_from(&mut buf).await?;
        state.media.handle_packet(&sock, &buf[..n], src);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;
    use xmp_proto::media::{flags, stream_id};

    async fn client(server: SocketAddr, token: Token) -> UdpSocket {
        let s = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let mut p = Header::new(PacketType::Hello, 0, 0, 0).to_bytes().to_vec();
        p.extend_from_slice(&token);
        s.send_to(&p, server).await.unwrap();
        let mut buf = [0u8; 256];
        let (n, _) = timeout(Duration::from_secs(1), s.recv_from(&mut buf)).await.unwrap().unwrap();
        let (h, body) = Header::parse(&buf[..n]).unwrap();
        assert_eq!(h.ptype, PacketType::HelloAck);
        assert_eq!(std::str::from_utf8(body).unwrap(), s.local_addr().unwrap().to_string());
        s
    }

    async fn serve() -> (Arc<MediaState>, SocketAddr) {
        let sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let addr = sock.local_addr().unwrap();
        let st = Arc::new(MediaState::default());
        let st2 = st.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 2048];
            loop {
                let (n, src) = sock.recv_from(&mut buf).await.unwrap();
                st2.handle_packet(&sock, &buf[..n], src);
            }
        });
        (st, addr)
    }

    fn pkt(pt: PacketType, sid: u32, layer: u8) -> Vec<u8> {
        let mut h = Header::new(pt, sid, 1, 1);
        h.layer = layer << 4;
        h.flags = flags::MARKER;
        let mut v = h.to_bytes().to_vec();
        v.extend_from_slice(b"ciphertext");
        v
    }

    #[test]
    fn layer_selection() {
        assert_eq!(pick_layer(0), 0);
        assert_eq!(pick_layer(300_000), 0);
        assert_eq!(pick_layer(800_000), 1);
        assert_eq!(pick_layer(2_500_000), 2);
        assert_eq!(pick_layer(50_000_000), 3);
    }

    #[tokio::test]
    async fn relays_between_call_peers() {
        let (st, addr) = serve().await;
        assert!(st.call_create(1, 10, 20));
        let (ta, tb) = (st.call_alloc(1, 10).unwrap(), st.call_alloc(1, 20).unwrap());
        assert!(st.call_alloc(1, 30).is_none());
        let (a, b) = (client(addr, ta).await, client(addr, tb).await);
        let p = pkt(PacketType::Audio, stream_id(1, 0), 0);
        a.send_to(&p, addr).await.unwrap();
        let mut buf = [0u8; 256];
        let (n, _) = timeout(Duration::from_secs(1), b.recv_from(&mut buf)).await.unwrap().unwrap();
        assert_eq!(&buf[..n], &p[..]);
        // unbound sender is dropped
        let rogue = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        rogue.send_to(&p, addr).await.unwrap();
        assert!(timeout(Duration::from_millis(200), b.recv_from(&mut buf)).await.is_err());
        assert!(st.call_end(1));
    }

    #[tokio::test]
    async fn sfu_forwards_with_layer_selection() {
        let (st, addr) = serve().await;
        let room = st.room_create(1);
        let (j1, j2, j3) = (st.room_join(room, 1).unwrap(), st.room_join(room, 2).unwrap(), st.room_join(room, 3).unwrap());
        assert!(st.room_join(room, 2).is_err());
        assert_eq!(j3.participants.len(), 2);
        let (c1, c2, c3) = (client(addr, j1.token).await, client(addr, j2.token).await, client(addr, j3.token).await);
        // subscriber 3 has a thin downlink: 2 publishers share 300 kbps => 150k each => layer 0
        let mut rep = Header::new(PacketType::Report, 0, 0, 0).to_bytes().to_vec();
        rep.extend_from_slice(&Report { est_bps: 300_000, max_spatial: 0xff }.encode());
        c3.send_to(&rep, addr).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        let sid = stream_id(j1.pid, 1);
        let hi = pkt(PacketType::Video, sid, 2);
        let lo = pkt(PacketType::Video, sid, 0);
        c1.send_to(&hi, addr).await.unwrap();
        c1.send_to(&lo, addr).await.unwrap();
        let mut buf = [0u8; 256];
        // c2 (no report, uncapped) gets both
        for want in [&hi, &lo] {
            let (n, _) = timeout(Duration::from_secs(1), c2.recv_from(&mut buf)).await.unwrap().unwrap();
            assert_eq!(&buf[..n], &want[..]);
        }
        // c3 only gets the base layer
        let (n, _) = timeout(Duration::from_secs(1), c3.recv_from(&mut buf)).await.unwrap().unwrap();
        assert_eq!(&buf[..n], &lo[..]);
        // spoofed stream id is dropped
        c1.send_to(&pkt(PacketType::Audio, stream_id(j2.pid, 0), 0), addr).await.unwrap();
        assert!(timeout(Duration::from_millis(200), c2.recv_from(&mut buf)).await.is_err());
        // leaving frees the slot
        let (pid, others, empty) = st.room_leave(room, 3).unwrap();
        assert_eq!((pid, others.len(), empty), (j3.pid, 2, false));
    }

    #[test]
    fn disconnect_cleanup() {
        let st = MediaState::default();
        st.call_create(5, 1, 2);
        let room = st.room_create(1);
        st.room_join(room, 1).unwrap();
        let c = st.user_cleanup(1);
        assert_eq!(c.calls, vec![(5, 2)]);
        assert_eq!(c.rooms.len(), 1);
        assert!(c.rooms[0].3, "room should be empty");
        assert!(st.room_owner(room).is_none());
    }
}
