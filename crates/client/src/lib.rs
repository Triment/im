//! XMP client core. UI layers (Swift/Kotlin/Flutter/Tauri) bind to this crate.
//!
//! * [`Client`]: QUIC control connection, chat, friends, files, call/room signalling.
//! * [`session::MediaSession`]: the UDP media engine (P2P + relay + SFU, FEC, NACK, congestion control).
//! * [`call`]: orchestration of 1:1 calls and room joins on top of both.

pub mod call;
pub mod files;
pub mod session;

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};
use xmp_proto::control::{Frame, Friend, Msg, StoredMessage, MAX_FRAME};

pub const ALPN: &[u8] = b"xmp/1";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("server error {0}: {1}")]
    Api(u16, String),
    #[error("connection closed")]
    Closed,
    #[error("timed out")]
    Timeout,
    #[error("{0}")]
    Other(String),
}
pub type Result<T> = std::result::Result<T, Error>;

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        Error::Other(e.to_string())
    }
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Other(e.to_string())
    }
}
pub(crate) fn other<E: std::fmt::Display>(e: E) -> Error {
    Error::Other(e.to_string())
}

pub struct ConnectOpts {
    /// `host:port` of the QUIC control endpoint.
    pub server: String,
    /// TLS server name to verify.
    pub server_name: String,
    /// DER certificate to pin (self-hosted deployments). `None` = public web PKI roots.
    pub pinned_cert: Option<Vec<u8>>,
}

pub enum Auth {
    Register { username: String, password: String, display_name: String },
    Login { username: String, password: String, device_name: String },
    Resume { token: String },
}

type SigTx = mpsc::UnboundedSender<(u8, Vec<u8>)>;

struct Control {
    out: mpsc::UnboundedSender<Vec<u8>>,
    pending: Mutex<HashMap<u32, oneshot::Sender<Msg>>>,
    next_id: AtomicU32,
    calls: Mutex<HashMap<u64, SigTx>>,
    _conn: quinn::Connection,
}

impl Control {
    async fn request(&self, msg: Msg) -> Result<Msg> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        if self.out.send(Frame::new(id, msg).encode()).is_err() {
            self.pending.lock().unwrap().remove(&id);
            return Err(Error::Closed);
        }
        match tokio::time::timeout(Duration::from_secs(30), rx).await {
            Ok(Ok(Msg::Err { code, message })) => Err(Error::Api(code, message)),
            Ok(Ok(m)) => Ok(m),
            Ok(Err(_)) => Err(Error::Closed),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(Error::Timeout)
            }
        }
    }
}

#[derive(Clone)]
pub struct Client {
    ctl: Arc<Control>,
    pub user_id: i64,
    pub token: String,
    pub(crate) server_host: String,
}

fn client_config(opts: &ConnectOpts) -> Result<quinn::ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    match &opts.pinned_cert {
        Some(der) => roots.add(rustls::pki_types::CertificateDer::from(der.clone())).map_err(other)?,
        None => roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned()),
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut crypto = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(other)?
        .with_root_certificates(roots)
        .with_no_client_auth();
    crypto.alpn_protocols = vec![ALPN.to_vec()];
    let q = quinn::crypto::rustls::QuicClientConfig::try_from(crypto).map_err(other)?;
    let mut cfg = quinn::ClientConfig::new(Arc::new(q));
    let mut tc = quinn::TransportConfig::default();
    tc.keep_alive_interval(Some(Duration::from_secs(5)));
    tc.max_idle_timeout(Some(Duration::from_secs(30).try_into().map_err(other)?));
    cfg.transport_config(Arc::new(tc));
    Ok(cfg)
}

impl Client {
    /// Connect, authenticate and return the client plus a stream of server pushes
    /// (`PushMessage`, `Presence`, incoming `CallSignal`s, `RoomEvent`s).
    pub async fn connect(opts: ConnectOpts, auth: Auth) -> Result<(Client, mpsc::UnboundedReceiver<Msg>)> {
        let addr: SocketAddr = tokio::net::lookup_host(&opts.server)
            .await?
            .next()
            .ok_or_else(|| Error::Other("cannot resolve server".into()))?;
        let bind: SocketAddr = if addr.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }.parse().unwrap();
        let mut ep = quinn::Endpoint::client(bind)?;
        ep.set_default_client_config(client_config(&opts)?);
        let conn = ep.connect(addr, &opts.server_name).map_err(other)?.await.map_err(other)?;
        let (mut send, mut recv) = conn.open_bi().await.map_err(other)?;

        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        tokio::spawn(async move {
            while let Some(b) = out_rx.recv().await {
                if send.write_all(&b).await.is_err() {
                    break;
                }
            }
        });
        let ctl = Arc::new(Control {
            out: out_tx,
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU32::new(1),
            calls: Mutex::new(HashMap::new()),
            _conn: conn,
        });
        let (ev_tx, ev_rx) = mpsc::unbounded_channel();
        let c2 = ctl.clone();
        tokio::spawn(async move {
            let _ep = ep; // keep the endpoint alive as long as the connection
            loop {
                let mut len = [0u8; 4];
                if recv.read_exact(&mut len).await.is_err() {
                    break;
                }
                let n = u32::from_be_bytes(len) as usize;
                if !(5..=MAX_FRAME + 64).contains(&n) {
                    break;
                }
                let mut body = vec![0u8; n];
                if recv.read_exact(&mut body).await.is_err() {
                    break;
                }
                let Ok(frame) = Frame::decode(&body) else { break };
                if frame.req_id != 0 {
                    if let Some(tx) = c2.pending.lock().unwrap().remove(&frame.req_id) {
                        let _ = tx.send(frame.msg);
                    }
                    continue;
                }
                if let Msg::CallSignal { call_id, kind, data, .. } = &frame.msg {
                    let routed = c2.calls.lock().unwrap().get(call_id).map(|tx| tx.send((*kind, data.clone())).is_ok());
                    if routed == Some(true) {
                        continue;
                    }
                }
                let _ = ev_tx.send(frame.msg);
            }
            c2.pending.lock().unwrap().clear(); // wakes waiters with Closed
        });

        let req = match auth {
            Auth::Register { username, password, display_name } => Msg::Register { username, password, display_name },
            Auth::Login { username, password, device_name } => Msg::Login { username, password, device_name },
            Auth::Resume { token } => Msg::Resume { token },
        };
        match ctl.request(req).await? {
            Msg::AuthOk { user_id, token } => {
                let host = opts.server.rsplit_once(':').map(|(h, _)| h.to_string()).unwrap_or(opts.server.clone());
                Ok((Client { ctl, user_id, token, server_host: host }, ev_rx))
            }
            m => Err(Error::Other(format!("unexpected auth response {m:?}"))),
        }
    }

    async fn expect_ok(&self, msg: Msg) -> Result<()> {
        match self.ctl.request(msg).await? {
            Msg::Ok => Ok(()),
            m => Err(Error::Other(format!("unexpected response {m:?}"))),
        }
    }

    pub async fn ping(&self) -> Result<Duration> {
        let t = std::time::Instant::now();
        self.ctl.request(Msg::Ping).await?;
        Ok(t.elapsed())
    }

    pub async fn friend_request(&self, username: &str) -> Result<()> {
        self.expect_ok(Msg::FriendRequest { username: username.into() }).await
    }
    pub async fn friend_accept(&self, user_id: i64) -> Result<()> {
        self.expect_ok(Msg::FriendAccept { user_id }).await
    }
    pub async fn friends(&self) -> Result<Vec<Friend>> {
        match self.ctl.request(Msg::FriendList).await? {
            Msg::FriendListResp { friends } => Ok(friends),
            m => Err(Error::Other(format!("unexpected {m:?}"))),
        }
    }

    /// Send a message; `client_id` makes retries idempotent. Returns `(conv_id, seq, ts)`.
    pub async fn send_message(&self, to_user: i64, kind: u8, body: Vec<u8>, client_id: u64) -> Result<(i64, i64, i64)> {
        match self.ctl.request(Msg::SendMessage { to_user, kind, body, client_id }).await? {
            Msg::MessageAck { conv_id, seq, ts } => Ok((conv_id, seq, ts)),
            m => Err(Error::Other(format!("unexpected {m:?}"))),
        }
    }
    pub async fn send_text(&self, to_user: i64, text: &str) -> Result<(i64, i64, i64)> {
        self.send_message(to_user, xmp_proto::control::msg_kind::TEXT, text.as_bytes().to_vec(), rand_id()).await
    }
    pub async fn sync(&self, conv_id: i64, after_seq: i64, limit: u32) -> Result<Vec<StoredMessage>> {
        match self.ctl.request(Msg::Sync { conv_id, after_seq, limit }).await? {
            Msg::SyncResp { messages } => Ok(messages),
            m => Err(Error::Other(format!("unexpected {m:?}"))),
        }
    }
    pub async fn conversations(&self) -> Result<Vec<xmp_proto::control::ConvSummary>> {
        match self.ctl.request(Msg::ConvList).await? {
            Msg::ConvListResp { convs } => Ok(convs),
            m => Err(Error::Other(format!("unexpected {m:?}"))),
        }
    }

    pub(crate) async fn request(&self, m: Msg) -> Result<Msg> {
        self.ctl.request(m).await
    }
    pub(crate) fn register_call(&self, call_id: u64) -> mpsc::UnboundedReceiver<(u8, Vec<u8>)> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.ctl.calls.lock().unwrap().insert(call_id, tx);
        rx
    }
    pub(crate) fn unregister_call(&self, call_id: u64) {
        self.ctl.calls.lock().unwrap().remove(&call_id);
    }
}

pub fn rand_id() -> u64 {
    rand::random::<u64>() >> 1 // keep it positive when stored as i64 server-side
}
