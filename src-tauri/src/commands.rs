use tauri::{AppHandle, State};
use tokio::process::Command;
use crate::models::{AppSettings, DownloadOptions, DownloadRecord, VideoInfo};
use crate::downloader::spawn_download;
use crate::AppState;

#[tauri::command]
pub async fn probe_formats(
    state: State<'_, AppState>,
    url: String,
) -> Result<VideoInfo, String> {
    let ytdlp_path = state.settings.lock().unwrap().ytdlp_path.clone();

    let output = Command::new(&ytdlp_path)
        .args([
            "-J",
            "--no-playlist",
            "--no-warnings",
            "--flat-playlist",
            &url,
        ])
        .output()
        .await
        .map_err(|e| format!("Failed to run yt-dlp: {}. Is it installed?", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if err.is_empty() { "Failed to fetch video info".to_string() } else { err });
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse yt-dlp output: {}", e))?;

    let title = json["title"].as_str().unwrap_or("Unknown Title").to_string();
    let thumbnail = json["thumbnail"].as_str().map(|s| s.to_string());
    let duration = json["duration"].as_f64();
    let uploader = json["uploader"].as_str()
        .or_else(|| json["channel"].as_str())
        .map(|s| s.to_string());

    let max_height = json["formats"].as_array()
        .map(|formats| {
            formats.iter()
                .filter_map(|f| f["height"].as_i64())
                .filter(|&h| h > 0)
                .max()
        })
        .flatten()
        .or_else(|| json["height"].as_i64());

    Ok(VideoInfo {
        url,
        title,
        thumbnail,
        duration,
        uploader,
        max_height,
    })
}

#[tauri::command]
pub async fn start_download(
    app: AppHandle,
    state: State<'_, AppState>,
    opts: DownloadOptions,
) -> Result<String, String> {
    let job_id = uuid::Uuid::new_v4().to_string();
    let ytdlp_path = state.settings.lock().unwrap().ytdlp_path.clone();

    spawn_download(
        app,
        state.manager.clone(),
        state.history.clone(),
        state.app_data_dir.clone(),
        job_id.clone(),
        opts,
        ytdlp_path,
    );

    Ok(job_id)
}

#[tauri::command]
pub fn cancel_download(state: State<'_, AppState>, job_id: String) -> bool {
    state.manager.lock().unwrap().cancel(&job_id)
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> AppSettings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: AppSettings) -> Result<(), String> {
    settings.save(&state.app_data_dir).map_err(|e| e.to_string())?;

    let max_concurrent = settings.max_concurrent;
    let mut mgr = state.manager.lock().unwrap();
    mgr.update_concurrency(max_concurrent);

    *state.settings.lock().unwrap() = settings;
    Ok(())
}

#[tauri::command]
pub async fn pick_output_dir(app: AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    app.dialog()
        .file()
        .blocking_pick_folder()
        .map(|p| p.to_string())
}

#[tauri::command]
pub fn get_history(state: State<'_, AppState>) -> Vec<DownloadRecord> {
    state.history.lock().unwrap().clone()
}

#[tauri::command]
pub fn clear_history(state: State<'_, AppState>) -> Result<(), String> {
    let mut hist = state.history.lock().unwrap();
    hist.clear();
    DownloadRecord::save_all(&state.app_data_dir, &hist).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn check_ytdlp(ytdlp_path: String) -> Result<String, String> {
    let output = Command::new(&ytdlp_path)
        .arg("--version")
        .output()
        .await
        .map_err(|e| format!("yt-dlp not found at '{}': {}", ytdlp_path, e))?;

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[tauri::command]
pub fn open_file(app: AppHandle, path: String) -> Result<(), String> {
    use std::path::Path;
    if !Path::new(&path).exists() {
        return Err("File không còn tồn tại hoặc đã bị xoá".to_string());
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(&path, None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn file_exists(path: String) -> bool {
    std::path::Path::new(&path).exists()
}

#[tauri::command]
pub async fn open_folder(path: String) -> Result<(), String> {
    use std::path::Path;
    let folder = if Path::new(&path).is_file() {
        Path::new(&path).parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or(path)
    } else {
        path
    };

    #[cfg(target_os = "windows")]
    { std::process::Command::new("explorer").arg(&folder).spawn().ok(); }
    #[cfg(target_os = "macos")]
    { std::process::Command::new("open").arg(&folder).spawn().ok(); }
    #[cfg(target_os = "linux")]
    { std::process::Command::new("xdg-open").arg(&folder).spawn().ok(); }

    Ok(())
}
