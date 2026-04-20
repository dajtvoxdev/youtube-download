use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::watch;
use tauri::{AppHandle, Emitter};
use regex::Regex;
use chrono::Utc;
use crate::models::{DownloadOptions, DownloadRecord, ProgressEvent};

pub struct DownloadHandle {
    pub cancel_tx: watch::Sender<bool>,
    pub title: Option<String>,
}

pub struct DownloadManager {
    pub handles: HashMap<String, DownloadHandle>,
    pub semaphore: Arc<tokio::sync::Semaphore>,
}

impl DownloadManager {
    pub fn new(max_concurrent: usize) -> Self {
        DownloadManager {
            handles: HashMap::new(),
            semaphore: Arc::new(tokio::sync::Semaphore::new(max_concurrent)),
        }
    }

    pub fn cancel(&self, job_id: &str) -> bool {
        if let Some(handle) = self.handles.get(job_id) {
            let _ = handle.cancel_tx.send(true);
            true
        } else {
            false
        }
    }

    pub fn cancel_all(&self) {
        for handle in self.handles.values() {
            let _ = handle.cancel_tx.send(true);
        }
    }

    pub fn update_concurrency(&mut self, max: usize) {
        self.semaphore = Arc::new(tokio::sync::Semaphore::new(max));
    }
}

fn emit_progress(app: &AppHandle, job_id: &str, status: &str, percent: f64, speed: &str, eta: &str, title: Option<String>, error: Option<String>) {
    let _ = app.emit("download-progress", ProgressEvent {
        job_id: job_id.to_string(),
        status: status.to_string(),
        percent,
        speed: speed.to_string(),
        eta: eta.to_string(),
        title,
        error,
    });
}

fn build_args(opts: &DownloadOptions) -> Vec<String> {
    let mut args = vec![];

    if opts.audio_only {
        args.push("-x".to_string());
        args.push("--audio-format".to_string());
        args.push(opts.audio_format.clone());
    } else {
        args.push("-f".to_string());
        args.push(opts.format_id.clone());
        args.push("--merge-output-format".to_string());
        args.push("mp4".to_string());
    }

    args.push("--newline".to_string());
    args.push("--no-playlist".to_string());
    args.push("--no-warnings".to_string());
    args.push("-o".to_string());
    args.push(format!("{}/{}", opts.output_dir, opts.filename_template));
    args.push(opts.url.clone());

    args
}

fn parse_progress_line(line: &str) -> Option<(f64, String, String)> {
    let pct_re = Regex::new(r"\[download\]\s+([\d.]+)%").ok()?;
    let speed_re = Regex::new(r"at\s+([\d.]+\s*\w+/s)").ok()?;
    let eta_re = Regex::new(r"ETA\s+(\d+:\d+)").ok()?;

    if !line.contains("[download]") {
        return None;
    }

    let pct = pct_re.captures(line)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<f64>().ok())
        .unwrap_or(0.0);

    let speed = speed_re.captures(line)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_default();

    let eta = eta_re.captures(line)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();

    Some((pct, speed, eta))
}

pub fn spawn_download(
    app: AppHandle,
    manager: Arc<Mutex<DownloadManager>>,
    history: Arc<Mutex<Vec<DownloadRecord>>>,
    app_data_dir: String,
    job_id: String,
    opts: DownloadOptions,
    ytdlp_path: String,
) {
    let (cancel_tx, cancel_rx) = watch::channel(false);

    let semaphore = {
        let mgr = manager.lock().unwrap();
        mgr.semaphore.clone()
    };

    {
        let mut mgr = manager.lock().unwrap();
        mgr.handles.insert(job_id.clone(), DownloadHandle {
            cancel_tx,
            title: opts.title.clone(),
        });
    }

    let started_at = Utc::now().to_rfc3339();

    emit_progress(&app, &job_id, "queued", 0.0, "", "", opts.title.clone(), None);

    tokio::spawn(async move {
        let _permit = match semaphore.acquire_owned().await {
            Ok(p) => p,
            Err(_) => {
                emit_progress(&app, &job_id, "error", 0.0, "", "", opts.title.clone(), Some("Semaphore closed".to_string()));
                return;
            }
        };

        emit_progress(&app, &job_id, "downloading", 0.0, "", "", opts.title.clone(), None);

        let result = run_download(&app, &job_id, &opts, &ytdlp_path, cancel_rx).await;

        let (status, error_msg) = match &result {
            Ok(_) => {
                emit_progress(&app, &job_id, "finished", 100.0, "", "", opts.title.clone(), None);
                ("completed".to_string(), None)
            }
            Err(e) if e.to_string() == "Cancelled" => {
                emit_progress(&app, &job_id, "cancelled", 0.0, "", "", opts.title.clone(), None);
                ("cancelled".to_string(), None)
            }
            Err(e) => {
                let msg = e.to_string();
                emit_progress(&app, &job_id, "error", 0.0, "", "", opts.title.clone(), Some(msg.clone()));
                ("error".to_string(), Some(msg))
            }
        };

        let record = DownloadRecord {
            id: job_id.clone(),
            url: opts.url.clone(),
            title: opts.title.clone(),
            output_path: result.ok(),
            format_id: opts.format_id.clone(),
            audio_only: opts.audio_only,
            status,
            error_msg,
            started_at,
            finished_at: Some(Utc::now().to_rfc3339()),
        };

        {
            let mut hist = history.lock().unwrap();
            hist.insert(0, record);
            hist.truncate(200);
            let _ = DownloadRecord::save_all(&app_data_dir, &hist);
        }

        {
            let mut mgr = manager.lock().unwrap();
            mgr.handles.remove(&job_id);
        }
    });
}

async fn run_download(
    app: &AppHandle,
    job_id: &str,
    opts: &DownloadOptions,
    ytdlp_path: &str,
    mut cancel_rx: watch::Receiver<bool>,
) -> anyhow::Result<String> {
    let args = build_args(opts);

    let mut child = Command::new(ytdlp_path)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to start yt-dlp: {}. Make sure yt-dlp is installed.", e))?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let mut stdout_lines = BufReader::new(stdout).lines();
    let mut stderr_lines = BufReader::new(stderr).lines();
    let mut stderr_buf = String::new();
    let mut last_output_path: Option<String> = None;

    let dest_re = Regex::new(r"\[download\] Destination: (.+)").unwrap();
    let merge_re = Regex::new(r#"Merging formats into "(.+)""#).unwrap();

    loop {
        tokio::select! {
            changed = cancel_rx.changed() => {
                if changed.is_ok() && *cancel_rx.borrow() {
                    let _ = child.kill().await;
                    return Err(anyhow::anyhow!("Cancelled"));
                }
            }
            result = stdout_lines.next_line() => {
                match result {
                    Ok(Some(line)) => {
                        if let Some((pct, speed, eta)) = parse_progress_line(&line) {
                            emit_progress(app, job_id, "downloading", pct, &speed, &eta, opts.title.clone(), None);
                        } else if line.contains("[Merger]") || line.contains("Merging formats") {
                            if let Some(caps) = merge_re.captures(&line) {
                                last_output_path = Some(caps[1].to_string());
                            }
                            emit_progress(app, job_id, "merging", 99.0, "", "", opts.title.clone(), None);
                        } else if line.contains("[download] Destination:") {
                            if let Some(caps) = dest_re.captures(&line) {
                                last_output_path = Some(caps[1].trim().to_string());
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
            result = stderr_lines.next_line() => {
                if let Ok(Some(line)) = result {
                    if !line.trim().is_empty() {
                        stderr_buf.push_str(&line);
                        stderr_buf.push('\n');
                    }
                }
            }
        }
    }

    let exit_status = child.wait().await?;
    if !exit_status.success() {
        let err = stderr_buf.trim().to_string();
        return Err(anyhow::anyhow!("{}", if err.is_empty() { "Download failed".to_string() } else { err }));
    }

    Ok(last_output_path.unwrap_or_default())
}
