use axum::Router;
use axum::routing::get;
use nsfw_api::{auth, health, request_id};
use nsfw_config::Settings;
use std::sync::Arc;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[derive(OpenApi)]
#[openapi(paths(health::health))]
struct ApiDoc;

#[tokio::main]
async fn main() {
    let settings = Arc::new(Settings::from_env().expect("failed to load settings"));

    let checks = health::ReadinessChecks {
        internal_auth: Arc::new({
            let settings = settings.clone();
            move || settings.internal_request_secret().is_some()
        }),
        // Phase 3 wires these to real repository/client pings; nothing to check against yet.
        postgres: Arc::new(|| false),
        kvrocks: Arc::new(|| false),
        clickhouse: Arc::new(|| false),
        gpu: Arc::new(|| false),
        ffmpeg: Arc::new(|| false),
        ffprobe: Arc::new(|| false),
    };

    // Empty for now -- no real /v1 routes exist until Phase 4/5. The HMAC middleware
    // is applied at the router level here so future routes nested under it are
    // automatically gated, matching spec §9.1's router-level (not per-route) auth.
    let v1_router: Router = Router::new().layer(axum::middleware::from_fn_with_state(
        settings.clone(),
        auth::require_signed_request,
    ));

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
