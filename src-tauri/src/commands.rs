//! The IPC surface. Inputs are validated here; no command accepts anything that becomes a shell
//! command or an arbitrary path.

use crate::android::adb::Adb;
use crate::android::server::SCRCPY_VERSION;
use crate::android::session::{MirrorManager, MirrorStatus, RecordingStatus};
use crate::devices::{DeviceMonitor, DeviceSnapshot};
use crate::error::{AppError, AppResult};
use crate::settings::{self, Settings};
use crate::validate::{file_in_dir, validate_serial};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_opener::OpenerExt;

pub struct AppState {
    pub adb: Option<Arc<Adb>>,
    pub adb_error: Option<String>,
    pub settings: Arc<Mutex<Settings>>,
    pub settings_path: PathBuf,
    pub default_save_dir: PathBuf,
    pub server_jar: PathBuf,
    pub mirror: Option<Arc<MirrorManager>>,
    pub monitor: Arc<DeviceMonitor>,
    #[cfg(target_os = "macos")]
    pub ios_capture: Arc<crate::ios::capture_macos::IosCapture>,
}

impl AppState {
    fn adb(&self) -> AppResult<&Arc<Adb>> {
        self.adb.as_ref().ok_or_else(|| {
            AppError::AdbUnavailable(self.adb_error.clone().unwrap_or_else(|| "adb was not found.".into()))
        })
    }

    fn mirror(&self) -> AppResult<&Arc<MirrorManager>> {
        self.adb()?;
        self.mirror.as_ref().ok_or(AppError::NotMirroring)
    }

    fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }
}

/// Run blocking work (adb, sockets, disk) away from the async runtime's worker threads.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> AppResult<T> + Send + 'static) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| AppError::Mirror(format!("Internal task failed: {e}")))?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: String,
    platform: &'static str,
    arch: &'static str,
    scrcpy_version: &'static str,
    adb_version: Option<String>,
    adb_path: Option<String>,
    ios_can_mirror: bool,
    log_dir: Option<String>,
}

#[tauri::command]
pub async fn get_app_info(app: AppHandle, state: State<'_, AppState>) -> AppResult<AppInfo> {
    let adb = state.adb.clone();
    let version = app.package_info().version.to_string();
    let log_dir = app.path().app_log_dir().ok().map(|p| p.display().to_string());
    blocking(move || {
        Ok(AppInfo {
            version,
            platform: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            scrcpy_version: SCRCPY_VERSION,
            adb_version: adb.as_ref().and_then(|a| a.version()),
            adb_path: adb.as_ref().map(|a| a.path().display().to_string()),
            ios_can_mirror: crate::ios::CAN_MIRROR,
            log_dir,
        })
    })
    .await
}

#[tauri::command]
pub fn get_devices(state: State<'_, AppState>) -> DeviceSnapshot {
    state.monitor.snapshot()
}

#[tauri::command]
pub fn refresh_devices(state: State<'_, AppState>) {
    state.monitor.request_scan();
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings()
}

#[tauri::command]
pub fn update_settings(app: AppHandle, state: State<'_, AppState>, settings: Settings) -> AppResult<Settings> {
    let next = settings.normalized(&state.default_save_dir);
    std::fs::create_dir_all(next.save_dir())?;
    settings::save(&state.settings_path, &next)?;
    apply_settings(&app, &next);
    *state.settings.lock().unwrap() = next.clone();
    let _ = app.emit("settings-changed", &next);
    Ok(next)
}

/// Apply settings that take effect on the OS rather than on the next mirroring session.
pub fn apply_settings(app: &AppHandle, s: &Settings) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_always_on_top(s.always_on_top);
    }
    let autostart = app.autolaunch();
    if autostart.is_enabled().unwrap_or(false) != s.launch_at_login {
        let result = if s.launch_at_login { autostart.enable() } else { autostart.disable() };
        if let Err(e) = result {
            log::warn!("could not change launch-at-login: {e}");
        }
    }
    let _ = app.asset_protocol_scope().allow_directory(s.save_dir(), false);
}

#[tauri::command]
pub async fn mirror_start(state: State<'_, AppState>, serial: String) -> AppResult<MirrorStatus> {
    validate_serial(&serial)?;
    let mirror = state.mirror()?.clone();
    let options = state.settings().server_options();
    let jar = state.server_jar.clone();
    blocking(move || mirror.start(&serial, &jar, options)).await
}

#[tauri::command]
pub async fn mirror_stop(state: State<'_, AppState>) -> AppResult<()> {
    let mirror = state.mirror()?.clone();
    blocking(move || {
        mirror.stop_active();
        Ok(())
    })
    .await
}

/// Apple devices share the Android preview pipeline but have their own capture backend.
#[tauri::command]
pub async fn ios_mirror_start(app: AppHandle, state: State<'_, AppState>, udid: String) -> AppResult<MirrorStatus> {
    validate_serial(&udid)?;
    #[cfg(target_os = "macos")]
    {
        let capture = state.ios_capture.clone();
        blocking(move || capture.start(&app, &udid)).await
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, state);
        Err(AppError::Unsupported(
            "Apple does not provide a screen capture API on Windows.".into(),
        ))
    }
}

#[tauri::command]
pub async fn ios_mirror_stop(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    #[cfg(target_os = "macos")]
    state.ios_capture.stop(&app);
    #[cfg(not(target_os = "macos"))]
    let _ = (app, state);
    Ok(())
}

/// The installer cannot replace adb.exe while its daemon runs from the install folder, so this
/// ends the session and stops the daemon even if an earlier instance of the app started it.
#[tauri::command]
pub async fn prepare_update(state: State<'_, AppState>) -> AppResult<()> {
    let (mirror, adb) = (state.mirror.clone(), state.adb.clone());
    blocking(move || {
        if let Some(mirror) = mirror {
            mirror.stop_active();
        }
        if let Some(adb) = adb {
            adb.stop_server();
        }
        Ok(())
    })
    .await
}

#[tauri::command]
pub fn mirror_status(state: State<'_, AppState>) -> AppResult<MirrorStatus> {
    Ok(state.mirror()?.status())
}

fn timestamped(prefix: &str) -> String {
    format!("{prefix} {}", chrono::Local::now().format("%Y-%m-%d at %H.%M.%S"))
}

#[tauri::command]
pub async fn take_screenshot(state: State<'_, AppState>, serial: String) -> AppResult<String> {
    validate_serial(&serial)?;
    let adb = state.adb()?.clone();
    let dir = state.settings().save_dir();
    blocking(move || {
        let png = adb.screenshot(&serial)?;
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.png", timestamped("Screenshot")));
        std::fs::write(&path, png)?;
        Ok(path.display().to_string())
    })
    .await
}

/// Save a PNG captured from the on-screen preview (Apple devices, which have no screencap tool).
#[tauri::command]
pub async fn save_preview_png(state: State<'_, AppState>, png: Vec<u8>) -> AppResult<String> {
    const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if !png.starts_with(PNG_SIGNATURE) {
        return Err(AppError::InvalidInput("Not a PNG image.".into()));
    }
    let dir = state.settings().save_dir();
    blocking(move || {
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.png", timestamped("Screenshot")));
        std::fs::write(&path, png)?;
        Ok(path.display().to_string())
    })
    .await
}

#[tauri::command]
pub async fn recording_start(state: State<'_, AppState>) -> AppResult<RecordingStatus> {
    let mirror = state.mirror()?.clone();
    let settings = state.settings();
    blocking(move || {
        mirror.start_recording(&settings.save_dir(), timestamped("Recording"), settings.audio_forwarding)
    })
    .await
}

#[tauri::command]
pub async fn recording_stop(state: State<'_, AppState>) -> AppResult<RecordingStatus> {
    let mirror = state.mirror()?.clone();
    blocking(move || mirror.stop_recording()).await
}

#[tauri::command]
pub fn recording_status(state: State<'_, AppState>) -> AppResult<RecordingStatus> {
    Ok(state.mirror()?.recording_status())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaItem {
    name: String,
    path: String,
    kind: &'static str,
    bytes: u64,
    modified_ms: u64,
}

fn media_kind(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some("screenshot"),
        "mp4" => Some("recording"),
        _ => None,
    }
}

#[tauri::command]
pub async fn list_media(state: State<'_, AppState>) -> AppResult<Vec<MediaItem>> {
    let dir = state.settings().save_dir();
    blocking(move || {
        let Ok(entries) = std::fs::read_dir(&dir) else { return Ok(Vec::new()) };
        let mut items: Vec<MediaItem> = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let kind = media_kind(&path)?;
                let meta = entry.metadata().ok()?;
                let modified_ms = meta
                    .modified()
                    .ok()?
                    .duration_since(UNIX_EPOCH)
                    .ok()?
                    .as_millis() as u64;
                Some(MediaItem {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    path: path.display().to_string(),
                    kind,
                    bytes: meta.len(),
                    modified_ms,
                })
            })
            .collect();
        items.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms));
        Ok(items)
    })
    .await
}

fn media_path(state: &State<'_, AppState>, name: &str) -> AppResult<PathBuf> {
    let path = file_in_dir(&state.settings().save_dir(), name)?;
    if media_kind(&path).is_none() || !path.is_file() {
        return Err(AppError::InvalidInput("That file is not in your media library.".into()));
    }
    Ok(path)
}

#[tauri::command]
pub fn delete_media(state: State<'_, AppState>, name: String) -> AppResult<()> {
    std::fs::remove_file(media_path(&state, &name)?)?;
    Ok(())
}

#[tauri::command]
pub fn reveal_media(app: AppHandle, state: State<'_, AppState>, name: String) -> AppResult<()> {
    let path = media_path(&state, &name)?;
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|e| AppError::Unsupported(e.to_string()))
}

#[tauri::command]
pub fn open_media(app: AppHandle, state: State<'_, AppState>, name: String) -> AppResult<()> {
    let path = media_path(&state, &name)?;
    app.opener()
        .open_path(path.display().to_string(), None::<&str>)
        .map_err(|e| AppError::Unsupported(e.to_string()))
}

#[tauri::command]
pub fn open_save_directory(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let dir = state.settings().save_dir();
    std::fs::create_dir_all(&dir)?;
    app.opener()
        .open_path(dir.display().to_string(), None::<&str>)
        .map_err(|e| AppError::Unsupported(e.to_string()))
}
