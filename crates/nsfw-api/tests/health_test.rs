use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use http_body_util::BodyExt;
use nsfw_api::error::ApiError;
use nsfw_core::{AppError, ErrorCode};
use tower::ServiceExt;

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
