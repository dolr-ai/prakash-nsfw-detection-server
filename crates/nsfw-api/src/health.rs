use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Serialize;
use std::sync::Arc;

#[derive(Clone)]
pub struct ReadinessChecks {
    pub internal_auth: Arc<dyn Fn() -> bool + Send + Sync>,
    pub postgres: Arc<dyn Fn() -> bool + Send + Sync>,
    pub kvrocks: Arc<dyn Fn() -> bool + Send + Sync>,
    pub clickhouse: Arc<dyn Fn() -> bool + Send + Sync>,
    pub gpu: Arc<dyn Fn() -> bool + Send + Sync>,
    pub ffmpeg: Arc<dyn Fn() -> bool + Send + Sync>,
    pub ffprobe: Arc<dyn Fn() -> bool + Send + Sync>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ReadinessDependency {
    name: String,
    ready: bool,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ReadinessResponse {
    status: String,
    dependencies: Vec<ReadinessDependency>,
}

#[utoipa::path(get, path = "/health", responses((status = 200, description = "Liveness check")))]
pub async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}

#[utoipa::path(
    get, path = "/ready",
    responses(
        (status = 200, description = "All dependencies ready", body = ReadinessResponse),
        (status = 503, description = "One or more dependencies not ready", body = ReadinessResponse),
    )
)]
pub async fn ready(State(checks): State<ReadinessChecks>) -> (StatusCode, Json<ReadinessResponse>) {
    let dependencies = vec![
        ReadinessDependency {
            name: "internal_auth".into(),
            ready: (checks.internal_auth)(),
        },
        ReadinessDependency {
            name: "postgres".into(),
            ready: (checks.postgres)(),
        },
        ReadinessDependency {
            name: "kvrocks".into(),
            ready: (checks.kvrocks)(),
        },
        ReadinessDependency {
            name: "clickhouse".into(),
            ready: (checks.clickhouse)(),
        },
        ReadinessDependency {
            name: "gpu".into(),
            ready: (checks.gpu)(),
        },
        ReadinessDependency {
            name: "ffmpeg".into(),
            ready: (checks.ffmpeg)(),
        },
        ReadinessDependency {
            name: "ffprobe".into(),
            ready: (checks.ffprobe)(),
        },
    ];
    let all_ready = dependencies.iter().all(|d| d.ready);
    let status = if all_ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let body = ReadinessResponse {
        status: if all_ready {
            "ready".into()
        } else {
            "not_ready".into()
        },
        dependencies,
    };
    (status, Json(body))
}
