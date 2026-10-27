# xmp_flutter

The Flutter client uses `flutter_rust_bridge` native assets. The hook builds the
Rust library for the current Flutter target, so the same configuration covers
Android (arm64/armv7/x64/ x86), macOS (Apple Silicon and Intel), Linux and
Windows. Install the targets listed in `rust/rust-toolchain.toml` before a
release build; Android builds also require the Android NDK configured by
Flutter.

```sh
flutter pub get
flutter_rust_bridge_codegen generate --config-file flutter_rust_bridge.yaml
flutter build apk       # Android
flutter build macos     # macOS
flutter build linux     # Linux
flutter build windows   # Windows
```

The login form accepts the QUIC server address, TLS server name, and a local
DER/CRT/CER/PEM certificate file. The file is read and encoded as Base64 by the
client; users no longer need to paste certificate data. It also supports either
username/password or an existing session token.

Android builds use the NDK `libc++_shared.so` runtime automatically. Do not
copy a single ABI-specific `libxmp_flutter_rust.so` into `android/app`; the
FRB native-assets hook owns the Rust library packaging.

For release signing and CI secrets, see `docs/RELEASING.md`.
