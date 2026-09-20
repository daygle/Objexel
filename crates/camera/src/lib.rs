use anyhow::Result;
use async_trait::async_trait;
use objexel_common::CameraStatus;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[async_trait]
pub trait CameraManager: Send + Sync {
    async fn status(&self, camera_id: Uuid) -> Result<CameraStatus>;
}

#[derive(Clone, Default)]
pub struct CameraService {
    statuses: Arc<RwLock<std::collections::HashMap<Uuid, CameraStatus>>>,
}

impl CameraService {
    pub async fn set_status(&self, camera_id: Uuid, status: CameraStatus) {
        self.statuses.write().await.insert(camera_id, status);
    }
}

#[async_trait]
impl CameraManager for CameraService {
    async fn status(&self, camera_id: Uuid) -> Result<CameraStatus> {
        Ok(self.statuses.read().await.get(&camera_id).cloned().unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn missing_camera_is_unknown() {
        let service = CameraService::default();
        assert_eq!(service.status(Uuid::new_v4()).await.unwrap(), CameraStatus::Unknown);
    }
}
