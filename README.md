# XMP — low-latency IM + voice/video stack (Rust)

iMessage + FaceTime style: 1:1 chat with text/voice/file, 1:1 calls (P2P with relay fallback), multi-party
video conferences (SFU). Server in Rust (tokio, quinn), storage in PostgreSQL, custom UDP media protocol.

```
crates/
  proto/    wire formats: control frames (QUIC stream) + 16-byte XMP media header
  media/    AEAD+Noise, Reed-Solomon FEC, jitter buffer, pacer, delay-gradient congestion control
  server/   QUIC gateway, PostgreSQL layer, blob store, UDP relay + SFU
  client/   client core library: control API, files, media session, calls, rooms
  codec/    Opus (48 kHz stereo, 192 kbps default) + H.264 (OpenH264 software; `VideoEncoder` trait for hardware)
  ffi/      C ABI (`xmp.h`) over client+codec: UIs push raw PCM/I420, get decoded PCM/I420 + JSON events
  cli/      reference CLI client / test tool (`xmp`), uses the real codecs
```

## Run

```bash
createdb xmp                                   # any PostgreSQL >= 13
cargo run --release -p xmp-server -- \
    --db "host=localhost user=postgres dbname=xmp" \
    --udp-public YOUR.PUBLIC.IP:4434           # advertised media address
# first start writes data/cert.der (self-signed). Clients pin it:
export XMP="cargo run --release -p xmp-cli -- --cert data/cert.der"
$XMP register alice password1 ; $XMP register bob password2
$XMP -u alice -p password1 chat        # /friend bob   then in bob's shell: /accept <id>
$XMP -u alice -p password1 listen      # terminal 1
$XMP -u bob   -p password2 call alice  # terminal 2 (synthetic audio+video, prints path/bitrate/loss stats)
```
Set `XMP_TLS_NAMES=chat.example.com,1.2.3.4` before the first start to add SANs to the generated cert.
Open UDP 4434 and UDP 4433 (QUIC) on the firewall.

## Protocol

**Control plane** — QUIC (quinn), ALPN `xmp/1`, one bidirectional stream with `len|req_id|kind|body` frames,
hand-written binary codec (`proto/src/control.rs`). Server pushes use `req_id=0`. QUIC gives connection
migration (Wi-Fi ↔ cellular) and no head-of-line blocking across connections.

**Media plane** — custom UDP protocol, 16-byte header (`proto/src/media.rs`):
`ver|type | flags | layer(spatial:temporal) | fec(k:idx) | stream_id | seq | ts_ms`.
Header is AEAD-authenticated but clear so relays/SFU route without decrypting; payload is ChaCha20-Poly1305,
nonce derived from header fields; 1024-packet anti-replay window.

| Concern | Implementation |
|---|---|
| Key agreement (1:1) | Noise_XX_25519_ChaChaPoly_BLAKE2s over the relayed signalling channel; media key = blake3-KDF(handshake hash); 6-digit SAS shown to both users |
| NAT traversal | `Hello{token}` to server returns the reflexive address (STUN-like) → candidates exchanged via signalling → encrypted Probe/ProbeAck punching |
| Path selection | Media starts on the **server relay immediately**, upgrades to P2P when an authenticated ProbeAck arrives, falls back to relay if P2P is silent for 3 s |
| Audio | small-group RS FEC (k≤4, adaptive to loss), adaptive jitter buffer (20–200 ms), explicit loss events for PLC; codec is pluggable (Opus 48 kHz fullband stereo 128–256 kbps recommended) |
| Video | fragmentation, pacer (2.5× rate), RS FEC (adaptive k/m), NACK retransmit (3 tries, 30 ms), PLI key-frame request |
| Congestion control | receiver-side delay-gradient trendline (GCC-style, adaptive threshold) → REMB feedback every 200 ms + sender loss controller; audio budget reserved first |
| Conference | SFU: audio forwarded to all, video filtered per subscriber by spatial layer from its reported downlink estimate (`Report`) |
| Files | 1 MiB chunks, 4 in flight, blake3 content address, resumable (server returns missing chunks), dedup by hash, verified on server and client |
| Messages | per-conversation monotonic `seq` allocated under a row lock; idempotent sends via `(sender, client_id)`; incremental `Sync(after_seq)`; `messages` hash-partitioned by `conv_id` |

## Tests
`cargo test --workspace` — wire round-trips, AEAD/Noise, FEC recovery, jitter buffer, congestion detection,
blob store resume/verify, UDP relay & SFU layer selection, P2P upgrade, relay fallback, and a 10 %-loss
video test (NACK+FEC) over a lossy proxy.

## Codecs and UI bindings
`xmp-codec` needs `libopus-dev`, `cmake`, `nasm` and a C++ compiler at build time (`apt install libopus-dev cmake nasm g++`).
Measured here: Opus stereo 20 ms frames SNR 34.6 dB at 194 kbps; OpenH264 720p PSNR >= 45 dB on the synthetic source;
live bitrate retargeting and IDR-on-request work (key-frame requests from the receiver are served end to end).

`crates/ffi` builds `libxmp_ffi.{so,dylib,a}`; `crates/ffi/xmp.h` is the whole API and `crates/ffi/tests/smoke.c`
is a working example (login, chat, call with raw PCM/YUV, decoded callbacks). Any UI toolchain with a C FFI can bind it:
Swift (bridging header), Kotlin/JNI, Dart `ffi`, Tauri/Electron native module, Qt/C++.
Android/iOS targets are `cargo build --target aarch64-linux-android|aarch64-apple-ios -p xmp-ffi` (NOT yet built or tested).

## Known gaps (be honest about these before shipping)
* **No graphical client yet, and no hardware video encoders.** Capture/render/UI and VideoToolbox / MediaCodec /
  NVENC / VAAPI backends are platform work behind the `VideoEncoder` trait and the C API. Software H.264 is the only
  encoder, so expect high CPU at 1080p. Web cannot send custom UDP (needs WebTransport).
* All performance numbers so far are from debug builds on a single-core sandbox over loopback. Re-measure with
  `--release` on real hardware and real networks (`tc netem`); CC/FEC thresholds are untuned defaults.
* **SFU conferences are not end-to-end encrypted**: the room key is minted by the server. Replace with
  SFrame-style keys distributed over pairwise Noise channels. SFU also lacks NACK/PLI routing and cascading.
* 1:1 call keys use ephemeral Noise statics (SAS verification only); persist device keys for TOFU/pinning.
* File transfer always goes through the server (no P2P QUIC path yet); anyone holding a file hash can fetch it.
* Offline-call push notifications (APNs/FCM), group chat, read receipts, TURN-over-TCP/443 fallback for UDP-blocked
  networks, rate limiting, and multi-node SFU are not implemented.
* Single UDP socket/thread on the server; for >100k pps use `SO_REUSEPORT` sockets per core and `recvmmsg`.

## CI / releases
`.github/workflows/ci.yml` runs tests and the frontend build on every push/PR; pushing a `v*` tag runs `release.yml`
(server binaries for 5 platforms, desktop installers, a signed Android APK, checksums). Setup and signing: `docs/RELEASING.md`.
docker run -d   --name xmp-server   --restart unless-stopped   -p 4433:4433/udp -p 4433:4433/tcp  -p 4434:4434/udp   --entrypoint /app/xmp-server   -e PGPASSWORD='admin@cd123'   xmp-server   --db 'host=search-postgres port=5432 user=admin password=admin@cd123 dbname=xmp'