use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware;
use axum::routing::get;
use nsfw_api::auth::{
    SIGNATURE_HEADER, TIMESTAMP_HEADER, build_signature_message, require_signed_request,
};
use nsfw_config::Settings;
use std::collections::HashMap;
use std::sync::Arc;
use tower::ServiceExt;

fn settings_with_secret(secret: &str) -> Arc<Settings> {
    let mut vars = HashMap::new();
    vars.insert(
        "INTERNAL_REQUEST_HMAC_SECRET".to_string(),
        secret.to_string(),
    );
    Arc::new(Settings::from_map(&vars).unwrap())
}

fn protected_app(settings: Arc<Settings>) -> Router {
    Router::new()
        .route("/protected", get(|| async { "ok" }))
        .layer(middleware::from_fn_with_state(
            settings,
            require_signed_request,
        ))
}

fn sign(secret: &str, timestamp: &str, method: &str, path: &str, body: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let message = build_signature_message(timestamp, method, path, body);
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(&message);
    hex::encode(mac.finalize().into_bytes())
}

#[tokio::test]
async fn rejects_missing_auth_headers() {
    let settings = settings_with_secret("test-secret");
    let app = protected_app(settings);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/protected")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn accepts_a_validly_signed_request() {
    let settings = settings_with_secret("test-secret");
    let timestamp = chrono::Utc::now().timestamp().to_string();
    let signature = sign("test-secret", &timestamp, "GET", "/protected", b"");

    let app = protected_app(settings);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/protected")
                .header(TIMESTAMP_HEADER, &timestamp)
                .header(SIGNATURE_HEADER, &signature)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn rejects_wrong_signature() {
    let settings = settings_with_secret("test-secret");
    let timestamp = chrono::Utc::now().timestamp().to_string();

    let app = protected_app(settings);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/protected")
                .header(TIMESTAMP_HEADER, &timestamp)
                .header(SIGNATURE_HEADER, "0".repeat(64))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn rejects_stale_timestamp() {
    let settings = settings_with_secret("test-secret");
    let old_timestamp = (chrono::Utc::now().timestamp() - 10_000).to_string();
    let signature = sign("test-secret", &old_timestamp, "GET", "/protected", b"");

    let app = protected_app(settings);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/protected")
                .header(TIMESTAMP_HEADER, &old_timestamp)
                .header(SIGNATURE_HEADER, &signature)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
