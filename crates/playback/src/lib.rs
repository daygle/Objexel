use anyhow::{bail, Result};
use std::{path::{Path, PathBuf}, sync::Arc};
use tokio::fs;

#[derive(Clone)]
pub struct PlaybackService { root: Arc<PathBuf> }

impl PlaybackService {
    pub fn new(root: impl Into<PathBuf>) -> Self { Self { root: Arc::new(root.into()) } }
    pub async fn read_file(&self, path: &str) -> Result<Vec<u8>> { let path = self.safe_path(path)?; Ok(fs::read(path).await?) }
    pub fn content_type(path: &str) -> &'static str { match Path::new(path).extension().and_then(|ext| ext.to_str()).unwrap_or_default().to_ascii_lowercase().as_str() { "mp4" => "video/mp4", "jpg" | "jpeg" => "image/jpeg", "png" => "image/png", _ => "application/octet-stream" } }
    fn safe_path(&self, path: &str) -> Result<PathBuf> {
        let requested = Path::new(path);
        if requested.is_absolute()
            || requested.components().any(|component| matches!(component, std::path::Component::ParentDir | std::path::Component::Prefix(_)))
        {
            bail!("media path is outside playback storage");
        }
        let root = std::fs::canonicalize(&*self.root)?;
        let normalized = root.join(requested).canonicalize()?;
        if !normalized.starts_with(&root) { bail!("media path is outside playback storage"); }
        Ok(normalized)
    }
}
