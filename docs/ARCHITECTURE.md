# Architecture

ScreenLink Studio is a Tauri 2 app: a Rust backend and a React frontend in the system webview
(WebView2 on Windows, WKWebView on macOS).

## Embedded preview: how the video gets inside the app

Most scrcpy front ends launch `scrcpy.exe` and show its window. That is not embedding, and
reparenting that window does not work on macOS. ScreenLink instead replaces the scrcpy *client*
and keeps scrcpy's official *server* unmodified:

```
Android device                       Rust backend                    Webview
┌───────────────┐   adb forward     ┌───────────────────┐  loopback  ┌─────────────────┐
│ scrcpy-server │ ─ video socket ─▶ │ stream.rs demuxer │  WebSocket │ WebCodecs       │
│ (official jar)│ ─ audio socket ─▶ │        │          │ ─ /media ▶ │ VideoDecoder    │
│               │ ◀ control sock ─  │        ▼          │            │   ▼             │
└───────────────┘                   │ recording (MP4)   │            │ <canvas>        │
                                    │ control.rs encoder│ ◀ /input ─ │ pointer + keys  │
                                    └───────────────────┘            └─────────────────┘
```

Why this design:

- **Low latency.** Compressed H.264 goes to the GPU decoder with no re-encoding, and frames are
  drawn as soon as they decode. Rust never decodes video.
- **Same code on both platforms.** No native window handling.
- **Quality.** Pixels are the device's own encoder output, and `recording/` muxes the same stream,
  so recording adds no encode step and no device load.
- **Security.** The bridge binds 127.0.0.1 only and requires a per-session 256-bit token and an app
  origin. Tauri IPC is not used for video because it adds per-message overhead at 60 fps.
- **Backpressure.** Sends to the page never block the device socket. If the page falls behind, the
  backend skips to the next key frame and requests one (H.264 cannot drop arbitrary frames).

The scrcpy wire protocol is internal and changes between versions, so the version is pinned in
`android/server.rs` and `scripts/fetch-sidecars.mjs` (a unit test keeps them equal). **Upgrading
scrcpy means re-reading `doc/develop.md` and `app/src/control_msg.c` at the new tag.** v5 changed
the frame header layout relative to earlier versions.

## Module map

| Path | Responsibility |
| --- | --- |
| `src-tauri/src/android/adb.rs` | adb wrapper: timeouts, no shell, output parsers |
| `src-tauri/src/android/server.rs` | Deploy and connect to scrcpy-server |
| `src-tauri/src/android/stream.rs` | Video and audio demuxer |
| `src-tauri/src/android/control.rs` | Typed input events and their wire encoding |
| `src-tauri/src/android/session.rs` | The single active session and every exit path |
| `src-tauri/src/bridge.rs` | Loopback WebSocket to the page |
| `src-tauri/src/recording/` | H.264 helpers, MP4 muxer, file splitting |
| `src-tauri/src/ios/` | Apple backend: usbmuxd, lockdownd, macOS capture |
| `src-tauri/src/usb.rs` | Raw USB enumeration for phones adb and usbmuxd cannot see |
| `src-tauri/src/devices.rs` | Unified device model and background monitor |
| `src-tauri/src/settings.rs` | Validated, atomically saved settings |
| `src-tauri/src/process.rs` | Hidden, timed child processes; Windows job object |
| `src/lib/mirrorClient.ts` | WebCodecs decode, audio playback, canvas rendering |
| `src/lib/input.ts` | Browser input to Android input mapping |
| `src/store.tsx` | App state; subscribes to backend events |

## Device states

Each state comes from a real response. Nothing is simulated.

| State | Source |
| --- | --- |
| Connected | `adb devices` reports `device`, or a trusted Apple device on macOS |
| Unauthorized | adb `unauthorized`, or no usbmuxd pair record for an Apple device |
| Offline | adb `offline`/transitional, or USB-visible but unknown to adb |
| Disconnected | Was present and is now gone (kept listed for 60 seconds) |
| Unsupported | Android below 5.0, or an Apple device on Windows |

## Apple devices

Separate backend, as Apple's rules differ completely from Android's.

- **macOS:** `CoreMediaIO` plus `AVFoundation`, the public path QuickTime Player uses. View-only:
  iOS has no API for a computer to inject input. Audio, recording and control are all reported as
  unavailable rather than offered.
- **Windows:** Apple offers no capture API. Third-party tools rely on reverse-engineered USB
  protocols, and some replace Apple's driver, which breaks Apple's own software. The app detects
  the device and reports the limitation instead.

## Security

- No command accepts shell text. adb runs with discrete arguments, and server options are numeric
  or fixed literals.
- Serials are validated before use. Media commands accept only a bare file name inside the save
  directory, so traversal is rejected.
- UI input is parsed into a closed enum; anything else is dropped, and event contents are not logged.
- Serial numbers are redacted in logs.
- The CSP blocks remote code. Window permissions are limited in `capabilities/default.json`.

## Process lifetime

- One session at a time, enforced by `MirrorManager`, and start and stop are serialized.
- Every exit path runs `stop_locked`, which finalizes the recording *before* any adb call, because
  adb can stall for seconds after a USB loss.
- On Windows a job object kills `adb shell` even if the app crashes. On every platform the device
  server exits when its sockets close.
- The adb daemon is stopped on exit only if this app started it.
