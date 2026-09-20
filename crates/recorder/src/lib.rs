use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use objexel_common::{Clip, Recording, Snapshot};
use std::{path::PathBuf, process::Stdio, sync::Arc};
use tokio::{fs, process::{Child, Command}, time::{timeout, Duration as TokioDuration}};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct RecorderConfig {
    pub ffmpeg_bin: String,
    pub storage_root: PathBuf,
    pub segment_seconds: u32,
    pub clip_before_seconds: u32,
    pub clip_after_seconds: u32,
    pub retention: RetentionPolicy,
}

#[derive(Debug, Clone)]
pub struct RetentionPolicy {
    pub continuous_days: i64,
    pub event_days: i64,
    pub snapshot_days: i64,
}

impl Default for RecorderConfig {
    fn default() -> Self { Self { ffmpeg_bin: "ffmpeg".into(), storage_root: "/var/lib/objexel".into(), segment_seconds: 60, clip_before_seconds: 15, clip_after_seconds: 15, retention: RetentionPolicy { continuous_days: 7, event_days: 30, snapshot_days: 30 } } }
}

#[derive(Clone)]
pub struct Recorder { config: Arc<RecorderConfig> }

impl Default for Recorder { fn default() -> Self { Self::new(RecorderConfig::default()) } }

impl Recorder {
    pub fn new(config: RecorderConfig) -> Self { Self { config: Arc::new(config) } }
    pub fn config(&self) -> &RecorderConfig { &self.config }

    pub async fn start_continuous(&self, camera_id: Uuid, rtsp_url: &str) -> Result<(Recording, Child)> {
        let directory = self.config.storage_root.join("recordings").join(camera_id.to_string());
        fs::create_dir_all(&directory).await?;
        let start_time = Utc::now();
        let pattern = directory.join("%Y-%m-%d_%H-%M-%S.mp4");
        let child = Command::new(&self.config.ffmpeg_bin)
            .args(["-hide_banner", "-loglevel", "error", "-rtsp_transport", "tcp", "-i"])
            .arg(rtsp_url)
            .args(["-map", "0:v:0", "-c:v", "copy", "-an", "-f", "segment", "-reset_timestamps", "1", "-segment_atclocktime", "1"])
            .arg("-segment_time").arg(self.config.segment_seconds.to_string())
            .arg(pattern.to_string_lossy().as_ref())
            .stdout(Stdio::null()).stderr(Stdio::piped()).spawn().context("start continuous FFmpeg recording")?;
        Ok((Recording { id: Uuid::new_v4(), camera_id, start_time, end_time: None, file_path: directory.to_string_lossy().into_owned(), file_size: None, mode: "continuous".into() }, child))
    }

    pub async fn create_event_clip(&self, camera_id: Uuid, event_id: Uuid, rtsp_url: &str, event_time: DateTime<Utc>) -> Result<Clip> {
        let directory = self.config.storage_root.join("clips").join(camera_id.to_string());
        fs::create_dir_all(&directory).await?;
        let start = event_time - Duration::seconds(self.config.clip_before_seconds as i64);
        let duration = self.config.clip_before_seconds + self.config.clip_after_seconds;
        let path = directory.join(format!("{event_id}.mp4"));
        let mut command = Command::new(&self.config.ffmpeg_bin);
        command.args(["-hide_banner", "-loglevel", "error", "-rtsp_transport", "tcp", "-ss"])
            .arg(start.to_rfc3339()).arg("-i").arg(rtsp_url)
            .args(["-t"]).arg(duration.to_string()).args(["-map", "0:v:0", "-c:v", "copy", "-an", "-movflags", "+faststart"])
            .arg(&path).stdout(Stdio::null()).stderr(Stdio::piped());
        let mut child = command.spawn().context("start event clip FFmpeg")?;
        let output = timeout(TokioDuration::from_secs((duration + 15) as u64), child.wait_with_output()).await.context("event clip timed out")??;
        if !output.status.success() { anyhow::bail!("event clip failed: {}", String::from_utf8_lossy(&output.stderr).trim()); }
        Ok(Clip { id: Uuid::new_v4(), event_id: Some(event_id), recording_id: None, clip_start: start, clip_end: event_time + Duration::seconds(self.config.clip_after_seconds as i64), clip_path: path.to_string_lossy().into_owned() })
    }

    pub async fn create_snapshot(&self, camera_id: Uuid, event_id: Option<Uuid>, rtsp_url: &str, timestamp: DateTime<Utc>) -> Result<Snapshot> {
        let directory = self.config.storage_root.join("snapshots").join(camera_id.to_string());
        fs::create_dir_all(&directory).await?;
        let path = directory.join(format!("{}.jpg", timestamp.timestamp_millis()));
        let output = Command::new(&self.config.ffmpeg_bin)
            .args(["-hide_banner", "-loglevel", "error", "-rtsp_transport", "tcp", "-i"])
            .arg(rtsp_url).args(["-frames:v", "1", "-q:v", "2", "-f", "image2"])
            .arg(&path).output().await.context("capture event snapshot")?;
        if !output.status.success() { anyhow::bail!("snapshot failed: {}", String::from_utf8_lossy(&output.stderr).trim()); }
        Ok(Snapshot { id: Uuid::new_v4(), event_id, camera_id, image_path: path.to_string_lossy().into_owned(), timestamp })
    }

    pub async fn cleanup_paths(&self, now: DateTime<Utc>) -> Result<usize> {
        let mut removed = 0;
        for (folder, days) in [("recordings", self.config.retention.continuous_days), ("clips", self.config.retention.event_days), ("snapshots", self.config.retention.snapshot_days)] {
            let root = self.config.storage_root.join(folder);
            if !root.is_dir() { continue; }
            let mut pending = vec![root];
            while let Some(directory) = pending.pop() {
                let mut entries = fs::read_dir(&directory).await?;
                while let Some(entry) = entries.next_entry().await? {
                    let path = entry.path();
                    let metadata = entry.metadata().await?;
                    if metadata.is_dir() {
                        pending.push(path);
                    } else if metadata.is_file() {
                        if let Ok(modified) = metadata.modified() {
                            if now.signed_duration_since(DateTime::<Utc>::from(modified)).num_days() > days {
                                fs::remove_file(path).await?;
                                removed += 1;
                            }
                        }
                    }
                }
            }
        }
        Ok(removed)
    }
}
