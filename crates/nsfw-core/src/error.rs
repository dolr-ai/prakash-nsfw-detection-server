use http::StatusCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    AuthMissingHeaders,
    AuthBadTimestamp,
    AuthTimestampOutOfRange,
    AuthBadSignature,
    NotFound,
    ServiceUnavailable,
    QueueUnavailable,
    ValidationError,
    ModelModerationFailed,
    ModelResponseInvalidJson,
    ModelResponseInvalidSchema,
    ImageDownloadFailed,
    ImageDownloadTimeout,
    ImageDownloadUpstreamError,
    VideoDownloadEmpty,
    VideoTooLarge,
    VideoNoStream,
    VideoProbeFailed,
    VideoExtractionFailed,
    GpuNotConfigured,
    InvalidImageBase64,
    EmptyImage,
    ImageTooLarge,
    StorjNotConfigured,
    /// Declared in Python's codes.py, never raised. Carried for parity; never construct this.
    NotImplemented,
    /// Declared in Python's codes.py, never raised. Carried for parity; never construct this.
    QueueError,
}

impl ErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AuthMissingHeaders => "auth_missing_headers",
            Self::AuthBadTimestamp => "auth_bad_timestamp",
            Self::AuthTimestampOutOfRange => "auth_timestamp_out_of_range",
            Self::AuthBadSignature => "auth_bad_signature",
            Self::NotFound => "not_found",
            Self::ServiceUnavailable => "service_unavailable",
            Self::QueueUnavailable => "queue_unavailable",
            Self::ValidationError => "validation_error",
            Self::ModelModerationFailed => "model_moderation_failed",
            Self::ModelResponseInvalidJson => "model_response_invalid_json",
            Self::ModelResponseInvalidSchema => "model_response_invalid_schema",
            Self::ImageDownloadFailed => "image_download_failed",
            Self::ImageDownloadTimeout => "image_download_timeout",
            Self::ImageDownloadUpstreamError => "image_download_upstream_error",
            Self::VideoDownloadEmpty => "video_download_empty",
            Self::VideoTooLarge => "video_too_large",
            Self::VideoNoStream => "video_no_stream",
            Self::VideoProbeFailed => "video_probe_failed",
            Self::VideoExtractionFailed => "video_extraction_failed",
            Self::GpuNotConfigured => "gpu_not_configured",
            Self::InvalidImageBase64 => "invalid_image_base64",
            Self::EmptyImage => "empty_image",
            Self::ImageTooLarge => "image_too_large",
            Self::StorjNotConfigured => "storj_not_configured",
            Self::NotImplemented => "not_implemented",
            Self::QueueError => "queue_error",
        }
    }

    pub fn default_status(&self) -> StatusCode {
        match self {
            Self::AuthMissingHeaders
            | Self::AuthBadTimestamp
            | Self::AuthTimestampOutOfRange
            | Self::AuthBadSignature => StatusCode::UNAUTHORIZED,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::ServiceUnavailable
            | Self::QueueUnavailable
            | Self::ModelModerationFailed
            | Self::GpuNotConfigured
            | Self::StorjNotConfigured => StatusCode::SERVICE_UNAVAILABLE,
            Self::ValidationError => StatusCode::UNPROCESSABLE_ENTITY,
            Self::ModelResponseInvalidJson
            | Self::ModelResponseInvalidSchema
            | Self::ImageDownloadUpstreamError => StatusCode::BAD_GATEWAY,
            Self::ImageDownloadTimeout => StatusCode::GATEWAY_TIMEOUT,
            Self::ImageDownloadFailed
            | Self::VideoDownloadEmpty
            | Self::VideoTooLarge
            | Self::VideoNoStream
            | Self::VideoProbeFailed
            | Self::VideoExtractionFailed
            | Self::InvalidImageBase64
            | Self::EmptyImage
            | Self::ImageTooLarge => StatusCode::BAD_REQUEST,
            Self::NotImplemented => StatusCode::NOT_IMPLEMENTED,
            Self::QueueError => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    pub status: StatusCode,
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        let status = code.default_status();
        Self {
            code,
            message: message.into(),
            status,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;

    #[rstest::rstest]
    #[case(
        ErrorCode::AuthMissingHeaders,
        "auth_missing_headers",
        StatusCode::UNAUTHORIZED
    )]
    #[case(
        ErrorCode::AuthBadTimestamp,
        "auth_bad_timestamp",
        StatusCode::UNAUTHORIZED
    )]
    #[case(
        ErrorCode::AuthTimestampOutOfRange,
        "auth_timestamp_out_of_range",
        StatusCode::UNAUTHORIZED
    )]
    #[case(
        ErrorCode::AuthBadSignature,
        "auth_bad_signature",
        StatusCode::UNAUTHORIZED
    )]
    #[case(ErrorCode::NotFound, "not_found", StatusCode::NOT_FOUND)]
    #[case(
        ErrorCode::ServiceUnavailable,
        "service_unavailable",
        StatusCode::SERVICE_UNAVAILABLE
    )]
    #[case(
        ErrorCode::QueueUnavailable,
        "queue_unavailable",
        StatusCode::SERVICE_UNAVAILABLE
    )]
    #[case(
        ErrorCode::ValidationError,
        "validation_error",
        StatusCode::UNPROCESSABLE_ENTITY
    )]
    #[case(
        ErrorCode::ModelModerationFailed,
        "model_moderation_failed",
        StatusCode::SERVICE_UNAVAILABLE
    )]
    #[case(
        ErrorCode::ModelResponseInvalidJson,
        "model_response_invalid_json",
        StatusCode::BAD_GATEWAY
    )]
    #[case(
        ErrorCode::ModelResponseInvalidSchema,
        "model_response_invalid_schema",
        StatusCode::BAD_GATEWAY
    )]
    #[case(
        ErrorCode::ImageDownloadFailed,
        "image_download_failed",
        StatusCode::BAD_REQUEST
    )]
    #[case(
        ErrorCode::ImageDownloadTimeout,
        "image_download_timeout",
        StatusCode::GATEWAY_TIMEOUT
    )]
    #[case(
        ErrorCode::ImageDownloadUpstreamError,
        "image_download_upstream_error",
        StatusCode::BAD_GATEWAY
    )]
    #[case(
        ErrorCode::VideoDownloadEmpty,
        "video_download_empty",
        StatusCode::BAD_REQUEST
    )]
    #[case(ErrorCode::VideoTooLarge, "video_too_large", StatusCode::BAD_REQUEST)]
    #[case(ErrorCode::VideoNoStream, "video_no_stream", StatusCode::BAD_REQUEST)]
    #[case(
        ErrorCode::VideoProbeFailed,
        "video_probe_failed",
        StatusCode::BAD_REQUEST
    )]
    #[case(
        ErrorCode::VideoExtractionFailed,
        "video_extraction_failed",
        StatusCode::BAD_REQUEST
    )]
    #[case(
        ErrorCode::GpuNotConfigured,
        "gpu_not_configured",
        StatusCode::SERVICE_UNAVAILABLE
    )]
    #[case(
        ErrorCode::InvalidImageBase64,
        "invalid_image_base64",
        StatusCode::BAD_REQUEST
    )]
    #[case(ErrorCode::EmptyImage, "empty_image", StatusCode::BAD_REQUEST)]
    #[case(ErrorCode::ImageTooLarge, "image_too_large", StatusCode::BAD_REQUEST)]
    #[case(
        ErrorCode::StorjNotConfigured,
        "storj_not_configured",
        StatusCode::SERVICE_UNAVAILABLE
    )]
    fn error_code_matches_exact_wire_string_and_status(
        #[case] code: ErrorCode,
        #[case] expected_str: &str,
        #[case] expected_status: StatusCode,
    ) {
        assert_eq!(code.as_str(), expected_str);
        assert_eq!(code.default_status(), expected_status);
    }

    #[test]
    fn declared_but_never_raised_codes_still_exist_for_registry_completeness() {
        // Python declares these in codes.py but never raises them anywhere -- carried
        // here for parity but no production call site should ever construct them.
        assert_eq!(ErrorCode::NotImplemented.as_str(), "not_implemented");
        assert_eq!(ErrorCode::QueueError.as_str(), "queue_error");
    }

    #[test]
    fn app_error_new_applies_the_codes_default_status() {
        let err = AppError::new(ErrorCode::NotFound, "video job not found");
        assert_eq!(err.code.as_str(), "not_found");
        assert_eq!(err.status, StatusCode::NOT_FOUND);
        assert_eq!(err.message, "video job not found");
    }
}
