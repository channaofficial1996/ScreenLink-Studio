# ScreenLink Studio

Mirror a phone or tablet to your computer over USB, right inside a desktop app, with control,
recording and screenshots.

| Capability | Android | iPhone / iPad |
| --- | --- | --- |
| Detection, status, trust | Windows, macOS | Windows, macOS |
| Screen mirroring in the app | Windows, macOS | macOS only (view-only) |
| Mouse and keyboard control | Yes | No: iOS does not allow it |
| Audio forwarding | Android 11+ | No |
| Screen recording to MP4 | Yes | Not yet |
| PNG screenshots | Yes | macOS, while mirroring |

Apple provides no way to capture an iPhone screen on Windows, so iPhones and iPads there are
detected but cannot be mirrored. See [docs/TESTING.md](docs/TESTING.md) for what has and has not
been verified, including the macOS and iPhone paths, which have **not yet been run**.

## Install

Download the latest installer from the
[Releases page](https://github.com/channaofficial1996/ScreenLink-Studio/releases/latest).
The app adds Desktop and Start Menu shortcuts, and updates itself from there. To build it yourself,
see [docs/BUILDING.md](docs/BUILDING.md).

- **Windows 10/11:** run `ScreenLink Studio_1.0.0_x64-setup.exe` or the `.msi`.
- **macOS 12+:** open the `.dmg` and drag the app to Applications.

Builds are not code signed yet, so Windows SmartScreen will warn (More info > Run anyway), and on
macOS you need to right-click the app and choose Open.

## Connect an Android device

1. On the phone, open **Settings > About phone** and tap **Build number** seven times.
2. Open **Settings > System > Developer options** and turn on **USB debugging**.
3. Plug in the USB cable, unlock the phone, and tap **Allow** on the prompt.
4. In ScreenLink Studio, click **Start mirroring**.

Controls: click to tap, drag to swipe, scroll to scroll, right click for Back, middle click for
Home. Click the screen and type to use the keyboard, and Ctrl+V pastes.

Some Xiaomi and Oppo phones need an extra Developer option, such as "USB debugging (Security
settings)", before touch input works.

## Phone Only mode, for recording lessons

Press **F10**, or click **Phone Only**, while mirroring. The window becomes a borderless panel the
shape of the phone, docked to the side of your desktop and kept on top, so your screen recording
shows only the phone.

Move the mouse to the top edge of the panel to reveal its controls: phone keys, dock left or right,
keep on top, and exit. Drag that strip to move the panel, drag a window edge to resize it, and press
**F10** to return. The panel buttons at the top right of the normal window hide the menu and the
device panel individually.

## Connect an iPhone or iPad (macOS)

1. Plug in the cable, unlock the device, and tap **Trust**.
2. Click **Start mirroring** and allow camera access when macOS asks. macOS treats a connected
   iPhone as a camera, so that permission is what grants access to its screen.

## Troubleshooting

| Symptom | Fix |
| --- | --- |
| Unauthorized | Unlock the phone and tap Allow. If no prompt appears, reconnect the cable. |
| Offline or not listed | Use a data cable and another USB port, and set USB mode to File transfer. |
| Phone listed but "not responding to adb" | Check USB debugging is on, then reconnect the cable. |
| Black areas in the preview | Banking apps and DRM video block all capture on Android. |
| No audio | Requires Android 11+. Some apps also disallow capture. |
| A new file appears mid-recording | Rotating the phone changes the video size, so the recording continues in `(part 2)`. |
| Rotate button does nothing | The current app is portrait-only, such as many launchers. The button also turns off the phone's auto-rotate. |

The Help & Support page in the app has the same guidance, and log files are in the folder it names.

## Documentation

- [Architecture](docs/ARCHITECTURE.md): how embedded mirroring works, and security design
- [Building](docs/BUILDING.md): setup, tests, packaging, signing
- [Testing](docs/TESTING.md): what was verified, and how
- [Releasing](docs/RELEASING.md): publishing updates and the signing key
- [Third-party components](docs/THIRD_PARTY.md): licenses and redistribution notes
