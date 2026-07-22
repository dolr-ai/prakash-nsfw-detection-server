use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware;
use axum::routing::post;
use http_body_util::BodyExt;
use nsfw_api::auth::{
    SIGNATURE_HEADER, TIMESTAMP_HEADER, build_signature_message, require_signed_request,
};
use nsfw_api::image_detection::ImageDetectionService;
use nsfw_api::text_detection::TextDetectionService;
use nsfw_api::{moderation_routes, text_detection};
use nsfw_clients::gpu::GpuOpenAiClient;
use nsfw_config::Settings;
use nsfw_services::gpu_moderation::{GpuModerationConfig, GpuModerationService};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn valid_categories_json() -> serde_json::Value {
    json!({"safe":0,"suggestive":0,"nudity":0,"porn":0,"gore":0,"violence":0,"self_harm":0,"hate_or_extremism":0,"drugs":0,"unknown":0,"sexual_minor_content":0})
}

async fn build_app(gpu_server_uri: &str, hmac_secret: &str) -> Router {
    let mut vars = HashMap::new();
    vars.insert(
        "INTERNAL_REQUEST_HMAC_SECRET".to_string(),
        hmac_secret.to_string(),
    );
    let settings = Arc::new(Settings::from_map(&vars).unwrap());

    let client = GpuOpenAiClient::new(
        reqwest::Client::new(),
        gpu_server_uri.to_string(),
        "key".into(),
        "model".into(),
    );
    let config = GpuModerationConfig {
        max_attempts: 1,
        retry_base_delay_seconds: 0.001,
        max_concurrency: 5,
    };
    let gpu_service = Some(Arc::new(GpuModerationService::new(
        client,
        config,
        Some("image prompt".into()),
        Some("image+text prompt".into()),
        Some("text prompt".into()),
    )));

    let image_service = Arc::new(ImageDetectionService::new(
        settings.clone(),
        gpu_service.clone(),
        reqwest::Client::new(),
    ));
    let text_service = Arc::new(TextDetectionService::new(gpu_service));

    let image_router: Router = Router::new()
        .route(
            "/images/detect-url",
            post(moderation_routes::detect_image_url),
        )
        .route(
            "/images/detect-base64",
            post(moderation_routes::detect_image_base64),
        )
        .with_state(image_service);
    let text_router: Router = Router::new()
        .route("/text/detect", post(moderation_routes::detect_text))
        .with_state(text_service);

    Router::new().nest(
        "/v1",
        Router::new()
            .merge(image_router)
            .merge(text_router)
            .layer(middleware::from_fn_with_state(
                settings,
                require_signed_request,
            )),
    )
}

fn sign(secret: &str, method_str: &str, path: &str, body: &[u8]) -> (String, String) {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let timestamp = chrono::Utc::now().timestamp().to_string();
    let message = build_signature_message(&timestamp, method_str, path, body);
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(&message);
    (timestamp, hex::encode(mac.finalize().into_bytes()))
}

#[tokio::test]
async fn detect_text_returns_moderation_response_through_full_hmac_stack() {
    let gpu_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": json!({"top_category":"safe","reason":"clean","categories":valid_categories_json()}).to_string()}}]
        })))
        .mount(&gpu_server)
        .await;

    let app = build_app(&gpu_server.uri(), "test-secret").await;
    let body = serde_json::to_vec(&json!({"text": "a nice sunny day"})).unwrap();
    let (timestamp, signature) = sign("test-secret", "POST", "/v1/text/detect", &body);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/text/detect")
                .header("content-type", "application/json")
                .header(TIMESTAMP_HEADER, timestamp)
                .header(SIGNATURE_HEADER, signature)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["top_category"], "safe");
    assert_eq!(json["is_nsfw"], false);
}

#[tokio::test]
async fn detect_text_rejects_unsigned_request() {
    let gpu_server = MockServer::start().await;
    let app = build_app(&gpu_server.uri(), "test-secret").await;
    let body = serde_json::to_vec(&json!({"text": "hello"})).unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/text/detect")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn detect_image_base64_rejects_invalid_base64() {
    let gpu_server = MockServer::start().await;
    let app = build_app(&gpu_server.uri(), "test-secret").await;
    let body = serde_json::to_vec(&json!({"image_base64": "not-valid-base64!!!"})).unwrap();
    let (timestamp, signature) = sign("test-secret", "POST", "/v1/images/detect-base64", &body);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/images/detect-base64")
                .header("content-type", "application/json")
                .header(TIMESTAMP_HEADER, timestamp)
                .header(SIGNATURE_HEADER, signature)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["error"]["code"], "invalid_image_base64");
}

#[tokio::test]
async fn detect_image_base64_accepts_valid_image_and_returns_moderation_response() {
    let gpu_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": json!([{"frame_index":0,"top_category":"safe","reason":"x","categories":valid_categories_json()}]).to_string()}}]
        })))
        .mount(&gpu_server)
        .await;

    let app = build_app(&gpu_server.uri(), "test-secret").await;
    use base64::Engine;
    let image_base64 = base64::engine::general_purpose::STANDARD.encode(b"fake-image-bytes");
    let body = serde_json::to_vec(&json!({"image_base64": image_base64})).unwrap();
    let (timestamp, signature) = sign("test-secret", "POST", "/v1/images/detect-base64", &body);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/images/detect-base64")
                .header("content-type", "application/json")
                .header(TIMESTAMP_HEADER, timestamp)
                .header(SIGNATURE_HEADER, signature)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn empty_text_is_a_validation_error_not_a_gpu_call() {
    // Python's pydantic `min_length=1` equivalent -- must 422 before any GPU call.
    let gpu_server = MockServer::start().await;
    let app = build_app(&gpu_server.uri(), "test-secret").await;
    let body = serde_json::to_vec(&json!({"text": "   "})).unwrap();
    let (timestamp, signature) = sign("test-secret", "POST", "/v1/text/detect", &body);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/text/detect")
                .header("content-type", "application/json")
                .header(TIMESTAMP_HEADER, timestamp)
                .header(SIGNATURE_HEADER, signature)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["error"]["code"], "validation_error");
}

#[tokio::test]
async fn malformed_json_body_uses_the_error_envelope() {
    // Axum's default Json rejection would emit plain-text 400; must be 422 in-envelope.
    let gpu_server = MockServer::start().await;
    let app = build_app(&gpu_server.uri(), "test-secret").await;
    let body = b"{not json".to_vec();
    let (timestamp, signature) = sign("test-secret", "POST", "/v1/text/detect", &body);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/text/detect")
                .header("content-type", "application/json")
                .header(TIMESTAMP_HEADER, timestamp)
                .header(SIGNATURE_HEADER, signature)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["error"]["code"], "validation_error");
}

#[tokio::test]
async fn missing_required_field_uses_the_error_envelope() {
    let gpu_server = MockServer::start().await;
    let app = build_app(&gpu_server.uri(), "test-secret").await;
    let body = serde_json::to_vec(&json!({})).unwrap();
    let (timestamp, signature) = sign("test-secret", "POST", "/v1/text/detect", &body);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/text/detect")
                .header("content-type", "application/json")
                .header(TIMESTAMP_HEADER, timestamp)
                .header(SIGNATURE_HEADER, signature)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["error"]["code"], "validation_error");
}

#[tokio::test]
async fn gpu_not_configured_returns_503() {
    let mut vars = HashMap::new();
    vars.insert(
        "INTERNAL_REQUEST_HMAC_SECRET".to_string(),
        "test-secret".to_string(),
    );
    let settings = Arc::new(Settings::from_map(&vars).unwrap());
    let text_service = Arc::new(text_detection::TextDetectionService::new(None));
    let text_router: Router = Router::new()
        .route("/text/detect", post(moderation_routes::detect_text))
        .with_state(text_service);
    let app: Router = Router::new().nest(
        "/v1",
        text_router.layer(middleware::from_fn_with_state(
            settings,
            require_signed_request,
        )),
    );

    let body = serde_json::to_vec(&json!({"text": "hello"})).unwrap();
    let (timestamp, signature) = sign("test-secret", "POST", "/v1/text/detect", &body);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/text/detect")
                .header("content-type", "application/json")
                .header(TIMESTAMP_HEADER, timestamp)
                .header(SIGNATURE_HEADER, signature)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}
