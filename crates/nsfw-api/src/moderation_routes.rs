use axum::Json;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use nsfw_core::{AppError, ErrorCode, ModerationModelOutput};
use serde::Deserialize;
use std::sync::Arc;

use crate::error::ApiError;
use crate::image_detection::ImageDetectionService;
use crate::text_detection::TextDetectionService;

#[derive(Deserialize)]
pub struct ImageUrlDetectRequest {
    pub image_url: String,
    pub prompt: Option<String>,
}

#[derive(Deserialize)]
pub struct ImageBase64DetectRequest {
    pub image_base64: String,
    pub prompt: Option<String>,
}

#[derive(Deserialize)]
pub struct TextDetectRequest {
    pub text: String,
}

/// Mirrors Python's pydantic `min_length=1` on the request fields: a blank/whitespace
/// value is a 422 `validation_error`, not a real GPU call.
fn require_non_empty(value: &str, field: &str) -> Result<(), ApiError> {
    if value.trim().is_empty() {
        return Err(ApiError::from(AppError::new(
            ErrorCode::ValidationError,
            format!("{field} must not be empty"),
        )));
    }
    Ok(())
}

/// Converts axum's own JSON extractor rejection (malformed body, missing field, wrong
/// type) into the service's standard error envelope. Without this, axum emits a
/// plain-text 400/422 that bypasses `{"error":{"code","message"}}` entirely.
fn json_or_validation_error<T>(result: Result<Json<T>, JsonRejection>) -> Result<T, ApiError> {
    result.map(|Json(value)| value).map_err(|rejection| {
        ApiError::from(AppError::new(
            ErrorCode::ValidationError,
            rejection.body_text(),
        ))
    })
}

pub async fn detect_image_url(
    State(service): State<Arc<ImageDetectionService>>,
    request: Result<Json<ImageUrlDetectRequest>, JsonRejection>,
) -> Result<Json<ModerationModelOutput>, ApiError> {
    let request = json_or_validation_error(request)?;
    require_non_empty(&request.image_url, "image_url")?;
    Ok(Json(
        service
            .detect_url(&request.image_url, request.prompt.as_deref())
            .await?,
    ))
}

pub async fn detect_image_base64(
    State(service): State<Arc<ImageDetectionService>>,
    request: Result<Json<ImageBase64DetectRequest>, JsonRejection>,
) -> Result<Json<ModerationModelOutput>, ApiError> {
    let request = json_or_validation_error(request)?;
    require_non_empty(&request.image_base64, "image_base64")?;
    Ok(Json(
        service
            .detect_base64(&request.image_base64, request.prompt.as_deref())
            .await?,
    ))
}

pub async fn detect_text(
    State(service): State<Arc<TextDetectionService>>,
    request: Result<Json<TextDetectRequest>, JsonRejection>,
) -> Result<Json<ModerationModelOutput>, ApiError> {
    let request = json_or_validation_error(request)?;
    require_non_empty(&request.text, "text")?;
    Ok(Json(service.detect(&request.text).await?))
}
