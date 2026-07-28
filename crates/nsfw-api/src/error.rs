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
        let code = self.0.code.as_str().to_string();
        if status.is_server_error() {
            // 5xx -> ERROR (Sentry event). method/path/request_id come from the entered
            // http.request span via sentry-tracing propagation (spec §3.D), not fields here.
            tracing::error!(error_code = %code, http_status = status.as_u16(), "request failed");
        } else {
            // 4xx -> client error, not an alert.
            tracing::debug!(error_code = %code, http_status = status.as_u16(), "request rejected");
        }
        let body = ErrorEnvelope {
            error: ErrorBody {
                code,
                message: self.0.message,
            },
        };
        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;
    use nsfw_core::{AppError, ErrorCode};
    use tracing_test::traced_test;

    #[traced_test]
    #[test]
    fn logs_error_event_for_5xx() {
        // ModelModerationFailed -> 503, a server error.
        let err = ApiError::from(AppError::new(ErrorCode::ModelModerationFailed, "boom"));
        let _ = err.into_response();
        assert!(logs_contain("http_status"));
        assert!(logs_contain("error_code"));
        assert!(logs_contain("request failed"));
    }

    #[traced_test]
    #[test]
    fn does_not_error_log_for_4xx() {
        // ValidationError -> 422, a client error.
        let err = ApiError::from(AppError::new(ErrorCode::ValidationError, "bad"));
        let _ = err.into_response();
        // A 4xx must not produce the ERROR-level "request failed" event (would page on-call).
        assert!(!logs_contain("request failed"));
    }
}
