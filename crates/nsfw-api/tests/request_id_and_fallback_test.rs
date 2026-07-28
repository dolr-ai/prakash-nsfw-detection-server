use axum::Router;
use axum::body::Body;
use axum::extract::Extension;
use axum::http::{Request, StatusCode};
use axum::middleware;
use axum::routing::get;
use nsfw_api::request_id::{REQUEST_ID_HEADER, RequestId, request_id_middleware};
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

#[tokio::test]
async fn request_id_is_available_to_handlers_during_request() {
    async fn echo_id(Extension(id): Extension<RequestId>) -> String {
        id.0
    }
    let app = Router::new()
        .route("/echo", get(echo_id))
        .layer(middleware::from_fn(request_id_middleware));

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/echo")
                .header(REQUEST_ID_HEADER, "abc-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let header_id = resp
        .headers()
        .get(REQUEST_ID_HEADER)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_id = String::from_utf8(body.to_vec()).unwrap();

    assert_eq!(header_id, "abc-123");
    assert_eq!(
        body_id, "abc-123",
        "handler must see the same id the response carries"
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
