//! XMP desktop/mobile app backend. The web UI talks to this through Tauri commands; media crosses the
//! boundary as raw bytes (binary IPC / channels), never JSON.

mod convert;

use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc, Mutex as StdMutex,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{
    ipc::{Channel, InvokeBody, InvokeResponseBody, Request},
    AppHandle, Emitter, State,
};
use tokio::sync::Mutex;
use xmp_client::{
    call::Call,
    session::{MediaEvent, MediaSession, Path},
    Auth, Client, ConnectOpts,
};
use xmp_codec::{
    audio::{AudioConfig, AudioDecoder, AudioEncoder},
    video::{H264Decoder, H264Encoder, VideoDecoder, VideoEncoder},
};
use xmp_proto::control::{msg_kind, sig, FileRef, Msg, StoredMessage};

type Res<T> = Result<T, String>;

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

#[derive(Default)]
struct AppState {
    client: Mutex<Option<Client>>,
    call: StdMutex<Option<Arc<ActiveCall>>>,
}

async fn client(state: &State<'_, AppState>) -> Res<Client> {
    state.client.lock().await.clone().ok_or_else(|| "尚未登录".to_string())
}

// ---- DTOs ----------------------------------------------------------------------------------

#[derive(Serialize)]
struct FileDto {
    name: String,
    size: u64,
    hash: String,
}

#[derive(Serialize)]
struct MsgDto {
    conv: i64,
    seq: i64,
    sender: i64,
    kind: u8,
    ts: i64,
    text: Option<String>,
    file: Option<FileDto>,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
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

fn event_json(m: &Msg) -> serde_json::Value {
    match m {
        Msg::PushMessage(s) => json!({ "type": "message", "message": msg_dto(s) }),
        Msg::Presence { user_id, online } => json!({ "type": "presence", "user": user_id, "online": online }),
        Msg::CallSignal { call_id, peer, kind, data } if *kind == sig::INVITE => {
            json!({ "type": "incoming_call", "call_id": call_id.to_string(), "peer": peer, "video": data.first() == Some(&1) })
        }
        Msg::CallSignal { call_id, peer, kind, .. } if *kind == sig::HANGUP || *kind == sig::REJECT => {
            json!({ "type": "call_ended", "call_id": call_id.to_string(), "peer": peer })
        }
        _ => json!({ "type": "other" }),
    }
}

// ---- certificates ---------------------------------------------------------------------------

/// Accepts base64 of a DER certificate or of a PEM file and returns DER.
fn normalize_cert(b64: &str) -> Res<Vec<u8>> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let bytes = STANDARD.decode(b64.trim()).map_err(|_| "证书文件内容无效".to_string())?;
    if bytes.starts_with(b"-----BEGIN") {
        let text = String::from_utf8(bytes).map_err(|_| "证书文件内容无效".to_string())?;
        let body: String = text
            .lines()
            .skip_while(|l| !l.starts_with("-----BEGIN CERTIFICATE-----"))
            .skip(1)
            .take_while(|l| !l.starts_with("-----END"))
            .map(str::trim)
            .collect();
        if body.is_empty() {
            return Err("PEM 文件中没有找到证书".into());
        }
        return STANDARD.decode(body).map_err(|_| "PEM 证书内容无效".to_string());
    }
    // a DER certificate is an ASN.1 SEQUENCE
    if bytes.first() != Some(&0x30) {
        return Err("不是有效的证书文件（需要 DER 或 PEM 格式）".into());
    }
    Ok(bytes)
}

// ---- secrets (OS keychain) --------------------------------------------------------------------

const KEYCHAIN_SERVICE: &str = "app.xmp.client";

#[cfg(desktop)]
fn entry(key: &str) -> Res<keyring::Entry> {
    keyring::Entry::new(KEYCHAIN_SERVICE, key).map_err(|e| e.to_string())
}

/// Store a secret in the OS keychain. Returns Err("unsupported") on mobile so the UI can fall back
/// to the app-private sandbox; any other error means the keychain exists but failed.
#[tauri::command]
async fn secret_set(key: String, value: String) -> Res<()> {
    #[cfg(desktop)]
    {
        entry(&key)?.set_password(&value).map_err(|e| e.to_string())
    }
    #[cfg(mobile)]
    {
        let _ = (key, value);
        Err("unsupported".into())
    }
}

#[tauri::command]
async fn secret_get(key: String) -> Res<Option<String>> {
    #[cfg(desktop)]
    {
        match entry(&key)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    #[cfg(mobile)]
    {
        let _ = key;
        Err("unsupported".into())
    }
}

#[tauri::command]
async fn secret_delete(key: String) -> Res<()> {
    #[cfg(desktop)]
    {
        match entry(&key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
    #[cfg(mobile)]
    {
        let _ = key;
        Err("unsupported".into())
    }
}

// ---- session -------------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConnectArgs {
    server: String,
    server_name: String,
    /// Pinned server certificate, base64 of the file content (DER or PEM). Read by the UI via the
    /// file picker so it also works with Android content URIs.
    cert_b64: Option<String>,
    mode: String,
    username: Option<String>,
    password: Option<String>,
    token: Option<String>,
}

#[derive(Serialize)]
struct Me {
    user_id: i64,
    token: String,
}

#[tauri::command]
async fn connect(app: AppHandle, state: State<'_, AppState>, args: ConnectArgs) -> Res<Me> {
    let pinned_cert = match args.cert_b64.as_deref().filter(|p| !p.trim().is_empty()) {
        Some(b64) => Some(normalize_cert(b64)?),
        None => None,
    };
    let (u, p) = (args.username.clone().unwrap_or_default(), args.password.clone().unwrap_or_default());
    let auth = match args.mode.as_str() {
        "register" => Auth::Register { username: u.clone(), password: p, display_name: u },
        "resume" => Auth::Resume { token: args.token.clone().unwrap_or_default() },
        _ => Auth::Login { username: u, password: p, device_name: format!("XMP App ({})", std::env::consts::OS) },
    };
    let (c, mut events) = Client::connect(ConnectOpts { server: args.server, server_name: args.server_name, pinned_cert }, auth)
        .await
        .map_err(friendly)?;
    let me = Me { user_id: c.user_id, token: c.token.clone() };
    *state.client.lock().await = Some(c);
    tauri::async_runtime::spawn(async move {
        while let Some(m) = events.recv().await {
            let _ = app.emit("xmp-event", event_json(&m));
        }
        let _ = app.emit("xmp-event", json!({ "type": "disconnected" }));
    });
    Ok(me)
}

#[tauri::command]
async fn logout(state: State<'_, AppState>) -> Res<()> {
    hangup_inner(&state).await;
    *state.client.lock().await = None;
    Ok(())
}

// ---- friends & chat ------------------------------------------------------------------------

#[derive(Serialize)]
struct FriendDto {
    id: i64,
    username: String,
    display_name: String,
    status: &'static str,
}

#[tauri::command]
async fn friends(state: State<'_, AppState>) -> Res<Vec<FriendDto>> {
    let fs = client(&state).await?.friends().await.map_err(friendly)?;
    Ok(fs
        .into_iter()
        .map(|f| FriendDto { id: f.user_id, username: f.username, display_name: f.display_name, status: ["sent", "received", "friends"][(f.status as usize).min(2)] })
        .collect())
}

#[tauri::command]
async fn friend_request(state: State<'_, AppState>, username: String) -> Res<()> {
    client(&state).await?.friend_request(&username).await.map_err(friendly)
}

#[tauri::command]
async fn friend_accept(state: State<'_, AppState>, user_id: i64) -> Res<()> {
    client(&state).await?.friend_accept(user_id).await.map_err(friendly)
}

#[derive(Serialize)]
struct ConvDto {
    conv: i64,
    peer: i64,
    last_seq: i64,
}

#[tauri::command]
async fn conversations(state: State<'_, AppState>) -> Res<Vec<ConvDto>> {
    let cs = client(&state).await?.conversations().await.map_err(friendly)?;
    Ok(cs.into_iter().map(|c| ConvDto { conv: c.conv_id, peer: c.peer_id, last_seq: c.last_seq }).collect())
}

#[tauri::command]
async fn sync(state: State<'_, AppState>, conv: i64, after: i64, limit: u32) -> Res<Vec<MsgDto>> {
    let ms = client(&state).await?.sync(conv, after, limit).await.map_err(friendly)?;
    Ok(ms.iter().map(msg_dto).collect())
}

#[derive(Serialize)]
struct Ack {
    conv: i64,
    seq: i64,
    ts: i64,
}

#[tauri::command]
async fn send_text(state: State<'_, AppState>, to: i64, text: String) -> Res<Ack> {
    let (conv, seq, ts) = client(&state).await?.send_text(to, &text).await.map_err(friendly)?;
    Ok(Ack { conv, seq, ts })
}

#[tauri::command]
async fn send_file(state: State<'_, AppState>, to: i64, path: String) -> Res<()> {
    client(&state).await?.send_file(to, std::path::Path::new(&path)).await.map(|_| ()).map_err(friendly)
}

#[tauri::command]
async fn download_file(state: State<'_, AppState>, hash: String, size: u64, dest: String) -> Res<()> {
    if hash.len() != 64 {
        return Err("无效的文件哈希".into());
    }
    let mut h = [0u8; 32];
    for i in 0..32 {
        h[i] = u8::from_str_radix(&hash[i * 2..i * 2 + 2], 16).map_err(|_| "无效的文件哈希".to_string())?;
    }
    client(&state).await?.download_file(&FileRef { hash: h, size, name: String::new() }, std::path::Path::new(&dest)).await.map_err(friendly)
}

// ---- calls ---------------------------------------------------------------------------------

#[derive(Serialize)]
struct CallInfo {
    sas: String,
}

fn raw(b: Vec<u8>) -> InvokeResponseBody {
    InvokeResponseBody::Raw(b)
}

fn wrap_call(app: AppHandle, mut call: Call, audio: Channel<InvokeResponseBody>, video: Channel<InvokeResponseBody>) -> (Arc<ActiveCall>, String) {
    let (_t1, dummy_ev) = tokio::sync::mpsc::unbounded_channel();
    let (_t2, dummy_sig) = tokio::sync::mpsc::unbounded_channel();
    let mut events = std::mem::replace(&mut call.events, dummy_ev);
    let mut signals = std::mem::replace(&mut call.signals, dummy_sig);
    let (call_id, peer) = (call.call_id, call.peer);
    let sas = call.sas.clone();
    let target = Arc::new(AtomicU32::new(call.media.video_target_bps()));
    let force_key = Arc::new(AtomicBool::new(false));
    let (tg, fk) = (target.clone(), force_key.clone());

    tauri::async_runtime::spawn(async move {
        let mut adec = AudioDecoder::new(AudioConfig::default()).ok();
        let mut vdec = H264Decoder::new().ok();
        while let Some(ev) = events.recv().await {
            match ev {
                MediaEvent::Audio { data, .. } => {
                    if let Some(Ok(pcm)) = adec.as_mut().map(|d| d.decode(data.as_deref())) {
                        let mut bytes = Vec::with_capacity(pcm.len() * 2);
                        for s in pcm {
                            bytes.extend_from_slice(&s.to_le_bytes());
                        }
                        let _ = audio.send(raw(bytes));
                    }
                }
                MediaEvent::Video { data, .. } => {
                    if let Some(Ok(Some(f))) = vdec.as_mut().map(|d| d.decode(&data)) {
                        let _ = video.send(raw(convert::i420_to_rgba_frame(&f)));
                    }
                }
                MediaEvent::Bitrate { video_bps } => tg.store(video_bps, Ordering::Relaxed),
                MediaEvent::KeyframeRequest => fk.store(true, Ordering::Relaxed),
                _ => {}
            }
        }
    });
    // the peer hanging up arrives on the per-call signalling channel
    tauri::async_runtime::spawn(async move {
        while let Some((kind, _)) = signals.recv().await {
            if kind == sig::HANGUP || kind == sig::REJECT {
                let _ = app.emit("xmp-event", json!({ "type": "call_ended", "call_id": call_id.to_string(), "peer": peer }));
                break;
            }
        }
    });

    let media = call.media.clone();
    let ac = Arc::new(ActiveCall {
        call: Mutex::new(Some(call)),
        media,
        audio_enc: StdMutex::new(AudioEncoder::new(AudioConfig::default()).expect("opus encoder")),
        video_enc: StdMutex::new(None),
        target_bps: target,
        cur_bps: AtomicU32::new(0),
        force_key,
        t0: std::time::Instant::now(),
    });
    (ac, sas)
}

#[tauri::command]
async fn call_start(app: AppHandle, state: State<'_, AppState>, peer: i64, video: bool, audio: Channel<InvokeResponseBody>, video_ch: Channel<InvokeResponseBody>) -> Res<CallInfo> {
    let c = client(&state).await?;
    let call = c.start_call(peer, video).await.map_err(friendly)?;
    let (ac, sas) = wrap_call(app, call, audio, video_ch);
    *state.call.lock().unwrap() = Some(ac);
    Ok(CallInfo { sas })
}

#[tauri::command]
async fn call_accept(app: AppHandle, state: State<'_, AppState>, call_id: String, peer: i64, video: bool, audio: Channel<InvokeResponseBody>, video_ch: Channel<InvokeResponseBody>) -> Res<CallInfo> {
    let id: u64 = call_id.parse().map_err(|_| "无效的通话 ID".to_string())?;
    let c = client(&state).await?;
    let call = c.accept_call(id, peer, video).await.map_err(friendly)?;
    let (ac, sas) = wrap_call(app, call, audio, video_ch);
    *state.call.lock().unwrap() = Some(ac);
    Ok(CallInfo { sas })
}

#[tauri::command]
async fn call_reject(state: State<'_, AppState>, call_id: String, peer: i64) -> Res<()> {
    let id: u64 = call_id.parse().map_err(|_| "无效的通话 ID".to_string())?;
    client(&state).await?.reject_call(id, peer).await.map_err(friendly)
}

async fn hangup_inner(state: &State<'_, AppState>) {
    let ac = state.call.lock().unwrap().take();
    if let Some(ac) = ac {
        if let Some(c) = ac.call.lock().await.take() {
            c.hangup().await;
        }
    }
}

#[tauri::command]
async fn call_hangup(state: State<'_, AppState>) -> Res<()> {
    hangup_inner(&state).await;
    Ok(())
}

#[derive(Serialize)]
struct StatsDto {
    path: &'static str,
    video_target_kbps: u32,
    tx: u64,
    rx: u64,
    retransmits: u64,
    fec_recovered: u64,
}

#[tauri::command]
async fn call_stats(state: State<'_, AppState>) -> Res<StatsDto> {
    let ac = state.call.lock().unwrap().clone().ok_or("没有进行中的通话")?;
    let s = ac.media.stats();
    Ok(StatsDto {
        path: match ac.media.path() {
            Path::P2p(_) => "p2p",
            Path::Relay(_) => "relay",
            Path::None => "none",
        },
        video_target_kbps: ac.media.video_target_bps() / 1000,
        tx: s.tx_packets,
        rx: s.rx_packets,
        retransmits: s.retransmits,
        fec_recovered: s.fec_recovered,
    })
}

fn body(request: &Request<'_>) -> Res<Vec<u8>> {
    match request.body() {
        InvokeBody::Raw(b) => Ok(b.clone()),
        _ => Err("expected a binary payload".into()),
    }
}

/// One 20 ms stereo frame: 1920 interleaved little-endian i16 samples.
#[tauri::command]
async fn push_audio(state: State<'_, AppState>, request: Request<'_>) -> Res<()> {
    let Some(ac) = state.call.lock().unwrap().clone() else { return Ok(()) };
    let bytes = body(&request)?;
    let pcm: Vec<i16> = bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
    let pkt = ac.audio_enc.lock().unwrap().encode(&pcm).map_err(|e| e.to_string())?;
    let ts = ac.t0.elapsed().as_millis() as u32;
    ac.media.send_audio(ts, &pkt).await;
    Ok(())
}

/// RGBA frame (`x-w`, `x-h` headers). Converted to I420, H.264-encoded and paced out.
#[tauri::command]
async fn push_video(state: State<'_, AppState>, request: Request<'_>) -> Res<()> {
    let Some(ac) = state.call.lock().unwrap().clone() else { return Ok(()) };
    let hdr = |n: &str| request.headers().get(n).and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<usize>().ok()).ok_or_else(|| format!("missing header {n}"));
    let (w, h) = (hdr("x-w")?, hdr("x-h")?);
    let rgba = body(&request)?;
    if w == 0 || h == 0 || w % 2 != 0 || h % 2 != 0 || rgba.len() != w * h * 4 {
        return Err("bad frame dimensions".into());
    }
    let ac2 = ac.clone();
    let encoded = tauri::async_runtime::spawn_blocking(move || -> Res<Option<xmp_codec::video::EncodedFrame>> {
        let frame = convert::rgba_to_i420(&rgba, w, h);
        let want = ac2.target_bps.load(Ordering::Relaxed).min(8_000_000);
        let mut guard = ac2.video_enc.lock().unwrap();
        if guard.is_none() {
            *guard = Some(H264Encoder::new(want, 30.0, 300).map_err(|e| e.to_string())?);
            ac2.cur_bps.store(want, Ordering::Relaxed);
        }
        let enc = guard.as_mut().unwrap();
        let cur = ac2.cur_bps.load(Ordering::Relaxed);
        if (want as f64 - cur as f64).abs() > cur as f64 * 0.1 && enc.set_bitrate(want).is_ok() {
            ac2.cur_bps.store(want, Ordering::Relaxed);
        }
        let force = ac2.force_key.swap(false, Ordering::Relaxed);
        enc.encode(&frame, force).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    if let Some(f) = encoded {
        let ts = ac.t0.elapsed().as_millis() as u32;
        ac.media.send_video_frame(0, ts, f.keyframe, &f.data).await;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Debug-only: `XMP_DEMO=incoming` (or any text) reloads the UI on the in-memory demo backend.
            #[cfg(debug_assertions)]
            if let Ok(flag) = std::env::var("XMP_DEMO") {
                use tauri::Manager;
                if let Some(w) = app.get_webview_window("main") {
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(1200));
                        let _ = w.eval(format!("location.hash='#demo,{flag}'; location.reload();"));
                    });
                }
            }
            let _ = app;
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            connect, logout, secret_set, secret_get, secret_delete, friends, friend_request, friend_accept, conversations, sync, send_text, send_file, download_file,
            call_start, call_accept, call_reject, call_hangup, call_stats, push_audio, push_video
        ])
        .run(tauri::generate_context!())
        .expect("error while running XMP");
}

#[cfg(test)]
mod tests {
    use super::normalize_cert;
    use base64::{engine::general_purpose::STANDARD, Engine};

    #[test]
    fn der_and_pem_certs() {
        let der = [0x30u8, 0x82, 0x01, 0x0a, 1, 2, 3, 4];
        assert_eq!(normalize_cert(&STANDARD.encode(der)).unwrap(), der);
        let pem = format!("-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n", STANDARD.encode(der));
        assert_eq!(normalize_cert(&STANDARD.encode(pem)).unwrap(), der);
        assert!(normalize_cert(&STANDARD.encode(b"hello world")).is_err());
        assert!(normalize_cert("%%%not base64").is_err());
        assert!(normalize_cert(&STANDARD.encode("-----BEGIN CERTIFICATE-----\n-----END CERTIFICATE-----")).is_err());
    }
}
