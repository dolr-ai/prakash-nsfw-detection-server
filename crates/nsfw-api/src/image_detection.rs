use base64::Engine;
use nsfw_clients::gpu::ImageInput;
use nsfw_config::Settings;
use nsfw_core::{AppError, ErrorCode, ModerationModelOutput};
use nsfw_services::gpu_moderation::GpuModerationService;
use std::sync::Arc;

use crate::error::ApiError;

enum DownloadFailure {
    Timeout,
    HttpStatus,
    Request,
}

pub struct ImageDetectionService {
    settings: Arc<Settings>,
    gpu_service: Option<Arc<GpuModerationService>>,
    http_client: reqwest::Client,
}

impl ImageDetectionService {
    pub fn new(
        settings: Arc<Settings>,
        gpu_service: Option<Arc<GpuModerationService>>,
        http_client: reqwest::Client,
    ) -> Self {
        Self {
            settings,
            gpu_service,
            http_client,
        }
    }

    pub async fn detect_url(
        &self,
        image_url: &str,
        prompt: Option<&str>,
    ) -> Result<ModerationModelOutput, ApiError> {
        let gpu_service = self.require_gpu_service()?;
        let image_bytes = self.download_image_with_retries(image_url).await?;
        self.detect_image_bytes(gpu_service, image_bytes, prompt)
            .await
    }

    pub async fn detect_base64(
        &self,
        image_base64: &str,
        prompt: Option<&str>,
    ) -> Result<ModerationModelOutput, ApiError> {
        // Decode BEFORE the gpu-configured check, matching Python's ordering exactly:
        // `detect_base64` decodes first and only reaches the gpu check inside
        // `_detect_image_bytes`. Checking gpu first would return 503 gpu_not_configured
        // where Python returns 400 invalid_image_base64 for a malformed body.
        let image_bytes = base64::engine::general_purpose::STANDARD
            .decode(image_base64)
            .map_err(|_| {
                ApiError::from(AppError::new(
                    ErrorCode::InvalidImageBase64,
                    "image_base64 must be valid base64",
                ))
            })?;
        let gpu_service = self.require_gpu_service()?;
        self.detect_image_bytes(gpu_service, image_bytes, prompt)
            .await
    }

    fn require_gpu_service(&self) -> Result<&Arc<GpuModerationService>, ApiError> {
        self.gpu_service.as_ref().ok_or_else(|| {
            ApiError::from(AppError::new(
                ErrorCode::GpuNotConfigured,
                "GPU moderation is not configured",
            ))
        })
    }

    async fn detect_image_bytes(
        &self,
        gpu_service: &GpuModerationService,
        image_bytes: Vec<u8>,
        prompt: Option<&str>,
    ) -> Result<ModerationModelOutput, ApiError> {
        if image_bytes.is_empty() {
            return Err(ApiError::from(AppError::new(
                ErrorCode::EmptyImage,
                "image bytes are empty",
            )));
        }
        if image_bytes.len() as u64 > self.settings.image_max_bytes {
            return Err(ApiError::from(AppError::new(
                ErrorCode::ImageTooLarge,
                "image exceeds configured max bytes",
            )));
        }

        // Matches Python's `_detect_image_bytes`, which always writes the temp file as
        // "image.jpg" regardless of actual format -- so the mime type sent to the GPU
        // is always image/jpeg. Preserved exactly per spec §5's parity policy, not a bug.
        let image = ImageInput {
            bytes: image_bytes,
            mime_type: "image/jpeg".to_string(),
        };
        let generation_prompt = normalize_prompt(prompt);

        gpu_service
            .moderate_image_generation(image, generation_prompt.as_deref())
            .await
            .map_err(|err| {
                ApiError::from(AppError::new(
                    ErrorCode::ModelModerationFailed,
                    err.to_string(),
                ))
            })
    }

    async fn download_image_with_retries(&self, image_url: &str) -> Result<Vec<u8>, ApiError> {
        let max_attempts = self.settings.image_download_max_attempts.max(1);
        let mut last_error: Option<DownloadFailure> = None;

        for attempt in 1..=max_attempts {
            let request =
                self.http_client
                    .get(image_url)
                    .timeout(std::time::Duration::from_secs_f64(
                        self.settings.image_download_timeout_seconds,
                    ));

            match request.send().await {
                Ok(response) => match response.error_for_status() {
                    Ok(response) => {
                        return Ok(response
                            .bytes()
                            .await
                            .map(|b| b.to_vec())
                            .unwrap_or_default());
                    }
                    Err(err) => {
                        let status = err.status().map(|s| s.as_u16()).unwrap_or(0);
                        // 4xx is non-retryable and maps to a plain download failure,
                        // exactly like Python's `status_code < 500` branch.
                        if status < 500 {
                            return Err(ApiError::from(AppError::new(
                                ErrorCode::ImageDownloadFailed,
                                "image_url could not be downloaded",
                            )));
                        }
                        last_error = Some(DownloadFailure::HttpStatus);
                    }
                },
                Err(err) if err.is_timeout() => last_error = Some(DownloadFailure::Timeout),
                Err(_) => last_error = Some(DownloadFailure::Request),
            }

            if attempt < max_attempts {
                sleep_before_retry(
                    attempt,
                    max_attempts,
                    self.settings.image_download_retry_base_delay_seconds,
                )
                .await;
            }
        }

        Err(match last_error {
            Some(DownloadFailure::Timeout) => ApiError::from(AppError::new(
                ErrorCode::ImageDownloadTimeout,
                "image_url download timed out",
            )),
            Some(DownloadFailure::HttpStatus) => ApiError::from(AppError::new(
                ErrorCode::ImageDownloadUpstreamError,
                "image_url host returned an upstream error",
            )),
            _ => ApiError::from(AppError::new(
                ErrorCode::ImageDownloadFailed,
                "image_url could not be downloaded",
            )),
        })
    }
}

fn normalize_prompt(prompt: Option<&str>) -> Option<String> {
    prompt
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Capped exponential backoff with full jitter (spec §6.3 -- image-download retry site).
async fn sleep_before_retry(attempt: u32, max_attempts: u32, base_delay_seconds: f64) {
    if attempt >= max_attempts || base_delay_seconds <= 0.0 {
        return;
    }
    let capped = (base_delay_seconds * 2f64.powi(attempt as i32 - 1)).min(2.0);
    let jitter = 0.5 + rand::random::<f64>();
    tokio::time::sleep(std::time::Duration::from_secs_f64(
        (capped * jitter).max(0.0),
    ))
    .await;
}
