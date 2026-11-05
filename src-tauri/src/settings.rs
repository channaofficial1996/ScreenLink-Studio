//! Persistent user settings. Values are normalized on load and on save, so a hand-edited or
//! outdated file can never push an invalid option through to the device.

use crate::android::server::ServerOptions;
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const ALLOWED_RESOLUTIONS: [u32; 6] = [0, 720, 1080, 1280, 1600, 1920];
const ALLOWED_FPS: [u32; 5] = [15, 24, 30, 60, 90];
const MIN_BITRATE_MBPS: u32 = 1;
const MAX_BITRATE_MBPS: u32 = 40;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Longest edge in pixels; 0 means the device's native resolution.
    pub resolution: u32,
    pub fps: u32,
    pub bitrate_mbps: u32,
    pub audio_forwarding: bool,
    pub always_on_top: bool,
    pub launch_at_login: bool,
    pub start_minimized: bool,
    pub auto_connect: bool,
    pub save_directory: String,
    pub language: String,
    pub theme: String,
    pub stay_awake: bool,
    pub show_touches: bool,
    pub auto_check_updates: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            resolution: 1920,
            fps: 60,
            bitrate_mbps: 8,
            audio_forwarding: true,
            always_on_top: false,
            launch_at_login: false,
            start_minimized: false,
            auto_connect: false,
            save_directory: String::new(),
            language: "en".into(),
            theme: "dark".into(),
            stay_awake: true,
            show_touches: false,
            auto_check_updates: true,
        }
    }
}

impl Settings {
    pub fn normalized(mut self, default_dir: &Path) -> Self {
        if !ALLOWED_RESOLUTIONS.contains(&self.resolution) {
            self.resolution = Self::default().resolution;
        }
        if !ALLOWED_FPS.contains(&self.fps) {
            self.fps = Self::default().fps;
        }
        self.bitrate_mbps = self.bitrate_mbps.clamp(MIN_BITRATE_MBPS, MAX_BITRATE_MBPS);
        if !matches!(self.language.as_str(), "en" | "es" | "fr") {
            self.language = "en".into();
        }
        if !matches!(self.theme.as_str(), "dark" | "light" | "system") {
            self.theme = "dark".into();
        }
        if self.save_directory.is_empty() || !Path::new(&self.save_directory).is_absolute() {
            self.save_directory = default_dir.display().to_string();
        }
        self
    }

    pub fn server_options(&self) -> ServerOptions {
        ServerOptions {
            max_size: self.resolution,
            max_fps: self.fps,
            video_bit_rate: self.bitrate_mbps * 1_000_000,
            audio: self.audio_forwarding,
            stay_awake: self.stay_awake,
            show_touches: self.show_touches,
        }
    }

    pub fn save_dir(&self) -> PathBuf {
        PathBuf::from(&self.save_directory)
    }
}

pub fn load(path: &Path, default_dir: &Path) -> Settings {
    let stored = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| match serde_json::from_str::<Settings>(&text) {
            Ok(s) => Some(s),
            Err(e) => {
                log::warn!("settings file is invalid, using defaults: {e}");
                None
            }
        })
        .unwrap_or_default();
    stored.normalized(default_dir)
}

/// Write-then-rename, so a crash mid-save cannot leave a truncated settings file.
pub fn save(path: &Path, settings: &Settings) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec_pretty(settings)
        .map_err(|e| AppError::InvalidInput(format!("Could not encode settings: {e}")))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> PathBuf {
        std::env::temp_dir().join(format!("slstudio-settings-{}", rand::random::<u64>()))
    }

    #[test]
    fn normalization_rejects_invalid_values() {
        let s = Settings {
            resolution: 12345,
            fps: 1000,
            bitrate_mbps: 500,
            language: "xx".into(),
            theme: "neon".into(),
            save_directory: "relative/path".into(),
            ..Settings::default()
        }
        .normalized(Path::new("C:\\Media"));
        assert_eq!(s.resolution, 1920);
        assert_eq!(s.fps, 60);
        assert_eq!(s.bitrate_mbps, 40);
        assert_eq!(s.language, "en");
        assert_eq!(s.theme, "dark");
        assert_eq!(s.save_directory, "C:\\Media");
    }

    #[test]
    fn round_trips_through_disk() {
        let d = dir();
        let path = d.join("settings.json");
        let s = Settings { fps: 30, audio_forwarding: false, ..Settings::default() }
            .normalized(&d);
        save(&path, &s).unwrap();
        assert_eq!(load(&path, &d), s);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults() {
        let d = dir();
        std::fs::create_dir_all(&d).unwrap();
        let path = d.join("settings.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(load(&path, &d).fps, 60);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn missing_fields_take_defaults() {
        let d = dir();
        std::fs::create_dir_all(&d).unwrap();
        let path = d.join("settings.json");
        std::fs::write(&path, r#"{"fps": 30}"#).unwrap();
        let s = load(&path, &d);
        assert_eq!(s.fps, 30);
        assert_eq!(s.bitrate_mbps, 8);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn maps_to_server_options() {
        let o = Settings { resolution: 1080, fps: 30, bitrate_mbps: 4, ..Settings::default() }
            .server_options();
        assert_eq!(o.max_size, 1080);
        assert_eq!(o.max_fps, 30);
        assert_eq!(o.video_bit_rate, 4_000_000);
    }
}
