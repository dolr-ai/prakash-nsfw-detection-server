use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use http_body_util::BodyExt;
use nsfw_api::error::ApiError;
use nsfw_api::health::{self, ReadinessChecks};
use nsfw_core::{AppError, ErrorCode};
use std::sync::Arc;
use tower::ServiceExt;

fn checks(all_ready: bool) -> ReadinessChecks {
    ReadinessChecks {
        internal_auth: Arc::new(move || all_ready),
        postgres: Arc::new(move || all_ready),
        kvrocks: Arc::new(move || all_ready),
        clickhouse: Arc::new(move || all_ready),
        gpu: Arc::new(move || all_ready),
        ffmpeg: Arc::new(move || all_ready),
        ffprobe: Arc::new(move || all_ready),
    }
}

#[tokio::test]
async fn health_is_always_ok() {
    let app: Router = Router::new().route("/health", get(health::health));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn ready_returns_503_when_any_dependency_not_ready() {
    let app: Router = Router::new().route("/ready", get(health::ready).with_state(checks(false)));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn ready_returns_200_when_all_dependencies_ready() {
    let app: Router = Router::new().route("/ready", get(health::ready).with_state(checks(true)));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn app_error_produces_the_exact_error_envelope_shape() {
    async fn handler() -> Result<(), ApiError> {
        Err(AppError::new(ErrorCode::NotFound, "video job not found").into())
    }
    let app: Router = Router::new().route("/boom", get(handler));
    let response = app
        .oneshot(Request::builder().uri("/boom").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "not_found");
    assert_eq!(json["error"]["message"], "video job not found");
}
