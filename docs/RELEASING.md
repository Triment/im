# Releasing

Pushing a tag `vX.Y.Z` runs `.github/workflows/release.yml`:

| Job | Output |
|---|---|
| `test` | `cargo test --workspace` (gate for everything below) |
| `server` | `xmp-server-<ver>-{linux-x86_64,linux-aarch64,macos-aarch64,macos-x86_64,windows-x86_64}` (`.tar.gz` / `.zip`) |
| `desktop` | Linux `.deb` `.rpm` `.AppImage`, macOS `.dmg` (arm64 + x64, unsigned), Windows `.msi` + `.exe` (NSIS, unsigned) |
| `android` | `XMP-<ver>-android.apk` (arm64-v8a + armeabi-v7a), **signed with your release key** (best effort, see below) |
| `publish` | `SHA256SUMS.txt`, then the draft release becomes public |

A manual run (Actions -> Release -> Run workflow) builds everything but publishes nothing; download the artifacts from the run page.

```bash
git tag v0.2.0 && git push origin v0.2.0
```

## Android signing

1. Generate a keystore (needs a JDK for `keytool`):
   ```bash
   scripts/gen-android-keystore.sh release-keystore.jks xmp
   ```
   The script prints the values for the four repository secrets. **Back up the `.jks` and its password:** an app signed with a lost key can never be updated in place.
2. Add the secrets in *Settings -> Secrets and variables -> Actions*:

   | Secret | Value |
   |---|---|
   | `ANDROID_KEYSTORE_BASE64` | base64 of the `.jks` |
   | `ANDROID_KEYSTORE_PASSWORD` | keystore password |
   | `ANDROID_KEY_ALIAS` | key alias (`xmp`) |
   | `ANDROID_KEY_PASSWORD` | key password (defaults to the keystore password) |

3. Tag a release. For tag builds the job **fails instead of publishing a debug-signed APK** when a secret is missing. The workflow also runs `apksigner verify --print-certs` so the signer fingerprint appears in the log.

Local signed build: copy `src-tauri/gen/android/keystore.properties.example` to `keystore.properties` (git-ignored), fill it in and run `npm run tauri android build -- --apk`. Without credentials the release build falls back to the debug key with a warning (fine for local testing, never publish it).
The signing logic lives in `apps/xmp-app/src-tauri/gen/android/app/build.gradle.kts`. It reads `keystore.properties` first, then the `ANDROID_KEYSTORE_PATH`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD` environment variables.

## Android SDK / NDK setup
The `android` job installs the SDK platform (37.0), Build Tools (36.0.0) and NDK (`NDK_VERSION` at the top of `release.yml`) with
[`amyu/setup-android`](https://github.com/amyu/setup-android), pinned to the full commit SHA of v6.0 as that project recommends
(update the SHA and the `# v6.0` comment together when bumping). The action exports `ANDROID_HOME` / `ANDROID_NDK_HOME`; one extra
step copies the latter into `NDK_HOME`, which the Tauri CLI reads. Keep `sdk-version` in sync with `compileSdk` in
`src-tauri/gen/android/app/build.gradle.kts`. Its README notes that GitHub-hosted runners already ship an SDK; this action is used
here to get pinned, cached, reproducible versions instead of whatever the runner image contains.

## Versions
The workflow does **not** rewrite any version. Before tagging, bump them yourself so the artifacts carry the right number:
`[workspace.package] version` in `Cargo.toml` (server) and `version` in `apps/xmp-app/src-tauri/tauri.conf.json` (desktop installers and Android `versionName`).
Release asset names use the tag. The `prepare` job only prints a warning if these files disagree with the tag; it never fails or edits anything.
(Rewriting versions in CI is what used to break `--locked` builds and produced invalid installer versions for non-release runs.)

## Unsigned desktop builds
macOS and Windows installers are **not code-signed or notarized** (no Apple Developer account / certificate is used). Users will see
Gatekeeper / SmartScreen warnings: on macOS right-click -> Open the first time, or run `xattr -cr /Applications/XMP.app` if the app is reported as damaged.

## Android ABIs
Only `arm64-v8a` and `armeabi-v7a` are built (`aarch64` / `armv7`); x86 and x86_64 are excluded because the native codec stack does not compile for them.
This is enforced twice: the workflow passes `--target aarch64 armv7`, and the defaults in `gen/android/buildSrc/.../RustPlugin.kt` were trimmed to the same two ABIs, so local `tauri android build` and a regenerated project behave the same
(if you ever re-run `tauri android init`, re-apply that edit). Consequence: the APK does not install on x86 emulators; use an ARM image or a real device.

## What has and has not been verified
* Workflow syntax passes `actionlint`; lockfiles pass `cargo metadata --locked`.
* **The workflows have never run on GitHub.** Expect a first-run fix or two, most likely in the Windows/macOS native builds (OpenH264 needs NASM, Opus needs CMake) and in the Android job, which cross-compiles those C/C++ libraries with the NDK and is therefore marked `continue-on-error`.
* The Android Gradle signing block and manifest permissions were added to a CLI-generated project but never built here (no Android SDK / Gradle download in the sandbox).
# XMP Release Guide

## Flutter native assets

The Flutter client is built through `flutter_rust_bridge` native assets. The
GitHub workflows generate the bindings and build the Rust library for the
target platform. The Android Gradle task copies `libc++_shared.so` from the
selected NDK into every supported ABI, which avoids the runtime error:
`libc++_shared.so not found`.

Android releases are published as a zip containing split APKs for arm64,
armv7, x86_64, and x86. Desktop releases are built separately for macOS
Apple Silicon, macOS Intel, Linux x86_64, and Windows x86_64.

## Android signing

The release workflow can publish a signed Android build when these repository
secrets are configured:

- `ANDROID_KEYSTORE_BASE64`: base64-encoded `.jks` or `.keystore` file
- `ANDROID_KEYSTORE_PASSWORD`
- `ANDROID_KEY_ALIAS`
- `ANDROID_KEY_PASSWORD`

Example preparation command:

```sh
base64 -i release.jks | pbcopy       # macOS
base64 -w 0 release.jks              # Linux
```

Paste the resulting value into the GitHub Actions secret. The workflow writes
the keystore only to the runner's temporary directory and passes its path to
Gradle through `ANDROID_KEYSTORE_PATH`. When the secrets are absent, the
workflow uses the debug key so unsigned/dev artifacts can still be tested, but
those APKs must not be distributed as production releases.

The signing key must remain stable across updates. Android will reject an
upgrade if a new release is signed by a different key. Keep an encrypted backup
of the keystore and its passwords outside GitHub.

## Tags and artifacts

Push a tag such as `v0.2.0` to create a draft release. The workflow uploads
server binaries, split Android APKs, macOS arm64/x86_64 archives, Linux and
Windows archives, then publishes `SHA256SUMS.txt`. `workflow_dispatch` runs the
same builds without publishing a release.

Local Android verification:

```sh
flutter clean
flutter pub get
flutter run -d <android-device>
```

The app's Gradle task resolves the configured NDK and packages the C++ runtime;
no manually copied `.so` file is required under `android/app/src/main/jniLibs`.
