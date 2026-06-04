use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
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

    let mut cmd = Command::new(&ytdlp_path);
    cmd.args([
            "-J",
            "--no-playlist",
            "--no-warnings",
            "--flat-playlist",
            &url,
        ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let output = cmd.output()
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
    let ffmpeg_path = state.ffmpeg_path.lock().unwrap().clone();
    let cookie_source = state.settings.lock().unwrap().cookie_source.clone();

    spawn_download(
        app,
        state.manager.clone(),
        state.history.clone(),
        state.app_data_dir.clone(),
        job_id.clone(),
        opts,
        ytdlp_path,
        ffmpeg_path,
        cookie_source,
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
pub async fn pick_cookie_file(app: AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let result = app.dialog()
        .file()
        .add_filter("Cookies", &["txt"])
        .blocking_pick_file();
    result.map(|p| format!("file:{}", p.to_string()))
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
    let mut cmd = Command::new(&ytdlp_path);
    cmd.arg("--version");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let output = cmd.output()
        .await
        .map_err(|e| format!("yt-dlp not found at '{}': {}", ytdlp_path, e))?;

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[tauri::command]
pub fn get_ffmpeg_path(state: State<'_, AppState>) -> Option<String> {
    state.ffmpeg_path.lock().unwrap().clone()
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

#[derive(Serialize, Clone)]
pub struct YtdlpUpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
}

#[derive(Serialize, Clone)]
pub struct UpdateProgressEvent {
    pub percent: f64,
    pub status: String,
}

async fn get_current_version(ytdlp_path: &str) -> Result<String, String> {
    let mut cmd = Command::new(ytdlp_path);
    cmd.arg("--version");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let output = cmd.output().await.map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[tauri::command]
pub async fn check_ytdlp_update(state: State<'_, AppState>) -> Result<YtdlpUpdateInfo, String> {
    let ytdlp_path = state.settings.lock().unwrap().ytdlp_path.clone();

    let current_version = get_current_version(&ytdlp_path).await.unwrap_or_default();

    let client = reqwest::Client::builder()
        .user_agent("video-download-app")
        .build()
        .map_err(|e| e.to_string())?;

    let resp: serde_json::Value = client
        .get("https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest")
        .send()
        .await
        .map_err(|e| format!("Không thể kiểm tra bản cập nhật: {}", e))?
        .json()
        .await
        .map_err(|e| format!("Lỗi parse response: {}", e))?;

    let latest_version = resp["tag_name"]
        .as_str()
        .unwrap_or("unknown")
        .trim_start_matches('v')
        .to_string();

    let update_available = if current_version.is_empty() || latest_version == "unknown" {
        false
    } else {
        current_version != latest_version
    };

    Ok(YtdlpUpdateInfo {
        current_version,
        latest_version,
        update_available,
    })
}

#[tauri::command]
pub async fn update_ytdlp(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let ytdlp_path = state.settings.lock().unwrap().ytdlp_path.clone();

    let emit_progress = |pct: f64, status: &str| {
        let _ = app.emit("ytdlp-update-progress", UpdateProgressEvent {
            percent: pct,
            status: status.to_string(),
        });
    };

    emit_progress(0.0, "Đang kiểm tra bản mới nhất...");

    let client = reqwest::Client::builder()
        .user_agent("video-download-app")
        .build()
        .map_err(|e| e.to_string())?;

    let resp: serde_json::Value = client
        .get("https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest")
        .send()
        .await
        .map_err(|e| format!("Không thể kiểm tra bản cập nhật: {}", e))?
        .json()
        .await
        .map_err(|e| format!("Lỗi parse response: {}", e))?;

    let latest_version = resp["tag_name"]
        .as_str()
        .unwrap_or("unknown")
        .trim_start_matches('v')
        .to_string();

    let download_url = if cfg!(target_os = "windows") {
        format!("https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe")
    } else if cfg!(target_os = "macos") {
        format!("https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos")
    } else {
        format!("https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp")
    };

    emit_progress(10.0, &format!("Đang tải yt-dlp {}...", latest_version));

    let response = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| format!("Không thể tải bản cập nhật: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Lỗi tải file: HTTP {}", response.status()));
    }

    let total_size = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    let target_path = std::path::PathBuf::from(&ytdlp_path);
    let temp_path = target_path.with_extension({
        let ext = target_path.extension().map(|e| e.to_string_lossy().to_string());
        match ext {
            Some(e) => format!("{}.tmp", e),
            None => "tmp".to_string(),
        }
    });

    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::File::create(&temp_path)
        .await
        .map_err(|e| format!("Không thể tạo file tạm: {}", e))?;

    use futures::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Lỗi tải dữ liệu: {}", e))?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        if total_size > 0 {
            let pct = 10.0 + (downloaded as f64 / total_size as f64) * 80.0;
            emit_progress(pct, &format!("Đang tải... {:.0}%", (downloaded as f64 / total_size as f64) * 100.0));
        }
    }
    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);

    emit_progress(90.0, "Đang cài đặt...");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&temp_path, std::fs::Permissions::from_mode(0o755));
    }

    std::fs::rename(&temp_path, &target_path)
        .or_else(|_| {
            std::fs::copy(&temp_path, &target_path)?;
            let _ = std::fs::remove_file(&temp_path);
            Ok(())
        })
        .map_err(|e| format!("Không thể thay thế file: {}. Hãy đóng mọi tiến trình yt-dlp đang chạy.", e))?;

    emit_progress(100.0, "Hoàn tất!");
    Ok(format!("Đã cập nhật yt-dlp lên phiên bản {}", latest_version))
}
