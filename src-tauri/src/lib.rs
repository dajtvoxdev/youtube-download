mod commands;
mod downloader;
mod models;

use std::sync::{Arc, Mutex};
use tauri::Manager;
use downloader::DownloadManager;
use models::{AppSettings, DownloadRecord};

pub struct AppState {
    pub manager: Arc<Mutex<DownloadManager>>,
    pub settings: Arc<Mutex<AppSettings>>,
    pub history: Arc<Mutex<Vec<DownloadRecord>>>,
    pub app_data_dir: String,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to resolve app data dir")
                .to_string_lossy()
                .to_string();

            std::fs::create_dir_all(&app_data_dir).ok();

            let mut settings = AppSettings::load(&app_data_dir).unwrap_or_default();

            // If ytdlp_path is still the default placeholder, try to resolve the bundled sidecar
            if settings.ytdlp_path == "yt-dlp.exe" || settings.ytdlp_path == "yt-dlp" {
                if let Ok(resource_path) = app.path().resource_dir() {
                    let sidecar_name = if cfg!(windows) {
                        "yt-dlp.exe"
                    } else {
                        "yt-dlp"
                    };
                    // Tauri places sidecar binaries next to the executable
                    let exe_dir = std::env::current_exe()
                        .ok()
                        .and_then(|p| p.parent().map(|d| d.to_path_buf()));
                    let candidates = [
                        exe_dir.as_ref().map(|d| d.join(sidecar_name)),
                        Some(resource_path.join(sidecar_name)),
                    ];
                    for candidate in candidates.iter().flatten() {
                        if candidate.exists() {
                            settings.ytdlp_path = candidate.to_string_lossy().to_string();
                            break;
                        }
                    }
                }
            }

            if settings.output_dir.is_empty() {
                let default_dir: String = app
                    .path()
                    .download_dir()
                    .or_else(|_| app.path().home_dir())
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| ".".to_string());
                settings.output_dir = default_dir;
            }

            let max_concurrent = settings.max_concurrent;
            let history = DownloadRecord::load_all(&app_data_dir).unwrap_or_default();

            app.manage(AppState {
                manager: Arc::new(Mutex::new(DownloadManager::new(max_concurrent))),
                settings: Arc::new(Mutex::new(settings)),
                history: Arc::new(Mutex::new(history)),
                app_data_dir,
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::probe_formats,
            commands::start_download,
            commands::cancel_download,
            commands::get_settings,
            commands::save_settings,
            commands::pick_output_dir,
            commands::get_history,
            commands::clear_history,
            commands::check_ytdlp,
            commands::open_file,
            commands::file_exists,
            commands::open_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
