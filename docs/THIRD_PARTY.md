# Third-party components

## Bundled with the application

### scrcpy-server

Copyright (C) 2018 Genymobile; Copyright (C) 2018-2026 Romain Vimont.
Apache License 2.0. <https://github.com/Genymobile/scrcpy>

The Apache License requires shipping a copy of the license and the attribution notices with
redistributed binaries.

### Android SDK Platform-Tools (adb, AdbWinApi.dll, AdbWinUsbApi.dll)

Copyright Google LLC. Distributed under the Android Software Development Kit License Agreement.
`platform-tools-NOTICE.txt` ships beside the binaries.

**Check this before a commercial release.** The SDK license restricts redistribution of SDK
components. Many tools, scrcpy's own Windows release among them, bundle adb, but that is common
practice rather than a grant of rights. Have counsel review it, or change the installer to
download platform-tools on first run. `Adb::locate` already falls back to `ANDROID_HOME` and `PATH`.

## Libraries

Rust crates and npm packages are listed with their licenses in `src-tauri/Cargo.lock` and
`package-lock.json`. Main ones: Tauri (MIT/Apache-2.0), React (MIT), Lucide (ISC), nusb
(MIT/Apache-2.0), tungstenite (MIT/Apache-2.0), plist (MIT), objc2 (MIT).

Fonts: Manrope and JetBrains Mono (SIL Open Font License 1.1).

## Trademarks

Android is a trademark of Google LLC. iPhone, iPad and macOS are trademarks of Apple Inc. ScreenLink
Studio is not affiliated with or endorsed by Google, Apple or Genymobile.
