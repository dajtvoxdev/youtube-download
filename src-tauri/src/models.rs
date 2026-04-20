use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoInfo {
    pub url: String,
    pub title: String,
    pub thumbnail: Option<String>,
    pub duration: Option<f64>,
    pub uploader: Option<String>,
    pub max_height: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadOptions {
    pub url: String,
    pub format_id: String,
    pub output_dir: String,
    pub audio_only: bool,
    pub audio_format: String,
    pub filename_template: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEvent {
    pub job_id: String,
    pub status: String,
    pub percent: f64,
    pub speed: String,
    pub eta: String,
    pub title: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadRecord {
    pub id: String,
    pub url: String,
    pub title: Option<String>,
    pub output_path: Option<String>,
    pub format_id: String,
    pub audio_only: bool,
    pub status: String,
    pub error_msg: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

impl DownloadRecord {
    pub fn load_all(app_data_dir: &str) -> anyhow::Result<Vec<Self>> {
        let path = format!("{}/history.json", app_data_dir);
        if !Path::new(&path).exists() {
            return Ok(vec![]);
        }
        let data = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&data).unwrap_or_default())
    }

    pub fn save_all(app_data_dir: &str, records: &[Self]) -> anyhow::Result<()> {
        let path = format!("{}/history.json", app_data_dir);
        std::fs::write(&path, serde_json::to_string_pretty(records)?)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub output_dir: String,
    pub ytdlp_path: String,
    pub max_concurrent: usize,
    pub audio_format: String,
    pub filename_template: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            output_dir: String::new(),
            ytdlp_path: if cfg!(windows) {
                "yt-dlp.exe".to_string()
            } else {
                "yt-dlp".to_string()
            },
            max_concurrent: 2,
            audio_format: "mp3".to_string(),
            filename_template: "%(title)s.%(ext)s".to_string(),
        }
    }
}

impl AppSettings {
    pub fn load(app_data_dir: &str) -> anyhow::Result<Self> {
        let path = format!("{}/settings.json", app_data_dir);
        if !Path::new(&path).exists() {
            return Ok(Self::default());
        }
        let data = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&data).unwrap_or_default())
    }

    pub fn save(&self, app_data_dir: &str) -> anyhow::Result<()> {
        let path = format!("{}/settings.json", app_data_dir);
        std::fs::write(&path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}
