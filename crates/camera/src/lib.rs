use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use objexel_common::{Camera, CameraStatus, CameraTestResult, StreamMetadata};
use serde::Deserialize;
use std::{collections::HashMap, future::Future, process::Stdio, sync::Arc};
use tokio::{
    io::AsyncReadExt,
    process::Command,
    sync::RwLock,
    time::{sleep, timeout, Duration},
};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// CameraManager trait + CameraService (existing)
// ---------------------------------------------------------------------------

#[async_trait]
pub trait CameraManager: Send + Sync {
    async fn status(&self, camera_id: Uuid) -> Result<CameraStatus>;
    async fn test_connection(&self, camera: &Camera) -> CameraTestResult;
    async fn snapshot(&self, camera: &Camera) -> Result<Vec<u8>>;
}

#[derive(Debug, Clone)]
pub struct FfmpegConfig {
    pub ffmpeg_bin: String,
    pub ffprobe_bin: String,
    pub command_timeout: Duration,
    pub reconnect_initial: Duration,
    pub reconnect_max: Duration,
}

impl Default for FfmpegConfig {
    fn default() -> Self {
        Self {
            ffmpeg_bin: "ffmpeg".into(),
            ffprobe_bin: "ffprobe".into(),
            command_timeout: Duration::from_secs(15),
            reconnect_initial: Duration::from_secs(2),
            reconnect_max: Duration::from_secs(60),
        }
    }
}

#[derive(Clone)]
pub struct CameraService {
    config: FfmpegConfig,
    statuses: Arc<RwLock<HashMap<Uuid, CameraTestResult>>>,
}

impl Default for CameraService {
    fn default() -> Self { Self::new(FfmpegConfig::default()) }
}

impl CameraService {
    pub fn new(config: FfmpegConfig) -> Self {
        Self { config, statuses: Arc::new(RwLock::new(HashMap::new())) }
    }

    pub async fn set_status(&self, camera_id: Uuid, status: CameraStatus) {
        let result = CameraTestResult { camera_id, status, latency_ms: None, metadata: None, error: None };
        self.statuses.write().await.insert(camera_id, result);
    }

    /// Keep probing an enabled camera until the returned task is aborted.
    pub fn spawn_monitor<F, Fut>(&self, camera: Camera, on_result: F) -> tokio::task::JoinHandle<()>
    where
        F: Fn(CameraTestResult) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let service = self.clone();
        tokio::spawn(async move {
            let mut delay = service.config.reconnect_initial;
            loop {
                let result = service.test_connection(&camera).await;
                on_result(result.clone()).await;
                if result.status == CameraStatus::Online {
                    delay = service.config.reconnect_initial;
                    sleep(Duration::from_secs(30)).await;
                } else {
                    tracing::warn!(camera_id = %camera.id, ?delay, error = ?result.error, "camera probe failed; retrying");
                    sleep(delay).await;
                    delay = std::cmp::min(delay.saturating_mul(2), service.config.reconnect_max);
                }
            }
        })
    }

    async fn probe(&self, camera: &Camera) -> Result<StreamMetadata> {
        let started = std::time::Instant::now();
        let output = timeout(
            self.config.command_timeout,
            Command::new(&self.config.ffprobe_bin)
                .args(["-v", "error", "-rtsp_transport", "tcp", "-show_entries", "stream=codec_name,width,height,r_frame_rate", "-of", "json"])
                .arg(&camera.rtsp_url)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        )
        .await
        .context("ffprobe timed out")??;
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Err(anyhow!("ffprobe failed: {}", if error.is_empty() { "unknown error" } else { &error }));
        }
        let report: ProbeReport = serde_json::from_slice(&output.stdout).context("parse ffprobe metadata")?;
        let stream = report.streams.into_iter().next().context("RTSP stream has no media streams")?;
        tracing::debug!(camera_id = %camera.id, latency_ms = started.elapsed().as_millis(), "RTSP probe succeeded");
        Ok(StreamMetadata { codec: stream.codec_name, width: stream.width, height: stream.height, frame_rate: stream.frame_rate })
    }

    async fn capture_snapshot(&self, camera: &Camera) -> Result<Vec<u8>> {
        let output = timeout(
            self.config.command_timeout,
            Command::new(&self.config.ffmpeg_bin)
                .args(["-hide_banner", "-loglevel", "error", "-rtsp_transport", "tcp", "-i"])
                .arg(&camera.rtsp_url)
                .args(["-frames:v", "1", "-f", "image2pipe", "-vcodec", "mjpeg", "pipe:1"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        )
        .await
        .context("ffmpeg snapshot timed out")??;
        if !output.status.success() || output.stdout.is_empty() {
            let error = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Err(anyhow!("snapshot capture failed: {}", if error.is_empty() { "empty image" } else { &error }));
        }
        Ok(output.stdout)
    }
}

#[async_trait]
impl CameraManager for CameraService {
    async fn status(&self, camera_id: Uuid) -> Result<CameraStatus> {
        Ok(self.statuses.read().await.get(&camera_id).map(|result| result.status.clone()).unwrap_or_default())
    }

    async fn test_connection(&self, camera: &Camera) -> CameraTestResult {
        let started = std::time::Instant::now();
        let result = match self.probe(camera).await {
            Ok(metadata) => CameraTestResult { camera_id: camera.id, status: CameraStatus::Online, latency_ms: Some(started.elapsed().as_millis() as u64), metadata: Some(metadata), error: None },
            Err(error) => CameraTestResult { camera_id: camera.id, status: CameraStatus::Offline, latency_ms: Some(started.elapsed().as_millis() as u64), metadata: None, error: Some(error.to_string()) },
        };
        self.statuses.write().await.insert(camera.id, result.clone());
        result
    }

    async fn snapshot(&self, camera: &Camera) -> Result<Vec<u8>> {
        self.capture_snapshot(camera).await
    }
}

#[derive(Debug, Deserialize)]
struct ProbeReport { #[serde(default)] streams: Vec<ProbeStream> }

#[derive(Debug, Deserialize)]
struct ProbeStream {
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    #[serde(rename = "r_frame_rate")]
    frame_rate: Option<String>,
}

// ---------------------------------------------------------------------------
// FrameIngestor — RTSP frame decoding for the live processing pipeline
// ---------------------------------------------------------------------------

/// A single decoded video frame in raw RGB24 format.
#[derive(Debug, Clone)]
pub struct DecodedFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// Configuration for the [`FrameIngestor`].
#[derive(Debug, Clone)]
pub struct IngestorConfig {
    pub ffmpeg_bin: String,
    pub ffprobe_bin: String,
    /// Target frames per second.  Frames are dropped by FFmpeg to match.
    pub fps: f64,
    pub reconnect_initial: Duration,
    pub reconnect_max: Duration,
    pub connect_timeout: Duration,
}

impl Default for IngestorConfig {
    fn default() -> Self {
        Self {
            ffmpeg_bin: "ffmpeg".into(),
            ffprobe_bin: "ffprobe".into(),
            fps: 5.0,
            reconnect_initial: Duration::from_secs(2),
            reconnect_max: Duration::from_secs(60),
            connect_timeout: Duration::from_secs(30),
        }
    }
}

/// Continuously decodes an RTSP stream and yields [`DecodedFrame`]s via a
/// callback.  Automatically reconnects on failure with exponential back-off.
///
/// The returned [`JoinHandle`] runs until aborted (e.g. via `AbortHandle`).
pub struct FrameIngestor {
    config: IngestorConfig,
}

impl FrameIngestor {
    pub fn new(config: IngestorConfig) -> Self { Self { config } }

    /// Spawn a long-lived decode loop for `camera_id` / `rtsp_url`.
    ///
    /// `on_frame` is called for every successfully decoded frame.  The future
    /// runs on the Tokio runtime, so keep the work inside it non-blocking or
    /// spawn heavy processing on a blocking pool.
    pub fn spawn<F, Fut>(&self, camera_id: Uuid, rtsp_url: &str, on_frame: F) -> tokio::task::JoinHandle<()>
    where
        F: Fn(DecodedFrame) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let config = self.config.clone();
        let url = rtsp_url.to_owned();
        tokio::spawn(async move {
            let mut delay = config.reconnect_initial;
            loop {
                match run_stream(&config, &url, &on_frame).await {
                    Ok(()) => {
                        tracing::info!(camera_id = %camera_id, "RTSP ingestor ended; reconnecting");
                        delay = config.reconnect_initial;
                    }
                    Err(error) => {
                        tracing::warn!(camera_id = %camera_id, %error, "RTSP ingestor error; reconnecting");
                    }
                }
                sleep(delay).await;
                delay = std::cmp::min(delay.saturating_mul(2), config.reconnect_max);
            }
        })
    }
}

/// Probe the RTSP stream for video dimensions, then spawn FFmpeg to decode
/// frames at the configured FPS and invoke the callback for each one.
///
/// Uses a channel to bridge the blocking stdout reader and the async callback.
async fn run_stream<F, Fut>(config: &IngestorConfig, rtsp_url: &str, on_frame: &F) -> Result<()>
where
    F: Fn(DecodedFrame) -> Fut + Send + Sync,
    Fut: Future<Output = ()> + Send,
{
    let (width, height) = probe_dimensions(&config.ffprobe_bin, &config.connect_timeout, rtsp_url).await?;
    let frame_bytes = (width * height * 3) as usize;

    let fps_arg = format!("{}", config.fps);
    let mut child = timeout(config.connect_timeout, async {
        Command::new(&config.ffmpeg_bin)
            .args([
                "-hide_banner", "-loglevel", "error",
                "-rtsp_transport", "tcp",
                "-i", rtsp_url,
                "-vf", &format!("fps={fps_arg}"),
                "-f", "rawvideo",
                "-pix_fmt", "rgb24",
                "pipe:1",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("spawn ffmpeg")
    })
    .await
    .context("ffmpeg spawn timed out")??;

    let mut stdout = child.stdout.take().context("ffmpeg stdout not captured")?;

    // Read complete frames from stdout and invoke the callback for each.
    let mut frame_buf = vec![0u8; frame_bytes];
    let mut result: Result<()> = Ok(());
    loop {
        match stdout.read_exact(&mut frame_buf).await {
            Ok(_) => {
                on_frame(DecodedFrame { width, height, data: frame_buf.clone() }).await;
            }
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(e) => {
                result = Err(anyhow!("read ffmpeg frame: {e}"));
                break;
            }
        }
    }

    let _ = child.wait().await;
    result
}

/// Probe the RTSP stream and return `(width, height)` of the first video stream.
async fn probe_dimensions(ffprobe_bin: &str, command_timeout: &Duration, rtsp_url: &str) -> Result<(u32, u32)> {
    let output = timeout(
        *command_timeout,
        Command::new(ffprobe_bin)
            .args(["-v", "error", "-rtsp_transport", "tcp", "-show_entries", "stream=width,height", "-of", "json"])
            .arg(rtsp_url)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output(),
    )
    .await
    .context("ffprobe timed out")??;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(anyhow!("ffprobe failed: {}", if error.is_empty() { "unknown error" } else { &error }));
    }

    let report: ProbeReport = serde_json::from_slice(&output.stdout).context("parse ffprobe metadata")?;
    let stream = report.streams.into_iter().next().context("RTSP stream has no video streams")?;
    let width = stream.width.context("probed stream missing width")?;
    let height = stream.height.context("probed stream missing height")?;
    Ok((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn missing_camera_is_unknown() {
        let service = CameraService::default();
        assert_eq!(service.status(Uuid::new_v4()).await.unwrap(), CameraStatus::Unknown);
    }

    #[test]
    fn default_config_uses_standard_ffmpeg_commands() {
        let config = FfmpegConfig::default();
        assert_eq!(config.ffmpeg_bin, "ffmpeg");
        assert_eq!(config.ffprobe_bin, "ffprobe");
        assert!(config.reconnect_max > config.reconnect_initial);
    }

    #[test]
    fn ffprobe_metadata_is_decoded() {
        let report: ProbeReport = serde_json::from_str(
            r#"{"streams":[{"codec_name":"h264","width":1920,"height":1080,"r_frame_rate":"25/1"}]}"#,
        ).unwrap();
        let stream = report.streams.into_iter().next().unwrap();
        assert_eq!(stream.codec_name.as_deref(), Some("h264"));
        assert_eq!(stream.width, Some(1920));
        assert_eq!(stream.frame_rate.as_deref(), Some("25/1"));
    }

    #[test]
    fn ingestor_default_config() {
        let config = IngestorConfig::default();
        assert_eq!(config.ffmpeg_bin, "ffmpeg");
        assert!(config.fps > 0.0);
        assert!(config.reconnect_max > config.reconnect_initial);
    }

    #[test]
    fn decoded_frame_holds_rgb24_data() {
        let frame = DecodedFrame { width: 2, height: 2, data: vec![255u8; 12] };
        assert_eq!(frame.data.len(), (frame.width * frame.height * 3) as usize);
    }
}
