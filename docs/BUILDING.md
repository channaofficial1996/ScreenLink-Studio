# Building ScreenLink Studio

## Prerequisites

| Tool | Windows | macOS |
| --- | --- | --- |
| Node.js 22+ | nodejs.org | nodejs.org |
| Rust stable | `winget install Rustlang.Rustup` | rustup.rs |
| C/C++ toolchain | Visual Studio Build Tools with "Desktop development with C++" | `xcode-select --install` |
| WebView runtime | Included with Windows 11; the installer fetches it elsewhere | Built in |

## Set up

```bash
npm install
npm run setup   # downloads scrcpy-server (checksum verified) and adb into src-tauri/resources
```

## Develop

```bash
npm run tauri dev
```

## Test

```bash
npm test             # frontend
npm run test:rust    # backend (no hardware needed)

# With an authorized Android device attached (needs ffmpeg and ffprobe on PATH):
cargo test --manifest-path src-tauri/Cargo.toml --lib hardware -- --ignored --nocapture --test-threads=1
```

## Package

### Windows

```bash
npm run tauri build
```

Outputs:

- `src-tauri/target/release/bundle/nsis/ScreenLink Studio_1.0.0_x64-setup.exe`
- `src-tauri/target/release/bundle/msi/ScreenLink Studio_1.0.0_x64_en-US.msi`

### macOS

Packaging must run on a Mac. To build a universal `.dmg` for Apple Silicon and Intel:

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
node scripts/fetch-sidecars.mjs --platform=mac
npm run tauri build -- --target universal-apple-darwin
```

`.github/workflows/build.yml` does this on GitHub's macOS runners. Run it from the Actions tab or
by pushing a `v*` tag.

## Signing

Builds are **unsigned**, so Windows SmartScreen warns on first run and macOS Gatekeeper blocks the
app. Before public distribution:

- **Windows:** set `bundle.windows.certificateThumbprint` in `tauri.conf.json`, or use a signing
  service, with an OV or EV code signing certificate.
- **macOS:** join the Apple Developer Program, set `APPLE_SIGNING_IDENTITY`, and notarize with
  `APPLE_ID`, `APPLE_PASSWORD` and `APPLE_TEAM_ID`. `macos.entitlements` already declares the
  camera entitlement that iPhone capture needs.

## Dependencies bundled at build time

| Component | Source | License |
| --- | --- | --- |
| scrcpy-server 5.0.1 | github.com/Genymobile/scrcpy | Apache-2.0 |
| adb and its Windows DLLs | dl.google.com platform-tools | Android SDK terms |

See [THIRD_PARTY.md](THIRD_PARTY.md) before distributing.

## Upgrading scrcpy

1. Change `SCRCPY_VERSION` in both `scripts/fetch-sidecars.mjs` and `src-tauri/src/android/server.rs`.
2. Read the protocol changes at the new tag (`doc/develop.md`, `app/src/control_msg.c`,
   `app/src/demuxer.c`, `app/src/server.c`).
3. Run `node scripts/fetch-sidecars.mjs --force`, then the hardware tests.
