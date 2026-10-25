# Releasing

Pushing a tag `vX.Y.Z` runs `.github/workflows/release.yml`:

| Job | Output |
|---|---|
| `test` | `cargo test --workspace` (gate for everything below) |
| `server` | `xmp-server-<ver>-{linux-x86_64,linux-aarch64,macos-aarch64,macos-x86_64,windows-x86_64}` (`.tar.gz` / `.zip`) |
| `desktop` | Linux `.deb` `.rpm` `.AppImage`, macOS `.dmg` (arm64 + x64), Windows `.msi` + `.exe` (NSIS) |
| `android` | `XMP-<ver>-android.apk`, **signed with your release key** (best effort, see below) |
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

## Optional: macOS signing and notarization
Set `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` secrets; unset means unsigned builds (Gatekeeper will warn). Windows builds are unsigned unless you add a code-signing step.

## What has and has not been verified
* Workflow syntax passes `actionlint`; lockfiles pass `cargo metadata --locked`.
* **The workflows have never run on GitHub.** Expect a first-run fix or two, most likely in the Windows/macOS native builds (OpenH264 needs NASM, Opus needs CMake) and in the Android job, which cross-compiles those C/C++ libraries with the NDK and is therefore marked `continue-on-error`.
* The Android Gradle signing block and manifest permissions were added to a CLI-generated project but never built here (no Android SDK / Gradle download in the sandbox).
