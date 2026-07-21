use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware;
use axum::routing::get;
use nsfw_api::request_id::{REQUEST_ID_HEADER, request_id_middleware};
use tower::ServiceExt;

fn app() -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .layer(middleware::from_fn(request_id_middleware))
}

#[tokio::test]
async fn generates_a_request_id_when_none_provided() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(response.headers().get(REQUEST_ID_HEADER).is_some());
}

#[tokio::test]
async fn echoes_a_provided_request_id() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/health")
                .header(REQUEST_ID_HEADER, "my-trace-id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.headers().get(REQUEST_ID_HEADER).unwrap(),
        "my-trace-id"
    );
}

async fn fallback_404() -> axum::response::Response {
    use axum::response::IntoResponse;
    (
        StatusCode::NOT_FOUND,
        axum::Json(serde_json::json!({"error": {"code": "404", "message": "Not Found"}})),
    )
        .into_response()
}

#[tokio::test]
async fn unmatched_route_returns_literal_status_code_string_shape() {
    use http_body_util::BodyExt;

    let app = app().fallback(fallback_404);
    let response = app
        .oneshot(Request::builder().uri("/nope").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "404");
    assert_eq!(json["error"]["message"], "Not Found");
}
