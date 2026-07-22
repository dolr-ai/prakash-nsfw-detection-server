use nsfw_core::{AppError, ErrorCode, ModerationModelOutput};
use nsfw_services::gpu_moderation::GpuModerationService;
use std::sync::Arc;

use crate::error::ApiError;

pub struct TextDetectionService {
    gpu_service: Option<Arc<GpuModerationService>>,
}

impl TextDetectionService {
    pub fn new(gpu_service: Option<Arc<GpuModerationService>>) -> Self {
        Self { gpu_service }
    }

    pub async fn detect(&self, text: &str) -> Result<ModerationModelOutput, ApiError> {
        let gpu_service = self.gpu_service.as_ref().ok_or_else(|| {
            ApiError::from(AppError::new(
                ErrorCode::GpuNotConfigured,
                "GPU moderation is not configured",
            ))
        })?;
        gpu_service.moderate_text(text).await.map_err(|err| {
            ApiError::from(AppError::new(
                ErrorCode::ModelModerationFailed,
                err.to_string(),
            ))
        })
    }
}
