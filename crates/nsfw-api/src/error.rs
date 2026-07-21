use axum::Json;
use axum::response::{IntoResponse, Response};
use nsfw_core::AppError;
use serde::Serialize;

pub struct ApiError(pub AppError);

impl From<AppError> for ApiError {
    fn from(err: AppError) -> Self {
        Self(err)
    }
}

#[derive(Serialize)]
struct ErrorBody {
    code: String,
    message: String,
}

#[derive(Serialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.0.status;
        let body = ErrorEnvelope {
            error: ErrorBody {
                code: self.0.code.as_str().to_string(),
                message: self.0.message,
            },
        };
        (status, Json(body)).into_response()
    }
}
