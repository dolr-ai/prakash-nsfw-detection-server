use axum::body::Body;
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use hmac::{Hmac, Mac};
use nsfw_config::Settings;
use nsfw_core::{AppError, ErrorCode};
use secrecy::ExposeSecret;
use sha2::{Digest, Sha256};
use std::sync::Arc;

use crate::error::ApiError;

type HmacSha256 = Hmac<Sha256>;

pub const TIMESTAMP_HEADER: &str = "x-internal-timestamp";
pub const SIGNATURE_HEADER: &str = "x-internal-signature";

pub fn body_sha256_hex(body: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body);
    hex::encode(hasher.finalize())
}

pub fn build_signature_message(timestamp: &str, method: &str, path: &str, body: &[u8]) -> Vec<u8> {
    format!(
        "{}\n{}\n{}\n{}",
        timestamp,
        method.to_uppercase(),
        path,
        body_sha256_hex(body)
    )
    .into_bytes()
}

pub fn signature_has_valid_shape(signature: &str) -> bool {
    signature.len() == 64 && signature.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn verify_signature(
    secret: &str,
    timestamp: &str,
    method: &str,
    path: &str,
    body: &[u8],
    signature: &str,
) -> bool {
    if !signature_has_valid_shape(signature) {
        return false;
    }
    let message = build_signature_message(timestamp, method, path, body);
    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(&message);
    let Ok(sig_bytes) = hex::decode(signature) else {
        return false;
    };
    mac.verify_slice(&sig_bytes).is_ok()
}

pub async fn require_signed_request(
    State(settings): State<Arc<Settings>>,
    req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    // The signed path must be the ORIGINAL request path (e.g. `/v1/text/detect`), the
    // exact path the caller signed -- not `parts.uri.path()`, which axum rewrites to the
    // nest-relative path (`/text/detect`) when this middleware runs inside a nested
    // `/v1` router. `OriginalUri` recovers the pre-nest path. Getting this wrong makes
    // every signature mismatch (401) the moment the middleware is nested.
    let original_path = req
        .extensions()
        .get::<axum::extract::OriginalUri>()
        .map(|uri| uri.0.path().to_string())
        .unwrap_or_else(|| req.uri().path().to_string());

    let (parts, body) = req.into_parts();
    let headers = parts.headers.clone();

    let timestamp_raw = headers
        .get(TIMESTAMP_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let signature = headers
        .get(SIGNATURE_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let (Some(timestamp_raw), Some(signature)) = (timestamp_raw, signature) else {
        return Err(ApiError::from(AppError::new(
            ErrorCode::AuthMissingHeaders,
            "missing internal auth headers",
        )));
    };

    // Deliberately the same code/message as a bad signature -- avoids leaking whether
    // the secret is configured at all. Matches Python's AuthService exactly (spec §9.4).
    let secret = match settings.internal_request_secret() {
        Some(s) => s,
        None => {
            return Err(ApiError::from(AppError::new(
                ErrorCode::AuthBadSignature,
                "invalid internal signature",
            )));
        }
    };

    let timestamp: i64 = timestamp_raw.parse().map_err(|_| {
        ApiError::from(AppError::new(
            ErrorCode::AuthBadTimestamp,
            "timestamp must be unix seconds",
        ))
    })?;

    let now = chrono::Utc::now().timestamp();
    if (now - timestamp).abs() > settings.internal_request_max_skew_sec {
        return Err(ApiError::from(AppError::new(
            ErrorCode::AuthTimestampOutOfRange,
            "stale internal request timestamp",
        )));
    }

    let body_bytes = axum::body::to_bytes(body, usize::MAX).await.map_err(|_| {
        ApiError::from(AppError::new(
            ErrorCode::ValidationError,
            "failed to read request body",
        ))
    })?;

    let secret_str = secret.expose_secret();
    if !verify_signature(
        secret_str,
        &timestamp_raw,
        parts.method.as_str(),
        &original_path,
        &body_bytes,
        &signature,
    ) {
        return Err(ApiError::from(AppError::new(
            ErrorCode::AuthBadSignature,
            "invalid internal signature",
        )));
    }

    let reconstructed = Request::from_parts(parts, Body::from(body_bytes));
    Ok(next.run(reconstructed).await)
}
