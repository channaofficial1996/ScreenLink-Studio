# Testing status

Tested on Windows 11 with a moto g54 (Android 14) connected by USB, in the running app, driving its
real controls and checking the effect on the phone.

## Verified on real hardware

| Feature | How it was checked |
| --- | --- |
| Device detection and unauthorized state | Real `adb` output, before and after approving the prompt |
| Model, Android version, serial | Live adb values shown in the app |
| Embedded preview | 864×1920 pixels decoded into the app's canvas, up to 60 FPS |
| Mouse | Dragging on the canvas pulled down the phone's quick settings |
| Keyboard and paste | Typed and pasted into the phone's Settings search box |
| Device keys | Home and Notifications buttons |
| Audio stream | Opus received and written to the recording; playback was not listened to |
| Screenshot | 1080×2400 PNG saved |
| Recording | H.264 and Opus MP4, read by ffprobe and fully decoded by ffmpeg |
| Rotation | Preview resized live; recording split into three correct files |
| Stop and restart | New session each time; no server or forward left behind |
| Double start | One server and one forward |
| USB loss (simulated with `adb kill-server`) | Recording finalized in 300 ms, UI idle in under 1 s, message shown |
| Close while mirroring | No processes or forwards left on the phone |
| Full screen | Fills the display and returns to live video |
| Settings | Saved to disk |
| Bad input | Malicious serial and path traversal both rejected |
| Windows installer | Installed; Desktop and Start Menu shortcuts and uninstall entry present |

Automated: 65 Rust unit tests, 12 frontend tests, and hardware tests
(`cargo test --lib hardware -- --ignored`).

## Not verified

| Item | Status |
| --- | --- |
| **macOS app** | Type-checked for Apple Silicon and Intel on Windows with a stubbed C compiler. Never linked or run. |
| **iPhone capture on macOS** | Written against public APIs, never run. Needs a Mac and an iPhone. |
| **iPhone detection and trust** | usbmuxd and lockdownd parsers are tested against fake responses, not a real device. |
| Physical cable unplug | Only simulated. Real behavior should match but has not been observed. |
| Automatic adb recovery | After an external `adb kill-server` the phone can stay invisible to adb until adb restarts. |
| Multiple Android devices at once | Parser tested; only one phone was available. |
| Light theme, Spanish, French | Applied without error, not reviewed in full. Translations have not been checked by native speakers. |
| Latency | Not measured glass to glass. |
| Launch at login, always on top, start minimized | Wired up but not exercised through a reboot. |
| MSI installer | Built, not installed. |
| Windows 10 | Not tested. |
