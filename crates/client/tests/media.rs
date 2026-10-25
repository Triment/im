use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{net::UdpSocket, sync::mpsc::UnboundedReceiver, time::timeout};
use xmp_client::session::{MediaConfig, MediaEvent, MediaSession, Path};
use xmp_server::media::MediaState;

const KEY: [u8; 32] = [42; 32];

async fn session(pid: u32, server: Option<(SocketAddr, [u8; 16])>) -> (MediaSession, UnboundedReceiver<MediaEvent>, SocketAddr) {
    let sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = sock.local_addr().unwrap();
    let mut cfg = MediaConfig::new(KEY, pid);
    if let Some((s, t)) = server {
        cfg.server = Some(s);
        cfg.token = Some(t);
    }
    let (m, ev) = MediaSession::start(cfg, sock);
    (m, ev, addr)
}

async fn wait_p2p(m: &MediaSession) {
    for _ in 0..200 {
        if matches!(m.path(), Path::P2p(_)) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("never reached p2p, path = {:?}", m.path());
}

/// UDP proxy that connects exactly two endpoints and randomly drops packets.
async fn lossy_proxy(a: SocketAddr, b: SocketAddr, loss: f64) -> SocketAddr {
    let s = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let addr = s.local_addr().unwrap();
    tokio::spawn(async move {
        let mut buf = vec![0u8; 2048];
        loop {
            let Ok((n, src)) = s.recv_from(&mut buf).await else { break };
            if rand::random::<f64>() < loss {
                continue;
            }
            let dst = if src == a { b } else { a };
            let _ = s.send_to(&buf[..n], dst).await;
        }
    });
    addr
}

async fn drain_audio(rx: &mut UnboundedReceiver<MediaEvent>, want: usize, secs: u64) -> (usize, usize) {
    let (mut ok, mut lost) = (0, 0);
    let _ = timeout(Duration::from_secs(secs), async {
        while ok + lost < want {
            match rx.recv().await {
                Some(MediaEvent::Audio { data: Some(_), .. }) => ok += 1,
                Some(MediaEvent::Audio { data: None, .. }) => lost += 1,
                Some(_) => {}
                None => break,
            }
        }
    })
    .await;
    (ok, lost)
}

#[tokio::test]
async fn p2p_direct_upgrade_and_audio() {
    let (a, mut aev, aaddr) = session(1, None).await;
    let (b, mut bev, baddr) = session(2, None).await;
    a.set_peer_candidates(vec![baddr]);
    b.set_peer_candidates(vec![aaddr]);
    wait_p2p(&a).await;
    wait_p2p(&b).await;
    for i in 0..50u32 {
        a.send_audio(i * 20, &[i as u8; 160]).await;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let (ok, _) = drain_audio(&mut bev, 50, 3).await;
    assert!(ok >= 48, "received {ok}/50");
    // reverse direction
    b.send_audio(0, b"pong").await;
    let got = timeout(Duration::from_secs(1), async {
        loop {
            if let Some(MediaEvent::Audio { data: Some(d), from, .. }) = aev.recv().await {
                return (d, from);
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(got, (b"pong".to_vec(), 2));
}

#[tokio::test]
async fn relay_when_p2p_unreachable() {
    let sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let server = sock.local_addr().unwrap();
    let st = Arc::new(MediaState::default());
    let st2 = st.clone();
    tokio::spawn(async move {
        let mut buf = vec![0u8; 2048];
        loop {
            let (n, src) = sock.recv_from(&mut buf).await.unwrap();
            st2.handle_packet(&sock, &buf[..n], src);
        }
    });
    st.call_create(9, 1, 2);
    let (ta, tb) = (st.call_alloc(9, 1).unwrap(), st.call_alloc(9, 2).unwrap());
    let (a, _aev, _) = session(1, Some((server, ta))).await;
    let (b, mut bev, _) = session(2, Some((server, tb))).await;
    // candidates that go nowhere (blackholed port)
    let dead: SocketAddr = "127.0.0.1:9".parse().unwrap();
    a.set_peer_candidates(vec![dead]);
    b.set_peer_candidates(vec![dead]);
    tokio::time::sleep(Duration::from_millis(400)).await; // let Hello bind
    assert!(matches!(a.path(), Path::Relay(_)));
    for i in 0..30u32 {
        a.send_audio(i * 20, &[1; 100]).await;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let (ok, _) = drain_audio(&mut bev, 30, 3).await;
    assert!(ok >= 28, "relay delivered {ok}/30");
    assert!(st.stats.relayed.load(std::sync::atomic::Ordering::Relaxed) >= 28);
}

#[tokio::test]
async fn video_survives_10_percent_loss_via_nack_and_fec() {
    let (a, _aev, aaddr) = session(1, None).await;
    let (b, mut bev, baddr) = session(2, None).await;
    let proxy = lossy_proxy(aaddr, baddr, 0.10).await;
    a.set_peer_candidates(vec![proxy]);
    b.set_peer_candidates(vec![proxy]);
    wait_p2p(&a).await;

    const FRAMES: u32 = 90;
    let frame: Vec<u8> = (0..4000u32).map(|x| (x % 251) as u8).collect();
    let sender = tokio::spawn({
        let (a, frame) = (a.clone(), frame.clone());
        async move {
            for i in 0..FRAMES {
                a.send_video_frame(0, i * 33, i == 0, &frame).await;
                // audio alongside video
                a.send_audio(i * 33, &[7; 120]).await;
                tokio::time::sleep(Duration::from_millis(33)).await;
            }
        }
    });
    let mut frames = 0;
    let _ = timeout(Duration::from_secs(8), async {
        while frames < FRAMES {
            match bev.recv().await {
                Some(MediaEvent::Video { data, .. }) => {
                    assert_eq!(data, frame, "frame corrupted");
                    frames += 1;
                }
                None => break,
                _ => {}
            }
        }
    })
    .await;
    sender.await.unwrap();
    let s = a.stats();
    println!("frames {frames}/{FRAMES}, retransmits {}, recv fec {}", s.retransmits, b.stats().fec_recovered);
    // 10% packet loss, 4 packets/frame => ~34% of frames would break without recovery
    assert!(frames as f64 >= FRAMES as f64 * 0.9, "only {frames}/{FRAMES} frames");
    assert!(s.retransmits > 0, "NACK path never exercised");
}
