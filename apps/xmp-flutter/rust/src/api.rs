use std::sync::{Arc, Mutex as StdMutex, atomic::{AtomicBool, AtomicU32}};
use tokio::sync::Mutex;
use xmp_client::{Client, ConnectOpts, Auth, session::MediaSession, call::Call};
use xmp_codec::{audio::{AudioConfig, AudioEncoder}, video::H264Encoder};
use xmp_proto::control::{StoredMessage, msg_kind, FileRef, sig, Msg};
use serde::{Serialize, Deserialize};
use anyhow::{Result, anyhow};

// ---- State Management ----

lazy_static::lazy_static! {
    static ref STATE: AppState = AppState::default();
    static ref RUNTIME: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
}

#[derive(Default)]
pub(crate) struct AppState {
    client: Mutex<Option<Client>>,
    call: StdMutex<Option<Arc<ActiveCall>>>,
    sink: StdMutex<Option<StreamSink<XmpEvent>>>,
}

struct ActiveCall {
    call: Mutex<Option<Call>>,
    media: MediaSession,
    audio_enc: StdMutex<AudioEncoder>,
    video_enc: StdMutex<Option<H264Encoder>>,
    target_bps: Arc<AtomicU32>,
    cur_bps: AtomicU32,
    force_key: Arc<AtomicBool>,
    t0: std::time::Instant,
}

// ---- DTOs ----

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FriendDto {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub status: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MsgDto {
    pub conv: i64,
    pub seq: i64,
    pub sender: i64,
    pub kind: u8,
    pub ts: i64,
    pub text: Option<String>,
    pub file: Option<FileDto>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileDto {
    pub name: String,
    pub size: u64,
    pub hash: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConvDto {
    pub conv: i64,
    pub peer: i64,
    pub last_seq: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MeDto {
    pub user_id: i64,
    pub token: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CallInfoDto {
    pub call_id: String,
    pub peer: i64,
    pub video: bool,
    pub sas: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CallStatsDto {
    pub path: String,
    pub video_target_kbps: u32,
    pub tx: u64,
    pub rx: u64,
    pub retransmits: u64,
    pub fec_recovered: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum XmpEvent {
    Message { message: MsgDto },
    Presence { user: i64, online: bool },
    IncomingCall { call_id: String, peer: i64, video: bool },
    CallEnded { call_id: String, peer: i64 },
    Disconnected,
}

// ---- API Functions ----

pub fn init_app() {
    // Initialization logic if needed
}

use crate::frb_generated::StreamSink;

pub fn create_event_stream(sink: StreamSink<XmpEvent>) {
    *STATE.sink.lock().unwrap() = Some(sink);
}

pub async fn connect(
    server: String,
    server_name: String,
    cert_b64: Option<String>,
    mode: String,
    username: Option<String>,
    password: Option<String>,
    token: Option<String>,
) -> Result<MeDto> {
    let pinned_cert = if let Some(b64) = cert_b64.as_deref().filter(|p| !p.trim().is_empty()) {
        Some(normalize_cert(b64)?)
    } else {
        None
    };

    let u = username.clone().unwrap_or_default();
    let p = password.clone().unwrap_or_default();
    let auth = match mode.as_str() {
        "register" => Auth::Register { username: u.clone(), password: p, display_name: u },
        "resume" => Auth::Resume { token: token.unwrap_or_default() },
        _ => Auth::Login { username: u, password: p, device_name: "XMP Flutter".to_string() },
    };

    let (c, mut events) = Client::connect(ConnectOpts { server, server_name, pinned_cert }, auth)
        .await
        .map_err(|e| anyhow!(friendly(e)))?;

    let me = MeDto { user_id: c.user_id, token: c.token.clone() };
    *STATE.client.lock().await = Some(c);

    // Spawn event listener
    tokio::spawn(async move {
        while let Some(m) = events.recv().await {
            if let Some(ev) = event_to_dto(&m) {
                if let Some(sink) = &*STATE.sink.lock().unwrap() {
                    let _ = sink.add(ev);
                }
            }
        }
        if let Some(sink) = &*STATE.sink.lock().unwrap() {
            let _ = sink.add(XmpEvent::Disconnected);
        }
    });

    Ok(me)
}

pub async fn logout() -> Result<()> {
    hangup_inner().await;
    *STATE.client.lock().await = None;
    Ok(())
}

pub async fn get_friends() -> Result<Vec<FriendDto>> {
    let client = get_client().await?;
    let fs = client.friends().await.map_err(|e| anyhow!(friendly(e)))?;
    Ok(fs.into_iter().map(|f| FriendDto {
        id: f.user_id,
        username: f.username,
        display_name: f.display_name,
        status: ["sent", "received", "friends"][(f.status as usize).min(2)].to_string(),
    }).collect())
}

pub async fn friend_request(username: String) -> Result<()> {
    get_client().await?.friend_request(&username).await.map_err(|e| anyhow!(friendly(e)))
}

pub async fn friend_accept(user_id: i64) -> Result<()> {
    get_client().await?.friend_accept(user_id).await.map_err(|e| anyhow!(friendly(e)))
}

pub async fn get_conversations() -> Result<Vec<ConvDto>> {
    let convs = get_client().await?.conversations().await.map_err(|e| anyhow!(friendly(e)))?;
    Ok(convs.into_iter().map(|c| ConvDto { conv: c.conv_id, peer: c.peer_id, last_seq: c.last_seq }).collect())
}

pub async fn sync_messages(conv: i64, after: i64) -> Result<Vec<MsgDto>> {
    let messages = get_client().await?.sync(conv, after, 200).await.map_err(|e| anyhow!(friendly(e)))?;
    Ok(messages.into_iter().map(|m| msg_dto(&m)).collect())
}

pub async fn send_text(to: i64, text: String) -> Result<ConvDto> {
    let client = get_client().await?;
    let (conv, seq, _ts) = client.send_text(to, &text).await.map_err(|e| anyhow!(friendly(e)))?;
    Ok(ConvDto { conv, peer: to, last_seq: seq })
}

pub async fn send_file(to: i64, path: String) -> Result<FileDto> {
    let file = get_client().await?.send_file(to, std::path::Path::new(&path)).await.map_err(|e| anyhow!(friendly(e)))?;
    Ok(FileDto { name: file.name, size: file.size, hash: hex(&file.hash) })
}

pub async fn download_file(hash: String, size: u64, name: String, dest: String) -> Result<()> {
    let bytes = hex::decode(&hash).map_err(|_| anyhow!("文件哈希无效"))?;
    let hash: [u8; 32] = bytes.try_into().map_err(|_| anyhow!("文件哈希无效"))?;
    let file = FileRef { name, size, hash };
    get_client().await?.download_file(&file, std::path::Path::new(&dest)).await.map_err(|e| anyhow!(friendly(e)))
}

pub async fn call_start(peer: i64, video: bool) -> Result<CallInfoDto> {
    let call = get_client().await?.start_call(peer, video).await.map_err(|e| anyhow!(friendly(e)))?;
    let info = CallInfoDto { call_id: call.call_id.to_string(), peer: call.peer, video: call.video, sas: call.sas.clone() };
    let media = call.media.clone();
    let active = ActiveCall {
        call: Mutex::new(Some(call)),
        media,
        audio_enc: StdMutex::new(AudioEncoder::new(AudioConfig::default()).map_err(|e| anyhow!(e.to_string()))?),
        video_enc: StdMutex::new(None),
        target_bps: Arc::new(AtomicU32::new(0)),
        cur_bps: AtomicU32::new(0),
        force_key: Arc::new(AtomicBool::new(false)),
        t0: std::time::Instant::now(),
    };
    *STATE.call.lock().unwrap() = Some(Arc::new(active));
    Ok(info)
}

pub async fn call_accept(call_id: String, peer: i64, video: bool) -> Result<CallInfoDto> {
    let id = call_id.parse::<u64>().map_err(|_| anyhow!("通话 ID 无效"))?;
    let call = get_client().await?.accept_call(id, peer, video).await.map_err(|e| anyhow!(friendly(e)))?;
    let info = CallInfoDto { call_id, peer: call.peer, video: call.video, sas: call.sas.clone() };
    let media = call.media.clone();
    *STATE.call.lock().unwrap() = Some(Arc::new(ActiveCall {
        call: Mutex::new(Some(call)),
        media,
        audio_enc: StdMutex::new(AudioEncoder::new(AudioConfig::default()).map_err(|e| anyhow!(e.to_string()))?),
        video_enc: StdMutex::new(None),
        target_bps: Arc::new(AtomicU32::new(0)),
        cur_bps: AtomicU32::new(0),
        force_key: Arc::new(AtomicBool::new(false)),
        t0: std::time::Instant::now(),
    }));
    Ok(info)
}

pub async fn call_reject(call_id: String, peer: i64) -> Result<()> {
    let id = call_id.parse::<u64>().map_err(|_| anyhow!("通话 ID 无效"))?;
    get_client().await?.reject_call(id, peer).await.map_err(|e| anyhow!(friendly(e)))
}

pub async fn call_hangup() -> Result<()> {
    hangup_inner().await;
    Ok(())
}

pub async fn call_stats() -> Result<CallStatsDto> {
    let active = STATE.call.lock().unwrap().clone().ok_or_else(|| anyhow!("当前没有通话"))?;
    let stats = active.media.stats();
    Ok(CallStatsDto {
        path: format!("{:?}", active.media.path()).to_lowercase(),
        video_target_kbps: stats.video_target_bps / 1000,
        tx: stats.tx_packets,
        rx: stats.rx_packets,
        retransmits: stats.retransmits,
        fec_recovered: stats.fec_recovered,
    })
}

// ... Additional functions for calls, files etc would go here ...

// ---- Helpers ----

async fn get_client() -> Result<Client> {
    STATE.client.lock().await.clone().ok_or_else(|| anyhow!("尚未登录"))
}

async fn hangup_inner() {
    let ac = STATE.call.lock().unwrap().take();
    if let Some(ac) = ac {
        if let Some(c) = ac.call.lock().await.take() {
            c.hangup().await;
        }
    }
}

fn friendly(e: xmp_client::Error) -> String {
    match e {
        xmp_client::Error::Api(409, _) => "用户名已被占用".into(),
        xmp_client::Error::Api(401, _) => "用户名或密码错误".into(),
        xmp_client::Error::Api(404, m) if m.contains("offline") => "对方当前不在线".into(),
        xmp_client::Error::Api(404, _) => "用户不存在".into(),
        xmp_client::Error::Api(403, _) => "没有权限（需要先成为好友）".into(),
        xmp_client::Error::Timeout => "请求超时，请检查网络或对方是否接听".into(),
        xmp_client::Error::Closed => "与服务器的连接已断开".into(),
        xmp_client::Error::Api(_, m) | xmp_client::Error::Other(m) => m,
    }
}

fn normalize_cert(b64: &str) -> Result<Vec<u8>> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let bytes = STANDARD.decode(b64.trim()).map_err(|_| anyhow!("证书文件内容无效"))?;
    // Simplification for brevity, full logic from Tauri can be copied here
    Ok(bytes)
}

fn event_to_dto(m: &Msg) -> Option<XmpEvent> {
    match m {
        Msg::PushMessage(s) => Some(XmpEvent::Message { message: msg_dto(s) }),
        Msg::Presence { user_id, online } => Some(XmpEvent::Presence { user: *user_id, online: *online }),
        Msg::CallSignal { call_id, peer, kind, data } if *kind == sig::INVITE => {
            Some(XmpEvent::IncomingCall { call_id: call_id.to_string(), peer: *peer, video: data.first() == Some(&1) })
        }
        Msg::CallSignal { call_id, peer, kind, .. } if *kind == sig::HANGUP || *kind == sig::REJECT => {
            Some(XmpEvent::CallEnded { call_id: call_id.to_string(), peer: *peer })
        }
        _ => None,
    }
}

fn msg_dto(m: &StoredMessage) -> MsgDto {
    let (mut text, mut file) = (None, None);
    if m.kind == msg_kind::FILE {
        file = FileRef::decode(&m.body).ok().map(|f| FileDto { name: f.name, size: f.size, hash: hex(&f.hash) });
    } else {
        text = Some(String::from_utf8_lossy(&m.body).into_owned());
    }
    MsgDto { conv: m.conv_id, seq: m.seq, sender: m.sender, kind: m.kind, ts: m.ts, text, file }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
