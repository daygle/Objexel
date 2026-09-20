use anyhow::{bail, Context, Result};
use chrono::Utc;
use objexel_common::{BenchmarkResult, CreateModel, Model, ModelCatalogEntry};
use objexel_detector::{ModelConfig, ModelManager};
use sha2::{Digest, Sha256};
use std::{fs, path::{Path, PathBuf}, time::Instant};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
pub enum InferenceProfile { Fast, Balanced, Accurate }
impl InferenceProfile { pub fn confidence_threshold(self) -> f32 { match self { Self::Fast => 0.40, Self::Balanced => 0.25, Self::Accurate => 0.15 } } }

#[derive(Clone)]
pub struct ModelRegistry { pub model_manager: ModelManager, pub root: PathBuf }
impl ModelRegistry {
    pub fn new(root: impl Into<PathBuf>, model_manager: ModelManager) -> Self { Self { root: root.into(), model_manager } }
    pub async fn discover(&self) -> Result<Vec<ModelConfig>> {
        let mut configs = Vec::new();
        for (directory, model_type) in [("yolov8", "yolov8"), ("yolov11", "yolov11"), ("yolov26", "yolov26")] {
            let path = self.root.join(directory); if !path.is_dir() { continue; }
            for entry in fs::read_dir(&path).with_context(|| format!("scan {}", path.display()))? {
                let file = entry?.path(); if file.extension().and_then(|v| v.to_str()) != Some("onnx") { continue; }
                configs.push(ModelConfig { id: Uuid::new_v4(), name: file.file_stem().and_then(|v| v.to_str()).unwrap_or(directory).into(), version: "discovered".into(), model_type: model_type.into(), path: file, input_width: 640, input_height: 640, labels: Vec::new(), confidence_threshold: 0.25 });
            }
        } Ok(configs)
    }
    pub async fn load_all(&self) -> Result<usize> { let n = self.load_configs(self.discover().await?).await?; tracing::info!(loaded=n, root=%self.root.display(), "model discovery complete"); Ok(n) }
    pub async fn load_registered(&self, models: &[Model]) -> Result<usize> {
        let mut configs = Vec::new();
        for model in models.iter().filter(|model| model.enabled) {
            let path = Self::validated_path(&self.root, Path::new(&model.path), model.input_width, model.input_height)?;
            configs.push(ModelConfig { id: model.id, name: model.name.clone(), version: model.version.clone(), model_type: model.model_type.clone(), path, input_width: model.input_width, input_height: model.input_height, labels: model.class_list.clone(), confidence_threshold: 0.25 });
        }
        self.load_configs(configs).await
    }
    async fn load_configs(&self, configs: Vec<ModelConfig>) -> Result<usize> { let mut n=0; for c in configs { self.model_manager.load(c).await?; n+=1; } Ok(n) }
    pub fn validated_path(root: &Path, path: &Path, width: u32, height: u32) -> Result<PathBuf> {
        if path.extension().and_then(|value| value.to_str()) != Some("onnx") { bail!("model must use the ONNX format"); }
        if width == 0 || height == 0 { bail!("model input dimensions must be positive"); }
        let canonical_root = root.canonicalize().with_context(|| format!("model root does not exist: {}", root.display()))?;
        let canonical_path = path.canonicalize().with_context(|| format!("model file does not exist: {}", path.display()))?;
        if !canonical_path.starts_with(&canonical_root) { bail!("model path is outside the model directory"); }
        Ok(canonical_path)
    }

    pub async fn validate(path: &Path, width: u32, height: u32) -> Result<()> {
        if path.extension().and_then(|value| value.to_str()) != Some("onnx") { bail!("model must use the ONNX format"); }
        if !path.is_file() { bail!("model file does not exist: {}", path.display()); }
        if width == 0 || height == 0 { bail!("model input dimensions must be positive"); }
        Ok(())
    }
    pub async fn benchmark(&self, model: &Model, iterations: u32) -> Result<BenchmarkResult> { let iterations=iterations.max(1); let config=self.model_manager.model_config(model.id).await.context("model is not loaded")?; let start=Instant::now(); for _ in 0..iterations { self.model_manager.benchmark_once(model.id,config.input_width,config.input_height).await?; } let elapsed=start.elapsed().as_secs_f32().max(0.000001); Ok(BenchmarkResult{id:Uuid::new_v4(),model_id:model.id,fps:iterations as f32/elapsed,average_inference_time_ms:elapsed*1000./iterations as f32,gpu_memory_usage_mb:None,cpu_usage_percent:None,test_timestamp:Utc::now()}) }

    pub async fn download_catalog_entry(&self, entry: &ModelCatalogEntry) -> Result<PathBuf> {
        self.download_catalog_entry_with_progress(entry, |_progress, _bytes, _total| {}).await
    }

    pub async fn download_catalog_entry_with_progress<F>(&self, entry: &ModelCatalogEntry, mut progress: F) -> Result<PathBuf>
    where F: FnMut(u8, i64, Option<i64>) + Send {
        if entry.download_url.parse::<reqwest::Url>().is_err() { bail!("catalog download URL must be absolute"); }
        if entry.sha256.len() != 64 || !entry.sha256.chars().all(|c| c.is_ascii_hexdigit()) { bail!("catalog entry has an invalid SHA-256 digest"); }
        fs::create_dir_all(&self.root)?;
        let temporary = self.root.join(format!(".{}.download", safe_name(&entry.id)));
        let response = reqwest::Client::new().get(&entry.download_url).send().await?.error_for_status()?;
        let total = response.content_length().map(|value| value as i64);
        let mut response = response;
        let mut output = tokio::fs::File::create(&temporary).await?;
        let mut hasher = Sha256::new(); let mut downloaded: i64 = 0;
        progress(0, 0, total);
        while let Some(chunk) = response.chunk().await? {
            output.write_all(&chunk).await?; hasher.update(&chunk); downloaded += chunk.len() as i64;
            let percent = total.filter(|value| *value > 0).map(|value| ((downloaded * 100) / value).min(99) as u8).unwrap_or(0);
            progress(percent, downloaded, total);
        }
        output.flush().await?; drop(output);
        let digest = hex::encode(hasher.finalize());
        if !digest.eq_ignore_ascii_case(&entry.sha256) { let _=fs::remove_file(&temporary); bail!("SHA-256 verification failed"); }
        progress(100, downloaded, total);
        let result = match entry.archive_format.as_deref() {
            None => { let path=self.root.join(format!("{}-{}.onnx", safe_name(&entry.name), safe_name(&entry.version))); fs::rename(&temporary,&path)?; path }
            Some("tar.gz") => { let out=self.root.join(safe_name(&entry.id)); fs::create_dir_all(&out)?; let file=fs::File::open(&temporary)?; let decoder=flate2::read::GzDecoder::new(file); extract_tar(decoder,&out)?; fs::remove_file(&temporary)?; find_onnx(&out)? }
            Some("zip") => { let out=self.root.join(safe_name(&entry.id)); fs::create_dir_all(&out)?; let file=fs::File::open(&temporary)?; let mut archive=zip::ZipArchive::new(file)?; for i in 0..archive.len() { let mut item=archive.by_index(i)?; let Some(relative)=item.enclosed_name().map(PathBuf::from) else { bail!("archive contains unsafe path"); }; let target=out.join(relative); if item.is_dir(){fs::create_dir_all(&target)?}else{if let Some(parent)=target.parent(){fs::create_dir_all(parent)?} let mut output=fs::File::create(target)?; std::io::copy(&mut item,&mut output)?;} } fs::remove_file(&temporary)?; find_onnx(&out)? }
            Some(other) => { let _=fs::remove_file(&temporary); bail!("unsupported archive format: {other}"); }
        }; Ok(result)
    }
}
fn safe_name(value:&str)->String { value.chars().map(|c| if c.is_ascii_alphanumeric()||matches!(c,'-'|'_'|'.'){c}else{'_'}).collect() }
fn find_onnx(root:&Path)->Result<PathBuf>{ for e in walk(root)? { if e.extension().and_then(|v|v.to_str())==Some("onnx"){return Ok(e)} } bail!("archive did not contain an ONNX model") }
fn walk(root:&Path)->Result<Vec<PathBuf>>{ let mut out=Vec::new(); for e in fs::read_dir(root)? { let p=e?.path(); if p.is_dir(){out.extend(walk(&p)?)}else{out.push(p)} } Ok(out) }
fn extract_tar<R:std::io::Read>(reader:R, target:&Path)->Result<()> {
    let mut archive = tar::Archive::new(reader);
    for item in archive.entries()? {
        let mut item = item?;
        let path = item.path()?.into_owned();
        let entry_type = item.header().entry_type();
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            bail!("archive contains unsafe link");
        }
        if path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir | std::path::Component::RootDir | std::path::Component::Prefix(_)))
        {
            bail!("archive contains unsafe path");
        }
        let destination = target.join(&path);
        if !destination.starts_with(target) {
            bail!("archive contains unsafe path");
        }
        item.unpack(destination)?;
    }
    Ok(())
}
pub fn model_config_from_create(id: Uuid, input: CreateModel) -> ModelConfig {
    ModelConfig { id, name: input.name, version: input.version, model_type: input.model_type, path: input.path.into(), input_width: input.input_width, input_height: input.input_height, labels: input.class_list, confidence_threshold: 0.25 }
}
#[cfg(test)] mod tests { use super::*; #[test] fn profiles_are_ordered(){assert!(InferenceProfile::Fast.confidence_threshold()>InferenceProfile::Accurate.confidence_threshold())} #[test] fn safe_names_remove_separators(){assert_eq!(safe_name("../../x"),".._.._x")} }
