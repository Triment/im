//! C ABI over `xmp-client` + `xmp-codec`, for Swift (Apple), Kotlin/JNI (Android), Dart FFI (Flutter),
//! Tauri/Electron native modules and C/C++ desktop apps. See `xmp.h`.
//!
//! Threading: every function may be called from any thread. Callbacks are invoked on library
//! threads and must return quickly. UI layers supply raw PCM (48 kHz stereo i16, 20 ms frames)
//! and raw I420 frames; Opus/H.264, FEC, congestion control and networking happen inside.

use std::{
    cell::RefCell,
    ffi::{c_char, c_void, CStr, CString},
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex, OnceLock,
    },
};
use tokio::runtime::Runtime;
use xmp_client::{call::Call, session::MediaEvent, Auth, Client, ConnectOpts};
use xmp_codec::{
    audio::{AudioConfig, AudioDecoder, AudioEncoder},
    video::{H264Decoder, H264Encoder, VideoDecoder, VideoEncoder, Yuv420},
};
use xmp_proto::control::{msg_kind, sig, FileRef, Msg};

fn rt() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build().expect("tokio runtime"))
}

thread_local! {
    static LAST_ERROR: RefCell<CString> = RefCell::new(CString::default());
}

fn set_err(e: impl std::fmt::Display) {
    let s = CString::new(e.to_string().replace('\0', " ")).unwrap_or_default();
    LAST_ERROR.with(|l| *l.borrow_mut() = s);
}

/// Message of the last failed call on this thread (valid until the next call on this thread).
#[no_mangle]
pub extern "C" fn xmp_last_error() -> *const c_char {
    LAST_ERROR.with(|l| l.borrow().as_ptr())
}

unsafe fn cstr<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() { None } else { CStr::from_ptr(p).to_str().ok() }
}

fn to_c(s: String) -> *mut c_char {
    CString::new(s.replace('\0', " ")).unwrap_or_default().into_raw()
}

/// Free any string returned by this library.
#[no_mangle]
pub unsafe extern "C" fn xmp_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

// ---- tiny JSON writer ---------------------------------------------------------------------

fn jstr(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn msg_json(m: &xmp_proto::control::StoredMessage) -> String {
    let body = match m.kind {
        msg_kind::FILE => match FileRef::decode(&m.body) {
            Ok(f) => format!("\"file\":{{\"name\":{},\"size\":{},\"hash\":\"{}\"}}", jstr(&f.name), f.size, hex(&f.hash)),
            Err(_) => "\"file\":null".into(),
        },
        _ => format!("\"text\":{}", jstr(&String::from_utf8_lossy(&m.body))),
    };
    format!("{{\"conv\":{},\"seq\":{},\"sender\":{},\"kind\":{},\"ts\":{},{}}}", m.conv_id, m.seq, m.sender, m.kind, m.ts, body)
}

/// Server pushes as JSON. 64-bit ids that may exceed 2^53 (call/room ids) are decimal strings.
fn event_json(m: &Msg) -> String {
    match m {
        Msg::PushMessage(s) => format!("{{\"type\":\"message\",\"message\":{}}}", msg_json(s)),
        Msg::Presence { user_id, online } => format!("{{\"type\":\"presence\",\"user\":{user_id},\"online\":{online}}}"),
        Msg::CallSignal { call_id, peer, kind, data } if *kind == sig::INVITE => {
            format!("{{\"type\":\"incoming_call\",\"call_id\":\"{call_id}\",\"peer\":{peer},\"video\":{}}}", data.first() == Some(&1))
        }
        Msg::CallSignal { call_id, peer, kind, .. } if *kind == sig::HANGUP || *kind == sig::REJECT => {
            format!("{{\"type\":\"call_ended\",\"call_id\":\"{call_id}\",\"peer\":{peer}}}")
        }
        Msg::RoomEvent { room_id, joined, pid, user_id } => {
            format!("{{\"type\":\"room_event\",\"room\":\"{room_id}\",\"joined\":{joined},\"pid\":{pid},\"user\":{user_id}}}")
        }
        _ => "{\"type\":\"other\"}".into(),
    }
}

// ---- client ------------------------------------------------------------------------------

pub type EventCb = extern "C" fn(*mut c_void, *const c_char);
pub type AudioCb = extern "C" fn(*mut c_void, *const i16, usize);
pub type VideoCb = extern "C" fn(*mut c_void, *const u8, *const u8, *const u8, i32, i32);

pub struct XmpClient {
    inner: Client,
}

struct Ud(*mut c_void);
unsafe impl Send for Ud {}
unsafe impl Sync for Ud {}

/// mode: 0 = login, 1 = register. Returns NULL on failure (see `xmp_last_error`).
/// `cert`/`cert_len`: pinned DER certificate, or NULL to use public CAs.
#[no_mangle]
pub unsafe extern "C" fn xmp_connect(
    server: *const c_char,
    server_name: *const c_char,
    cert: *const u8,
    cert_len: usize,
    mode: i32,
    user: *const c_char,
    password: *const c_char,
    cb: Option<EventCb>,
    ud: *mut c_void,
) -> *mut XmpClient {
    let (Some(server), Some(name), Some(user), Some(pw)) = (cstr(server), cstr(server_name), cstr(user), cstr(password)) else {
        set_err("null or invalid string argument");
        return std::ptr::null_mut();
    };
    let pinned_cert = if cert.is_null() { None } else { Some(std::slice::from_raw_parts(cert, cert_len).to_vec()) };
    let auth = if mode == 1 {
        Auth::Register { username: user.into(), password: pw.into(), display_name: user.into() }
    } else {
        Auth::Login { username: user.into(), password: pw.into(), device_name: "ffi".into() }
    };
    let opts = ConnectOpts { server: server.into(), server_name: name.into(), pinned_cert };
    match rt().block_on(Client::connect(opts, auth)) {
        Ok((client, mut events)) => {
            let ud = Ud(ud);
            rt().spawn(async move {
                let ud = ud;
                while let Some(m) = events.recv().await {
                    if let Some(cb) = cb {
                        if let Ok(c) = CString::new(event_json(&m)) {
                            cb(ud.0, c.as_ptr());
                        }
                    }
                }
            });
            Box::into_raw(Box::new(XmpClient { inner: client }))
        }
        Err(e) => {
            set_err(e);
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn xmp_free(c: *mut XmpClient) {
    if !c.is_null() {
        drop(Box::from_raw(c));
    }
}

#[no_mangle]
pub unsafe extern "C" fn xmp_user_id(c: *const XmpClient) -> i64 {
    (*c).inner.user_id
}

/// Session token for `Auth::Resume`-style reconnects (free with `xmp_string_free`).
#[no_mangle]
pub unsafe extern "C" fn xmp_token(c: *const XmpClient) -> *mut c_char {
    to_c((*c).inner.token.clone())
}

macro_rules! status {
    ($e:expr) => {
        match rt().block_on($e) {
            Ok(_) => 0,
            Err(e) => {
                set_err(e);
                -1
            }
        }
    };
}

#[no_mangle]
pub unsafe extern "C" fn xmp_friend_request(c: *const XmpClient, username: *const c_char) -> i32 {
    let Some(u) = cstr(username) else { return -1 };
    status!((*c).inner.friend_request(u))
}

#[no_mangle]
pub unsafe extern "C" fn xmp_friend_accept(c: *const XmpClient, user_id: i64) -> i32 {
    status!((*c).inner.friend_accept(user_id))
}

/// JSON array: [{"id":..,"username":"..","display_name":"..","status":"sent|received|friends"}]
#[no_mangle]
pub unsafe extern "C" fn xmp_friends_json(c: *const XmpClient) -> *mut c_char {
    match rt().block_on((*c).inner.friends()) {
        Ok(fs) => {
            let items: Vec<String> = fs
                .iter()
                .map(|f| {
                    format!(
                        "{{\"id\":{},\"username\":{},\"display_name\":{},\"status\":\"{}\"}}",
                        f.user_id,
                        jstr(&f.username),
                        jstr(&f.display_name),
                        ["sent", "received", "friends"][(f.status as usize).min(2)]
                    )
                })
                .collect();
            to_c(format!("[{}]", items.join(",")))
        }
        Err(e) => {
            set_err(e);
            std::ptr::null_mut()
        }
    }
}

/// Returns the message seq (>0) or -1 on error.
#[no_mangle]
pub unsafe extern "C" fn xmp_send_text(c: *const XmpClient, to_user: i64, text: *const c_char) -> i64 {
    let Some(t) = cstr(text) else { return -1 };
    match rt().block_on((*c).inner.send_text(to_user, t)) {
        Ok((_, seq, _)) => seq,
        Err(e) => {
            set_err(e);
            -1
        }
    }
}

/// Upload a file and send it to `to_user`. Returns 0 or -1.
#[no_mangle]
pub unsafe extern "C" fn xmp_send_file(c: *const XmpClient, to_user: i64, path: *const c_char) -> i32 {
    let Some(p) = cstr(path) else { return -1 };
    status!((*c).inner.send_file(to_user, std::path::Path::new(p)))
}

/// Download a received file (hash as 64 hex chars) to `dest`. Returns 0 or -1.
#[no_mangle]
pub unsafe extern "C" fn xmp_download_file(c: *const XmpClient, hash_hex: *const c_char, size: u64, dest: *const c_char) -> i32 {
    let (Some(h), Some(d)) = (cstr(hash_hex), cstr(dest)) else { return -1 };
    if h.len() != 64 {
        set_err("hash must be 64 hex chars");
        return -1;
    }
    let mut hash = [0u8; 32];
    for i in 0..32 {
        match u8::from_str_radix(&h[i * 2..i * 2 + 2], 16) {
            Ok(b) => hash[i] = b,
            Err(_) => {
                set_err("bad hex");
                return -1;
            }
        }
    }
    let fr = FileRef { hash, size, name: String::new() };
    status!((*c).inner.download_file(&fr, std::path::Path::new(d)))
}

/// JSON array of messages in a conversation after `after_seq`.
#[no_mangle]
pub unsafe extern "C" fn xmp_sync_json(c: *const XmpClient, conv_id: i64, after_seq: i64, limit: u32) -> *mut c_char {
    match rt().block_on((*c).inner.sync(conv_id, after_seq, limit)) {
        Ok(ms) => to_c(format!("[{}]", ms.iter().map(msg_json).collect::<Vec<_>>().join(","))),
        Err(e) => {
            set_err(e);
            std::ptr::null_mut()
        }
    }
}

/// JSON array: [{"conv":..,"peer":..,"last_seq":..}]
#[no_mangle]
pub unsafe extern "C" fn xmp_conversations_json(c: *const XmpClient) -> *mut c_char {
    match rt().block_on((*c).inner.conversations()) {
        Ok(cs) => to_c(format!(
            "[{}]",
            cs.iter().map(|x| format!("{{\"conv\":{},\"peer\":{},\"last_seq\":{}}}", x.conv_id, x.peer_id, x.last_seq)).collect::<Vec<_>>().join(",")
        )),
        Err(e) => {
            set_err(e);
            std::ptr::null_mut()
        }
    }
}

// ---- calls -------------------------------------------------------------------------------

pub struct XmpCall {
    call: Mutex<Option<Call>>,
    media: xmp_client::session::MediaSession,
    sas: String,
    audio_enc: Mutex<AudioEncoder>,
    video_enc: Mutex<Option<H264Encoder>>,
    target_bps: Arc<AtomicU32>,
    cur_bps: AtomicU32,
    force_key: Arc<AtomicBool>,
    t0: std::time::Instant,
}

fn wrap_call(mut call: Call, audio_cb: Option<AudioCb>, video_cb: Option<VideoCb>, ud: *mut c_void) -> *mut XmpCall {
    let (_dummy_tx, dummy_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut events = std::mem::replace(&mut call.events, dummy_rx);
    let target = Arc::new(AtomicU32::new(call.media.video_target_bps()));
    let force_key = Arc::new(AtomicBool::new(false));
    let (tg, fk, ud) = (target.clone(), force_key.clone(), Ud(ud));
    rt().spawn(async move {
        let ud = ud;
        let mut adec = AudioDecoder::new(AudioConfig::default()).ok();
        let mut vdec = H264Decoder::new().ok();
        while let Some(ev) = events.recv().await {
            match ev {
                MediaEvent::Audio { data, .. } => {
                    if let (Some(d), Some(cb)) = (adec.as_mut(), audio_cb) {
                        if let Ok(pcm) = d.decode(data.as_deref()) {
                            cb(ud.0, pcm.as_ptr(), pcm.len());
                        }
                    }
                }
                MediaEvent::Video { data, .. } => {
                    if let (Some(d), Some(cb)) = (vdec.as_mut(), video_cb) {
                        if let Ok(Some(f)) = d.decode(&data) {
                            cb(ud.0, f.y.as_ptr(), f.u.as_ptr(), f.v.as_ptr(), f.width as i32, f.height as i32);
                        }
                    }
                }
                MediaEvent::Bitrate { video_bps } => tg.store(video_bps, Ordering::Relaxed),
                MediaEvent::KeyframeRequest => fk.store(true, Ordering::Relaxed),
                _ => {}
            }
        }
    });
    let media = call.media.clone();
    let sas = call.sas.clone();
    Box::into_raw(Box::new(XmpCall {
        call: Mutex::new(Some(call)),
        media,
        sas,
        audio_enc: Mutex::new(AudioEncoder::new(AudioConfig::default()).expect("opus")),
        video_enc: Mutex::new(None),
        cur_bps: AtomicU32::new(0),
        target_bps: target,
        force_key,
        t0: std::time::Instant::now(),
    }))
}

/// Call a friend. Blocks until the peer answers and media is set up (up to ~45 s); run it off the UI thread.
#[no_mangle]
pub unsafe extern "C" fn xmp_call_start(c: *const XmpClient, peer: i64, video: i32, audio_cb: Option<AudioCb>, video_cb: Option<VideoCb>, ud: *mut c_void) -> *mut XmpCall {
    match rt().block_on((*c).inner.start_call(peer, video != 0)) {
        Ok(call) => wrap_call(call, audio_cb, video_cb, ud),
        Err(e) => {
            set_err(e);
            std::ptr::null_mut()
        }
    }
}

/// Accept an `incoming_call` event.
#[no_mangle]
pub unsafe extern "C" fn xmp_call_accept(c: *const XmpClient, call_id: u64, peer: i64, video: i32, audio_cb: Option<AudioCb>, video_cb: Option<VideoCb>, ud: *mut c_void) -> *mut XmpCall {
    match rt().block_on((*c).inner.accept_call(call_id, peer, video != 0)) {
        Ok(call) => wrap_call(call, audio_cb, video_cb, ud),
        Err(e) => {
            set_err(e);
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn xmp_call_reject(c: *const XmpClient, call_id: u64, peer: i64) -> i32 {
    status!((*c).inner.reject_call(call_id, peer))
}

/// 6-digit short authentication string to compare with the peer (free with `xmp_string_free`).
#[no_mangle]
pub unsafe extern "C" fn xmp_call_sas(call: *const XmpCall) -> *mut c_char {
    to_c((*call).sas.clone())
}

/// Push exactly one 20 ms stereo frame (1920 interleaved i16 samples at 48 kHz).
#[no_mangle]
pub unsafe extern "C" fn xmp_call_push_audio(call: *const XmpCall, pcm: *const i16, samples: usize) -> i32 {
    let call = &*call;
    let pkt = match call.audio_enc.lock().unwrap().encode(std::slice::from_raw_parts(pcm, samples)) {
        Ok(p) => p,
        Err(e) => {
            set_err(e);
            return -1;
        }
    };
    let (m, ts) = (call.media.clone(), call.t0.elapsed().as_millis() as u32);
    rt().spawn(async move { m.send_audio(ts, &pkt).await });
    0
}

/// Push one I420 frame (tightly packed planes). Encoding happens on the calling thread.
/// Returns 0, or -1 on error. `force_key != 0` requests an IDR.
#[no_mangle]
pub unsafe extern "C" fn xmp_call_push_video(call: *const XmpCall, y: *const u8, u: *const u8, v: *const u8, width: i32, height: i32, force_key: i32) -> i32 {
    let call = &*call;
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || w % 2 != 0 || h % 2 != 0 {
        set_err("width/height must be even and non-zero");
        return -1;
    }
    let frame = Yuv420 {
        width: w,
        height: h,
        y: std::slice::from_raw_parts(y, w * h).to_vec(),
        u: std::slice::from_raw_parts(u, w * h / 4).to_vec(),
        v: std::slice::from_raw_parts(v, w * h / 4).to_vec(),
    };
    let mut guard = call.video_enc.lock().unwrap();
    let want = call.target_bps.load(Ordering::Relaxed).min(8_000_000);
    if guard.is_none() {
        match H264Encoder::new(want, 30.0, 300) {
            Ok(e) => {
                *guard = Some(e);
                call.cur_bps.store(want, Ordering::Relaxed);
            }
            Err(e) => {
                set_err(e);
                return -1;
            }
        }
    }
    let enc = guard.as_mut().unwrap();
    let cur = call.cur_bps.load(Ordering::Relaxed);
    if (want as f64 - cur as f64).abs() > cur as f64 * 0.1 && enc.set_bitrate(want).is_ok() {
        call.cur_bps.store(want, Ordering::Relaxed);
    }
    let force = force_key != 0 || call.force_key.swap(false, Ordering::Relaxed);
    match enc.encode(&frame, force) {
        Ok(Some(f)) => {
            let (m, ts) = (call.media.clone(), call.t0.elapsed().as_millis() as u32);
            rt().spawn(async move { m.send_video_frame(0, ts, f.keyframe, &f.data).await });
            0
        }
        Ok(None) => 0,
        Err(e) => {
            set_err(e);
            -1
        }
    }
}

/// JSON: {"path":"p2p|relay|none","video_target_kbps":..,"tx":..,"rx":..,"retransmits":..,"fec_recovered":..}
#[no_mangle]
pub unsafe extern "C" fn xmp_call_stats_json(call: *const XmpCall) -> *mut c_char {
    use xmp_client::session::Path;
    let call = &*call;
    let s = call.media.stats();
    let path = match call.media.path() {
        Path::P2p(_) => "p2p",
        Path::Relay(_) => "relay",
        Path::None => "none",
    };
    to_c(format!(
        "{{\"path\":\"{path}\",\"video_target_kbps\":{},\"tx\":{},\"rx\":{},\"retransmits\":{},\"fec_recovered\":{}}}",
        call.media.video_target_bps() / 1000,
        s.tx_packets,
        s.rx_packets,
        s.retransmits,
        s.fec_recovered
    ))
}

/// Hang up and free the call handle.
#[no_mangle]
pub unsafe extern "C" fn xmp_call_hangup(call: *mut XmpCall) {
    if call.is_null() {
        return;
    }
    let call = Box::from_raw(call);
    let inner = call.call.lock().unwrap().take();
    if let Some(c) = inner {
        rt().block_on(c.hangup());
    }
}
