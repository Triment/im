//! QUIC control plane: one long-lived bidirectional stream per connection carrying
//! length-prefixed `Frame`s. Requests are answered with the same `req_id`; pushes use 0.

use crate::{
    db::{self, ApiErr, ApiResult},
    files::{BlobStore, FileError},
    AppState,
};
use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::mpsc::{self, UnboundedSender};
use xmp_proto::control::{err, msg_kind, sig, Frame, Msg, Participant, FILE_CHUNK_SIZE, MAX_FRAME};

pub const ALPN: &[u8] = b"xmp/1";
static CONN_IDS: AtomicU64 = AtomicU64::new(1);
const MAX_TEXT_BODY: usize = 64 * 1024;

pub async fn run(state: Arc<AppState>, addr: SocketAddr, data_dir: PathBuf) -> anyhow::Result<()> {
    std::fs::create_dir_all(&data_dir)?;
    let mut names = vec!["localhost".to_string(), "127.0.0.1".to_string()];
    if let Ok(extra) = std::env::var("XMP_TLS_NAMES") {
        names.extend(extra.split(',').filter(|s| !s.is_empty()).map(String::from));
    }
    let (cert, key) = crate::tls::load_or_create(&data_dir, names)?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut crypto = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(vec![cert], key.into())?;
    crypto.alpn_protocols = vec![ALPN.to_vec()];
    let mut cfg = quinn::ServerConfig::with_crypto(Arc::new(quinn::crypto::rustls::QuicServerConfig::try_from(crypto)?));
    let mut tc = quinn::TransportConfig::default();
    tc.keep_alive_interval(Some(Duration::from_secs(5)));
    tc.max_idle_timeout(Some(Duration::from_secs(30).try_into()?));
    cfg.transport_config(Arc::new(tc));
    let ep = quinn::Endpoint::server(cfg, addr)?;
    tracing::info!("QUIC control plane on {}", ep.local_addr()?);
    while let Some(incoming) = ep.accept().await {
        let st = state.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_conn(st, incoming).await {
                tracing::debug!("connection ended: {e}");
            }
        });
    }
    Ok(())
}

struct Session {
    user: Option<i64>,
    conn_id: u64,
}

async fn handle_conn(state: Arc<AppState>, incoming: quinn::Incoming) -> anyhow::Result<()> {
    let conn = incoming.await?;
    let (mut send, mut recv) = conn.accept_bi().await?;
    let (tx, mut rx) = mpsc::unbounded_channel::<Frame>();
    let writer = tokio::spawn(async move {
        while let Some(f) = rx.recv().await {
            if send.write_all(&f.encode()).await.is_err() {
                break;
            }
        }
    });
    let mut sess = Session { user: None, conn_id: CONN_IDS.fetch_add(1, Ordering::Relaxed) };
    let result: anyhow::Result<()> = async {
        loop {
            let mut len = [0u8; 4];
            recv.read_exact(&mut len).await?;
            let n = u32::from_be_bytes(len) as usize;
            if !(5..=MAX_FRAME + 64).contains(&n) {
                anyhow::bail!("bad frame length {n}");
            }
            let mut body = vec![0u8; n];
            recv.read_exact(&mut body).await?;
            let frame = Frame::decode(&body)?;
            dispatch(&state, &mut sess, &tx, frame).await;
        }
    }
    .await;
    if let Some(user) = sess.user {
        on_disconnect(&state, user, sess.conn_id).await;
    }
    writer.abort();
    result
}

fn reply(tx: &UnboundedSender<Frame>, req_id: u32, r: ApiResult<Msg>) {
    let msg = match r {
        Ok(m) => m,
        Err(ApiErr(code, message)) => Msg::Err { code, message },
    };
    let _ = tx.send(Frame::new(req_id, msg));
}

async fn dispatch(state: &Arc<AppState>, sess: &mut Session, tx: &UnboundedSender<Frame>, frame: Frame) {
    let Frame { req_id, msg } = frame;
    match msg {
        Msg::Ping => return reply(tx, req_id, Ok(Msg::Pong)),
        Msg::Register { .. } | Msg::Login { .. } | Msg::Resume { .. } => {
            if sess.user.is_some() {
                return reply(tx, req_id, Err(ApiErr::new(err::CONFLICT, "already authenticated")));
            }
            match authenticate(state, msg).await {
                Ok((uid, token)) => {
                    sess.user = Some(uid);
                    reply(tx, req_id, Ok(Msg::AuthOk { user_id: uid, token }));
                    on_connect(state, uid, sess.conn_id, tx.clone()).await;
                }
                Err(e) => reply(tx, req_id, Err(e)),
            }
            return;
        }
        _ => {}
    }
    let Some(me) = sess.user else {
        return reply(tx, req_id, Err(ApiErr::new(err::UNAUTHORIZED, "authenticate first")));
    };
    match msg {
        // Bulk file traffic runs concurrently so it cannot head-of-line block chat requests.
        m @ (Msg::FileInit { .. } | Msg::FileChunk { .. } | Msg::FileFinish { .. } | Msg::FileGet { .. }) => {
            let (st, tx) = (state.clone(), tx.clone());
            tokio::spawn(async move {
                let st2 = st.clone();
                let r = tokio::task::spawn_blocking(move || handle_file(&st2.files, m))
                    .await
                    .unwrap_or_else(|_| Err(ApiErr::new(err::INTERNAL, "file task failed")));
                reply(&tx, req_id, r);
            });
        }
        m => {
            let r = handle(state, me, m).await;
            reply(tx, req_id, r);
        }
    }
}

async fn authenticate(state: &AppState, msg: Msg) -> ApiResult<(i64, String)> {
    match msg {
        Msg::Register { username, password, display_name } => {
            db::register(&state.pool, &username, &password, &display_name).await?;
            db::login(&state.pool, &username, &password, "first device").await
        }
        Msg::Login { username, password, device_name } => db::login(&state.pool, &username, &password, &device_name).await,
        Msg::Resume { token } => {
            let uid = db::resume(&state.pool, &token).await?;
            Ok((uid, token))
        }
        _ => unreachable!(),
    }
}

async fn on_connect(state: &Arc<AppState>, user: i64, conn_id: u64, tx: UnboundedSender<Frame>) {
    let first = state.hub.register(user, conn_id, tx);
    if let Ok(friends) = db::friend_ids(&state.pool, user).await {
        for f in friends {
            if state.hub.is_online(f) {
                state.hub.push(user, &Msg::Presence { user_id: f, online: true });
                if first {
                    state.hub.push(f, &Msg::Presence { user_id: user, online: true });
                }
            }
        }
    }
}

async fn on_disconnect(state: &Arc<AppState>, user: i64, conn_id: u64) {
    if !state.hub.unregister(user, conn_id) {
        return;
    }
    if let Ok(friends) = db::friend_ids(&state.pool, user).await {
        for f in friends {
            state.hub.push(f, &Msg::Presence { user_id: user, online: false });
        }
    }
    let c = state.media.user_cleanup(user);
    for (call_id, peer) in c.calls {
        state.hub.push(peer, &Msg::CallSignal { call_id, peer: user, kind: sig::HANGUP, data: vec![] });
        db::call_ended(&state.pool, call_id).await;
    }
    for (room_id, pid, others, empty) in c.rooms {
        for u in others {
            state.hub.push(u, &Msg::RoomEvent { room_id, joined: false, pid, user_id: user });
        }
        if empty {
            db::call_ended(&state.pool, room_id).await;
        }
    }
}

fn file_err(e: FileError) -> ApiErr {
    match e {
        FileError::BadRequest(m) => ApiErr::new(err::BAD_REQUEST, m),
        FileError::NotFound => ApiErr::new(err::NOT_FOUND, "file or chunk not found"),
        FileError::Io(e) => {
            tracing::error!("file io: {e}");
            ApiErr::new(err::INTERNAL, "storage error")
        }
    }
}

fn handle_file(files: &BlobStore, msg: Msg) -> ApiResult<Msg> {
    match msg {
        Msg::FileInit { hash, size, .. } => {
            let missing = files.init(&hash, size).map_err(file_err)?;
            Ok(Msg::FileInitResp { missing, chunk_size: FILE_CHUNK_SIZE })
        }
        Msg::FileChunk { hash, index, data } => {
            files.put_chunk(&hash, index, &data).map_err(file_err)?;
            Ok(Msg::Ok)
        }
        Msg::FileFinish { hash } => {
            files.finish(&hash).map_err(file_err)?;
            Ok(Msg::Ok)
        }
        Msg::FileGet { hash, index } => Ok(Msg::FileChunkResp { data: files.get_chunk(&hash, index).map_err(file_err)? }),
        _ => Err(ApiErr::new(err::BAD_REQUEST, "unexpected message")),
    }
}

async fn handle(state: &Arc<AppState>, me: i64, msg: Msg) -> ApiResult<Msg> {
    let pool = &state.pool;
    match msg {
        Msg::FriendRequest { username } => {
            db::friend_request(pool, me, &username).await?;
            Ok(Msg::Ok)
        }
        Msg::FriendAccept { user_id } => {
            db::friend_accept(pool, me, user_id).await?;
            Ok(Msg::Ok)
        }
        Msg::FriendList => {
            let friends = db::friend_list(pool, me).await?;
            Ok(Msg::FriendListResp { friends })
        }
        Msg::SendMessage { to_user, kind, body, client_id } => {
            if kind > msg_kind::VOICE || body.len() > MAX_TEXT_BODY {
                return Err(ApiErr::new(err::BAD_REQUEST, "invalid message"));
            }
            let m = db::send_message(pool, me, to_user, kind, &body, client_id).await?;
            let ack = Msg::MessageAck { conv_id: m.conv_id, seq: m.seq, ts: m.ts };
            state.hub.push(to_user, &Msg::PushMessage(m));
            Ok(ack)
        }
        Msg::Sync { conv_id, after_seq, limit } => Ok(Msg::SyncResp { messages: db::sync(pool, me, conv_id, after_seq, limit).await? }),
        Msg::ConvList => Ok(Msg::ConvListResp { convs: db::conv_list(pool, me).await? }),

        Msg::CallSignal { call_id, peer, kind, data } => {
            if data.len() > 16 * 1024 {
                return Err(ApiErr::new(err::BAD_REQUEST, "signal too large"));
            }
            if !db::are_friends(pool, me, peer).await? {
                return Err(ApiErr::new(err::FORBIDDEN, "not friends"));
            }
            if kind == sig::INVITE {
                if !state.hub.is_online(peer) {
                    return Err(ApiErr::new(err::NOT_FOUND, "peer offline"));
                }
                if !state.media.call_create(call_id, me, peer) {
                    return Err(ApiErr::new(err::CONFLICT, "call id in use"));
                }
                db::call_started(pool, call_id, "p2p", me).await;
            } else if state.media.call_peer(call_id, me) != Some(peer) {
                return Err(ApiErr::new(err::FORBIDDEN, "not part of this call"));
            }
            state.hub.push(peer, &Msg::CallSignal { call_id, peer: me, kind, data });
            if kind == sig::HANGUP || kind == sig::REJECT {
                state.media.call_end(call_id);
                db::call_ended(pool, call_id).await;
            }
            Ok(Msg::Ok)
        }
        Msg::MediaAlloc { call_id } => {
            let token = state.media.call_alloc(call_id, me).ok_or_else(|| ApiErr::new(err::FORBIDDEN, "not part of this call"))?;
            Ok(Msg::MediaAllocResp { token, udp_addr: state.udp_advertise.clone() })
        }

        Msg::RoomCreate => {
            let room_id = state.media.room_create(me);
            db::call_started(pool, room_id, "room", me).await;
            Ok(Msg::RoomCreated { room_id })
        }
        Msg::RoomJoin { room_id } => {
            let owner = state.media.room_owner(room_id).ok_or_else(|| ApiErr::new(err::NOT_FOUND, "no such room"))?;
            if owner != me && !db::are_friends(pool, me, owner).await? {
                return Err(ApiErr::new(err::FORBIDDEN, "room is friends-only"));
            }
            let j = state.media.room_join(room_id, me).map_err(|m| ApiErr::new(err::CONFLICT, m))?;
            for u in &j.others {
                state.hub.push(*u, &Msg::RoomEvent { room_id, joined: true, pid: j.pid, user_id: me });
            }
            let participants: Vec<Participant> = j.participants;
            Ok(Msg::RoomJoined { pid: j.pid, token: j.token, udp_addr: state.udp_advertise.clone(), media_key: j.key, participants })
        }
        Msg::RoomLeave { room_id } => {
            if let Some((pid, others, empty)) = state.media.room_leave(room_id, me) {
                for u in others {
                    state.hub.push(u, &Msg::RoomEvent { room_id, joined: false, pid, user_id: me });
                }
                if empty {
                    db::call_ended(pool, room_id).await;
                }
            }
            Ok(Msg::Ok)
        }
        _ => Err(ApiErr::new(err::BAD_REQUEST, "unexpected message")),
    }
}
