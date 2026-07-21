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
