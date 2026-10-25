# XMP App — cross-platform client (Tauri 2 + React)

One codebase for Windows, macOS, Linux (desktop) and iOS / Android (Tauri mobile).

```
src/            React + TypeScript + Tailwind UI (login, chats, files, calls)
  api.ts        Tauri commands + a demo backend (open the UI in any browser with  #demo  to preview)
  media.ts      mic/camera capture, AudioWorklet playback with low-latency catch-up
  store.ts      app state, call lifecycle
src-tauri/      Rust backend: wraps xmp-client + xmp-codec, binary IPC for media
```

## Why Tauri
The Rust backend links `xmp-client` / `xmp-codec` directly (no FFI layer), the webview provides capture,
rendering and echo cancellation on every platform, and the installer is a few MB.

## Media path
`getUserMedia` → AudioWorklet (48 kHz stereo i16, 20 ms) / canvas (RGBA) → **binary** Tauri IPC → Rust:
RGBA→I420, Opus / H.264, FEC, pacing, P2P/relay → network. Receive path mirrors it back through `Channel`s.
JSON is only used for control events.

## Develop / build
```bash
npm install
npm run tauri dev                      # hot-reload desktop app
npm run tauri build                    # installers (needs libwebkit2gtk-4.1-dev etc. on Linux)
npm run dev   # then open http://localhost:1420/#demo   (pure-browser UI preview, no backend)
```
Requires Rust >= 1.90, Node >= 20, plus `libopus-dev cmake nasm g++` (codec crate).
Add `#demo` / `#demo,incoming` to the URL to preview the chat, call and incoming-call screens.
Mobile: `npm run tauri android init|dev`, `npm run tauri ios init|dev` (not built or tested yet).

## Settings, secrets, certificates
* Server address, TLS name, pinned certificate, username and the "remember password" choice are saved automatically on every change.
* The pinned server certificate is chosen with a file picker (`.der` / `.pem`); its content is stored, so it also works with Android content URIs.
* The session token (auto-login) and the optional remembered password go to the **OS keychain** on desktop (Credential Manager / Keychain / Secret Service).
  If the keychain exists but fails, nothing is stored (no silent plaintext fallback). On mobile they use the app-private sandbox.

## Android
`src-tauri/gen/android` is committed and includes release signing (see `docs/RELEASING.md`) plus CAMERA / RECORD_AUDIO permissions. Never built here.

## Known gaps
* Software H.264 only (OpenH264). Next step is WebCodecs / platform hardware encoders behind the same IPC.
* RGBA frames cross the IPC boundary (fine for 360p–720p; encode in the webview with WebCodecs for 1080p+).
* WebKitGTK (Linux) may need extra GStreamer plugins for `getUserMedia`.
* Group chat, voice messages, push notifications, offline call alerts are not implemented.
