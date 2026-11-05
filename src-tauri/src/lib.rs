//! ScreenLink Studio backend. See docs/ARCHITECTURE.md for the overall design.

mod android;
mod bridge;
mod commands;
mod devices;
mod error;
mod ios;
mod phone_mode;
mod process;
mod recording;
mod settings;
mod usb;
mod validate;

use android::adb::Adb;
use android::session::MirrorManager;
use commands::AppState;
use devices::DeviceMonitor;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{Manager, RunEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        // Must be first so a second launch focuses this window before anything else starts.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .max_file_size(2_000_000)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            setup(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::get_devices,
            commands::refresh_devices,
            commands::get_settings,
            commands::update_settings,
            commands::mirror_start,
            commands::mirror_stop,
            commands::mirror_status,
            commands::prepare_update,
            phone_mode::phone_mode_enter,
            phone_mode::phone_mode_fit,
            phone_mode::phone_mode_pin,
            phone_mode::phone_mode_exit,
            commands::ios_mirror_start,
            commands::ios_mirror_stop,
            commands::take_screenshot,
            commands::save_preview_png,
            commands::recording_start,
            commands::recording_stop,
            commands::recording_status,
            commands::list_media,
            commands::delete_media,
            commands::reveal_media,
            commands::open_media,
            commands::open_save_directory,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build the application");

    app.run(|handle, event| {
        if matches!(event, RunEvent::Exit) {
            let state = handle.state::<AppState>();
            if let Some(mirror) = &state.mirror {
                mirror.stop_active();
            }
            #[cfg(target_os = "macos")]
            state.ios_capture.stop(handle);
            if let Some(adb) = &state.adb {
                adb.stop_server_if_owned();
            }
        }
    });
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    let path = app.path();
    let resource_dir = path.resource_dir().ok();

    let default_save_dir = path
        .video_dir()
        .or_else(|_| path.document_dir())
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("ScreenLink Studio");
    let settings_path = path.app_config_dir()?.join("settings.json");
    let loaded = settings::load(&settings_path, &default_save_dir);

    let (adb, adb_error) = match Adb::locate(resource_dir.as_deref()) {
        Ok(adb) => (Some(Arc::new(adb)), None),
        Err(e) => {
            log::error!("{e}");
            (None, Some(e.to_string()))
        }
    };

    let server_jar = resource_dir
        .as_ref()
        .map(|d| d.join("resources").join("scrcpy-server"))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources").join("scrcpy-server")
        });

    let mirror = adb.as_ref().map(|a| MirrorManager::new(handle.clone(), a.clone()));
    let monitor = DeviceMonitor::new();
    let settings = Arc::new(Mutex::new(loaded.clone()));

    let auto_connect = {
        let (mirror, settings, jar) = (mirror.clone(), settings.clone(), server_jar.clone());
        Arc::new(move |serial: &str| {
            let Some(mirror) = mirror.clone() else { return };
            let options = settings.lock().unwrap().server_options();
            let (serial, jar) = (serial.to_string(), jar.clone());
            std::thread::spawn(move || {
                if let Err(e) = mirror.start(&serial, &jar, options) {
                    log::warn!("auto-connect failed: {e}");
                }
            });
        })
    };
    monitor.spawn(handle.clone(), adb.clone(), mirror.clone(), settings.clone(), auto_connect);

    app.manage(phone_mode::PhoneMode::default());
    app.manage(AppState {
        adb,
        adb_error,
        settings,
        settings_path,
        default_save_dir,
        server_jar,
        mirror,
        monitor,
        #[cfg(target_os = "macos")]
        ios_capture: Default::default(),
    });

    commands::apply_settings(&handle, &loaded);

    // The window starts hidden in tauri.conf.json to avoid a blank flash.
    if let Some(window) = handle.get_webview_window("main") {
        window.show()?;
        if loaded.start_minimized {
            window.minimize()?;
        }
    }
    Ok(())
}
