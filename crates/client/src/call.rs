//! 1:1 call and conference-room orchestration.
//!
//! 1:1 call flow (all signalling relayed by the server, which only sees opaque bytes):
//! `INVITE -> ACCEPT -> Noise XX (3 msgs) -> MediaAlloc + Hello (learn reflexive addr)
//!  -> exchange CANDIDATES -> media starts on the relay immediately, P2P upgrade in the background`.

use crate::{
    other,
    session::{MediaConfig, MediaEvent, MediaSession, Mode},
    Client, Error, Result,
};
use std::{net::SocketAddr, time::Duration};
use tokio::{net::UdpSocket, sync::mpsc, time::timeout};
use xmp_media::crypto::{generate_static_key, Handshake};
use xmp_proto::control::{decode_candidates, encode_candidates, sig, Msg, Participant};

type SigRx = mpsc::UnboundedReceiver<(u8, Vec<u8>)>;

pub struct Call {
    pub call_id: u64,
    pub peer: i64,
    pub video: bool,
    /// Short authentication string: users compare it out-of-band to rule out a MITM.
    pub sas: String,
    pub media: MediaSession,
    pub events: mpsc::UnboundedReceiver<MediaEvent>,
    /// Remaining signalling (e.g. the peer's HANGUP).
    pub signals: SigRx,
    client: Client,
}

pub struct Room {
    pub room_id: u64,
    pub pid: u32,
    pub participants: Vec<Participant>,
    pub media: MediaSession,
    pub events: mpsc::UnboundedReceiver<MediaEvent>,
}

async fn wait_sig(rx: &mut SigRx, want: u8, secs: u64) -> Result<Vec<u8>> {
    let fut = async {
        while let Some((kind, data)) = rx.recv().await {
            match kind {
                k if k == want => return Ok(data),
                sig::REJECT => return Err(Error::Other("call rejected".into())),
                sig::HANGUP => return Err(Error::Other("peer hung up".into())),
                _ => {}
            }
        }
        Err(Error::Closed)
    };
    timeout(Duration::from_secs(secs), fut).await.map_err(|_| Error::Timeout)?
}

async fn local_ip_toward(server: SocketAddr) -> Option<std::net::IpAddr> {
    let s = UdpSocket::bind(if server.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }).await.ok()?;
    s.connect(server).await.ok()?;
    s.local_addr().ok().map(|a| a.ip())
}

impl Client {
    async fn signal(&self, call_id: u64, peer: i64, kind: u8, data: Vec<u8>) -> Result<()> {
        match self.request(Msg::CallSignal { call_id, peer, kind, data }).await? {
            Msg::Ok => Ok(()),
            m => Err(other(format!("unexpected {m:?}"))),
        }
    }

    async fn resolve_udp(&self, advertised: &str) -> Result<SocketAddr> {
        let mut a: SocketAddr = match advertised.parse() {
            Ok(a) => a,
            Err(_) => tokio::net::lookup_host(advertised).await?.next().ok_or_else(|| other("cannot resolve media server"))?,
        };
        if a.ip().is_unspecified() {
            // server advertised its bind address: use the host we reached it at
            a = tokio::net::lookup_host((self.server_host.as_str(), a.port())).await?.next().ok_or_else(|| other("cannot resolve media server"))?;
        }
        Ok(a)
    }

    async fn start_media(&self, cfg: MediaConfig) -> Result<(MediaSession, mpsc::UnboundedReceiver<MediaEvent>)> {
        let sock = UdpSocket::bind("0.0.0.0:0").await?;
        Ok(MediaSession::start(cfg, sock))
    }

    /// Place a call. Resolves once the peer accepted and media is flowing (on the relay at least).
    pub async fn start_call(&self, peer: i64, video: bool) -> Result<Call> {
        let call_id = crate::rand_id();
        let mut sigs = self.register_call(call_id);
        match self.run_call(call_id, peer, video, true, &mut sigs).await {
            Ok((sas, media, events)) => Ok(Call { call_id, peer, video, sas, media, events, signals: sigs, client: self.clone() }),
            Err(e) => {
                let _ = self.signal(call_id, peer, sig::HANGUP, vec![]).await;
                self.unregister_call(call_id);
                Err(e)
            }
        }
    }

    /// Accept an incoming `CallSignal{kind: INVITE}` (`video` = `data[0] != 0` of the invite).
    pub async fn accept_call(&self, call_id: u64, peer: i64, video: bool) -> Result<Call> {
        let mut sigs = self.register_call(call_id);
        match self.run_call(call_id, peer, video, false, &mut sigs).await {
            Ok((sas, media, events)) => Ok(Call { call_id, peer, video, sas, media, events, signals: sigs, client: self.clone() }),
            Err(e) => {
                let _ = self.signal(call_id, peer, sig::HANGUP, vec![]).await;
                self.unregister_call(call_id);
                Err(e)
            }
        }
    }

    pub async fn reject_call(&self, call_id: u64, peer: i64) -> Result<()> {
        self.signal(call_id, peer, sig::REJECT, vec![]).await
    }

    async fn run_call(&self, call_id: u64, peer: i64, video: bool, initiator: bool, sigs: &mut SigRx) -> Result<(String, MediaSession, mpsc::UnboundedReceiver<MediaEvent>)> {
        // 1. invite / accept
        if initiator {
            self.signal(call_id, peer, sig::INVITE, vec![video as u8]).await?;
            wait_sig(sigs, sig::ACCEPT, 45).await?;
        } else {
            self.signal(call_id, peer, sig::ACCEPT, vec![]).await?;
        }
        // 2. Noise XX over the relayed signalling channel
        let mut hs = Handshake::new(initiator, &generate_static_key()).map_err(other)?;
        if initiator {
            self.signal(call_id, peer, sig::NOISE, hs.write().map_err(other)?).await?;
            hs.read(&wait_sig(sigs, sig::NOISE, 10).await?).map_err(other)?;
            self.signal(call_id, peer, sig::NOISE, hs.write().map_err(other)?).await?;
        } else {
            hs.read(&wait_sig(sigs, sig::NOISE, 10).await?).map_err(other)?;
            self.signal(call_id, peer, sig::NOISE, hs.write().map_err(other)?).await?;
            hs.read(&wait_sig(sigs, sig::NOISE, 10).await?).map_err(other)?;
        }
        let est = hs.finish().map_err(other)?;

        // 3. media allocation on the server, bind and learn our reflexive address
        let (token, udp) = match self.request(Msg::MediaAlloc { call_id }).await? {
            Msg::MediaAllocResp { token, udp_addr } => (token, udp_addr),
            m => return Err(other(format!("unexpected {m:?}"))),
        };
        let server = self.resolve_udp(&udp).await?;
        let mut cfg = MediaConfig::new(est.media_key, if initiator { 1 } else { 2 });
        cfg.server = Some(server);
        cfg.token = Some(token);
        cfg.mode = Mode::Call;
        let (media, mut events) = self.start_media(cfg).await?;
        let port = media.local_addr()?.port();
        let mut mine = vec![];
        if let Some(ip) = local_ip_toward(server).await {
            mine.push(SocketAddr::new(ip, port).to_string());
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while let Ok(Some(ev)) = tokio::time::timeout_at(deadline, events.recv()).await {
            if let MediaEvent::Reflexive(a) = ev {
                mine.push(a.to_string());
                break;
            }
        }

        // 4. exchange candidates and start punching
        self.signal(call_id, peer, sig::CANDIDATES, encode_candidates(&mine)).await?;
        let theirs = decode_candidates(&wait_sig(sigs, sig::CANDIDATES, 15).await?).map_err(other)?;
        media.set_peer_candidates(theirs.iter().filter_map(|s| s.parse().ok()).collect());
        Ok((est.sas, media, events))
    }

    pub async fn create_room(&self) -> Result<u64> {
        match self.request(Msg::RoomCreate).await? {
            Msg::RoomCreated { room_id } => Ok(room_id),
            m => Err(other(format!("unexpected {m:?}"))),
        }
    }

    pub async fn join_room(&self, room_id: u64) -> Result<Room> {
        match self.request(Msg::RoomJoin { room_id }).await? {
            Msg::RoomJoined { pid, token, udp_addr, media_key, participants } => {
                let mut cfg = MediaConfig::new(media_key, pid);
                cfg.server = Some(self.resolve_udp(&udp_addr).await?);
                cfg.token = Some(token);
                cfg.mode = Mode::Sfu;
                let (media, events) = self.start_media(cfg).await?;
                Ok(Room { room_id, pid, participants, media, events })
            }
            m => Err(other(format!("unexpected {m:?}"))),
        }
    }

    pub async fn leave_room(&self, room_id: u64) -> Result<()> {
        self.request(Msg::RoomLeave { room_id }).await.map(|_| ())
    }
}

impl Call {
    pub async fn hangup(self) {
        let _ = self.client.signal(self.call_id, self.peer, sig::HANGUP, vec![]).await;
        self.client.unregister_call(self.call_id);
    }
}
