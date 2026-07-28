use axum::Router;
use axum::routing::{get, post};
use nsfw_api::{auth, health, image_detection, moderation_routes, request_id, text_detection};
use nsfw_clients::gpu::GpuOpenAiClient;
use nsfw_config::Settings;
use nsfw_services::gpu_moderation::{GpuModerationConfig, GpuModerationService};
use secrecy::ExposeSecret;
use std::sync::Arc;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

const IMAGE_PROMPT: &str = include_str!("../prompts/image_generation_moderation_v1.txt");
const IMAGE_TEXT_PROMPT: &str =
    include_str!("../prompts/image_prompt_generation_moderation_v1.txt");
const TEXT_PROMPT: &str = include_str!("../prompts/text_moderation_v1.txt");

#[derive(OpenApi)]
#[openapi(paths(health::health))]
struct ApiDoc;

fn main() {
    // Load a local .env if present (no-op in prod where env vars come from compose/CI).
    let _ = dotenvy::dotenv();
    let settings = Arc::new(Settings::from_env().expect("failed to load settings"));

    // Observability MUST be installed before the async runtime: settings (DSN,
    // environment) load first, the guard lifetime is anchored to `main`, and the
    // subscriber + panic hook are in place before any task runs. Held to end of main.
    let _observability = nsfw_observability::init(&settings);

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
        .block_on(serve(settings));
}

async fn serve(settings: Arc<Settings>) {
    let http_client = reqwest::Client::new();

    tracing::info!(
        environment = %settings.environment,
        gpu_configured = settings.is_gpu_configured(),
        "nsfw-api starting"
    );

    let gpu_service: Option<Arc<GpuModerationService>> = if settings.is_gpu_configured() {
        let client = GpuOpenAiClient::new(
            http_client.clone(),
            settings
                .api_base_url
                .clone()
                .expect("checked by is_gpu_configured"),
            settings
                .api_key
                .as_ref()
                .expect("checked by is_gpu_configured")
                .expose_secret()
                .to_string(),
            settings
                .model_name
                .clone()
                .expect("checked by is_gpu_configured"),
        );
        let config = GpuModerationConfig {
            max_attempts: settings.gpu_max_attempts,
            retry_base_delay_seconds: settings.gpu_retry_base_delay_seconds,
            max_concurrency: settings.gpu_max_concurrency as usize,
        };
        Some(Arc::new(GpuModerationService::new(
            client,
            config,
            Some(IMAGE_PROMPT.to_string()),
            Some(IMAGE_TEXT_PROMPT.to_string()),
            Some(TEXT_PROMPT.to_string()),
        )))
    } else {
        None
    };

    let image_service = Arc::new(image_detection::ImageDetectionService::new(
        settings.clone(),
        gpu_service.clone(),
        http_client.clone(),
    ));
    let text_service = Arc::new(text_detection::TextDetectionService::new(
        gpu_service.clone(),
    ));

    let checks = health::ReadinessChecks {
        internal_auth: Arc::new({
            let settings = settings.clone();
            move || settings.internal_request_secret().is_some()
        }),
        // Phase 4 (data layer) wires postgres/kvrocks/clickhouse to real repository pings.
        postgres: Arc::new(|| false),
        kvrocks: Arc::new(|| false),
        clickhouse: Arc::new(|| false),
        gpu: Arc::new({
            let settings = settings.clone();
            move || settings.is_gpu_configured()
        }),
        ffmpeg: Arc::new(|| false),
        ffprobe: Arc::new(|| false),
    };

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

    let v1_router: Router = Router::new().merge(image_router).merge(text_router).layer(
        axum::middleware::from_fn_with_state(settings.clone(), auth::require_signed_request),
    );

    let app: Router = Router::new()
        .route("/health", get(health::health))
        .route("/ready", get(health::ready).with_state(checks))
        .nest("/v1", v1_router)
        .merge(SwaggerUi::new("/docs").url("/openapi.json", ApiDoc::openapi()))
        .fallback(fallback_404)
        .layer(axum::middleware::from_fn(request_id::request_id_middleware));

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .expect("failed to bind");
    tracing::info!(port, "listening");
    axum::serve(listener, app).await.expect("server error");
}

async fn fallback_404() -> axum::response::Response {
    use axum::response::IntoResponse;
    (
        axum::http::StatusCode::NOT_FOUND,
        axum::Json(serde_json::json!({"error": {"code": "404", "message": "Not Found"}})),
    )
        .into_response()
}
