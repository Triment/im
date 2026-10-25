//! Reference command-line client / test tool. Real UIs bind to `xmp-client` instead.
//!
//! Examples:
//!   xmp --cert data/cert.der register alice secret123
//!   xmp --cert data/cert.der -u alice -p secret123 chat          # interactive: /friend, /msg, /file, /call
//!   xmp --cert data/cert.der -u alice -p secret123 listen        # wait for a call and answer it
//!   xmp --cert data/cert.der -u bob   -p secret123 call alice    # synthetic 48 kHz audio + video, prints stats

use clap::{Parser, Subcommand};
use std::{path::PathBuf, time::Duration};
use tokio::io::{AsyncBufReadExt, BufReader};
use xmp_client::{call::Call, session::MediaEvent, Auth, Client, ConnectOpts};
use xmp_proto::control::{msg_kind, sig, FileRef, Msg};

#[derive(Parser)]
#[command(name = "xmp")]
struct Args {
    #[arg(long, default_value = "127.0.0.1:4433")]
    server: String,
    #[arg(long, default_value = "localhost")]
    server_name: String,
    /// Pinned server certificate (DER). Omit to use public CAs.
    #[arg(long)]
    cert: Option<PathBuf>,
    #[arg(short, long, default_value = "")]
    user: String,
    #[arg(short, long, default_value = "")]
    password: String,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create an account (then stays connected for the command, if any)
    Register { username: String, password: String },
    /// Interactive chat shell
    Chat,
    /// Wait for an incoming call, accept it, run for 10 s and print stats
    Listen,
    /// Call a friend by username with synthetic media and print stats
    Call { peer: String, #[arg(long, default_value_t = 10)] secs: u64, #[arg(long, default_value_t = 640)] width: usize, #[arg(long, default_value_t = 360)] height: usize },
}

async fn connect(a: &Args, auth: Auth) -> anyhow::Result<(Client, tokio::sync::mpsc::UnboundedReceiver<Msg>)> {
    let pinned_cert = match &a.cert {
        Some(p) => Some(std::fs::read(p)?),
        None => None,
    };
    Ok(Client::connect(ConnectOpts { server: a.server.clone(), server_name: a.server_name.clone(), pinned_cert }, auth).await?)
}

fn login(a: &Args) -> Auth {
    Auth::Login { username: a.user.clone(), password: a.password.clone(), device_name: "xmp-cli".into() }
}

async fn run_media(mut call: Call, secs: u64, w: usize, h: usize) {
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::Arc;
    use xmp_codec::{
        audio::{AudioConfig, AudioDecoder, AudioEncoder},
        testsrc::{moving_frame, sine_frame},
        video::{H264Decoder, H264Encoder, VideoDecoder, VideoEncoder},
    };
    println!("call established, SAS = {} (compare with peer), video={}", call.sas, call.video);
    let m = call.media.clone();
    let force_key = Arc::new(AtomicBool::new(false));
    let target = Arc::new(AtomicU32::new(m.video_target_bps()));
    let (fk, tg) = (force_key.clone(), target.clone());
    let send = tokio::spawn(async move {
        let acfg = AudioConfig::default();
        let mut aenc = AudioEncoder::new(acfg.clone()).unwrap();
        let mut venc = H264Encoder::new(tg.load(Ordering::Relaxed).min(4_000_000), 30.0, 300).unwrap();
        let mut cur_bps = tg.load(Ordering::Relaxed);
        let t0 = std::time::Instant::now();
        let (mut next_a, mut next_v, mut na, mut nv) = (0u32, 0u32, 0usize, 0usize);
        loop {
            let ms = t0.elapsed().as_millis() as u32;
            if ms >= secs as u32 * 1000 {
                break;
            }
            if ms >= next_a {
                let pkt = aenc.encode(&sine_frame(na, &acfg)).unwrap();
                m.send_audio(ms, &pkt).await;
                next_a += 20;
                na += 1;
            }
            if ms >= next_v {
                let want = tg.load(Ordering::Relaxed).min(8_000_000);
                if (want as f64 - cur_bps as f64).abs() > cur_bps as f64 * 0.1 {
                    let _ = venc.set_bitrate(want);
                    cur_bps = want;
                }
                let force = nv == 0 || fk.swap(false, Ordering::Relaxed);
                if let Ok(Some(f)) = venc.encode(&moving_frame(w, h, nv), force) {
                    m.send_video_frame(0, ms, f.keyframe, &f.data).await;
                }
                next_v += 33;
                nv += 1;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    });
    let mut adec = AudioDecoder::new(AudioConfig::default()).unwrap();
    let mut vdec = H264Decoder::new().unwrap();
    let (mut audio, mut plc, mut frames, mut keyreq, mut samples) = (0u64, 0u64, 0u64, 0u64, 0usize);
    let mut dims = (0, 0);
    let end = tokio::time::Instant::now() + Duration::from_secs(secs + 1);
    while let Ok(Some(ev)) = tokio::time::timeout_at(end, call.events.recv()).await {
        match ev {
            MediaEvent::Audio { data, .. } => {
                if data.is_some() { audio += 1 } else { plc += 1 }
                if let Ok(pcm) = adec.decode(data.as_deref()) {
                    samples += pcm.len();
                }
            }
            MediaEvent::Video { data, .. } => {
                if let Ok(Some(f)) = vdec.decode(&data) {
                    frames += 1;
                    dims = (f.width, f.height);
                }
            }
            MediaEvent::PathChanged(p) => println!("path -> {p:?}"),
            MediaEvent::Bitrate { video_bps } => target.store(video_bps, Ordering::Relaxed),
            MediaEvent::KeyframeRequest => {
                keyreq += 1;
                force_key.store(true, Ordering::Relaxed);
            }
            _ => {}
        }
    }
    let _ = send.await;
    let s = call.media.stats();
    println!(
        "received: opus frames {audio} (concealed {plc}, {} s of PCM decoded), h264 frames {frames} decoded at {}x{}, keyframe requests served {keyreq}",
        samples / 2 / 48_000, dims.0, dims.1
    );
    println!("final path {:?}, video target {} kbps, stats {s:?}", call.media.path(), call.media.video_target_bps() / 1000);
    call.hangup().await;
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();
    let a = Args::parse();
    match &a.cmd {
        Cmd::Register { username, password } => {
            let (c, _) = connect(&a, Auth::Register { username: username.clone(), password: password.clone(), display_name: username.clone() }).await?;
            println!("registered {username}, user_id={}", c.user_id);
        }
        Cmd::Chat => {
            let (c, mut ev) = connect(&a, login(&a)).await?;
            println!("logged in as {} (id {}). commands: /friend <name> | /accept <id> | /friends | /msg <id> <text> | /sync <conv> [after] | /file <id> <path> | /get <hash-hex> <size> <name> | /ping | /quit", a.user, c.user_id);
            let c2 = c.clone();
            tokio::spawn(async move {
                while let Some(m) = ev.recv().await {
                    match m {
                        Msg::PushMessage(m) if m.kind == msg_kind::TEXT => println!("\n[{}#{}] user {}: {}", m.conv_id, m.seq, m.sender, String::from_utf8_lossy(&m.body)),
                        Msg::PushMessage(m) if m.kind == msg_kind::FILE => {
                            if let Ok(f) = FileRef::decode(&m.body) {
                                println!("\n[{}#{}] user {} sent file {} ({} bytes)", m.conv_id, m.seq, m.sender, f.name, f.size);
                            }
                        }
                        Msg::Presence { user_id, online } => println!("\n* user {user_id} is {}", if online { "online" } else { "offline" }),
                        Msg::CallSignal { call_id, peer, kind: sig::INVITE, .. } => println!("\n* incoming call {call_id} from user {peer}; run `xmp listen` to answer"),
                        other => println!("\n* {other:?}"),
                    }
                }
                let _ = &c2;
            });
            let mut lines = BufReader::new(tokio::io::stdin()).lines();
            while let Some(line) = lines.next_line().await? {
                let parts: Vec<&str> = line.splitn(3, ' ').collect();
                let r: Result<(), xmp_client::Error> = async {
                    match parts[0] {
                        "/friend" => c.friend_request(parts[1]).await?,
                        "/accept" => c.friend_accept(parts[1].parse().unwrap_or(0)).await?,
                        "/friends" => {
                            for f in c.friends().await? {
                                println!("  {} {} ({}) status={}", f.user_id, f.username, f.display_name, ["sent", "received", "friends"][f.status as usize]);
                            }
                        }
                        "/msg" => {
                            let (conv, seq, _) = c.send_text(parts[1].parse().unwrap_or(0), parts.get(2).copied().unwrap_or("")).await?;
                            println!("  sent as {conv}#{seq}");
                        }
                        "/sync" => {
                            let after = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
                            for m in c.sync(parts[1].parse().unwrap_or(0), after, 100).await? {
                                println!("  #{} from {}: {}", m.seq, m.sender, String::from_utf8_lossy(&m.body));
                            }
                        }
                        "/file" => {
                            let f = c.send_file(parts[1].parse().unwrap_or(0), std::path::Path::new(parts.get(2).copied().unwrap_or(""))).await?;
                            println!("  uploaded {} hash {}", f.name, f.hash.iter().map(|b| format!("{b:02x}")).collect::<String>());
                        }
                        "/ping" => println!("  rtt {:?}", c.ping().await?),
                        "/quit" => std::process::exit(0),
                        _ => println!("  unknown command"),
                    }
                    Ok(())
                }
                .await;
                if let Err(e) = r {
                    println!("  error: {e}");
                }
            }
        }
        Cmd::Listen => {
            let (c, mut ev) = connect(&a, login(&a)).await?;
            println!("waiting for a call as {}...", a.user);
            while let Some(m) = ev.recv().await {
                if let Msg::CallSignal { call_id, peer, kind: sig::INVITE, data } = m {
                    let call = c.accept_call(call_id, peer, data.first() == Some(&1)).await?;
                    run_media(call, 10, 640, 360).await;
                    break;
                }
            }
        }
        Cmd::Call { peer, secs, width, height } => {
            let (c, _ev) = connect(&a, login(&a)).await?;
            let peer_id = c.friends().await?.into_iter().find(|f| &f.username == peer && f.status == 2).map(|f| f.user_id);
            let Some(peer_id) = peer_id else { anyhow::bail!("{peer} is not a friend") };
            let call = c.start_call(peer_id, true).await?;
            run_media(call, *secs, *width, *height).await;
        }
    }
    Ok(())
}
