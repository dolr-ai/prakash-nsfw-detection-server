# Rust NSFW Port — Phase 2: API Skeleton — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the `nsfw-api` axum binary — health/ready endpoints, the HMAC auth middleware, request-id middleware, the error envelope, and an OpenAPI/Swagger UI scaffold — Phase 2 of the design spec's 10-phase build order (§18).

**Architecture:** New `nsfw-api` binary crate depending on `nsfw-core` (domain/`AppError`) and `nsfw-config` (`Settings`), built with axum. `/v1` is mounted as an empty nested router with the HMAC middleware already applied, ready for Phase 4/5 to add real routes into — no real `/v1` endpoints exist yet, that's out of scope for this phase per spec §18.

**Tech Stack:** axum 0.8, tokio, tower, `hmac`/`sha2`/`hex` (HMAC-SHA256 signing/verification), `secrecy` (already in `nsfw-config`), `chrono`, `uuid` (request-id generation), `utoipa` 5 + `utoipa-swagger-ui` 9 (OpenAPI/Swagger UI — verified this specific version pairing resolves to a single axum 0.8 dependency graph; earlier utoipa-swagger-ui majors pull in axum 0.7 internally and produce a `Router<_>: From<SwaggerUi>` type error when merged with an axum-0.8 `Router`).

**Spec:** `docs/superpowers/specs/2026-07-21-rust-nsfw-detection-port-design.md` §9 (API Layer), §7.3 (error handling), §4 (tech stack). **Prior phase:** `docs/superpowers/plans/2026-07-21-rust-port-phase1-workspace-core-config.md` (merged to `main`) — this plan depends on `nsfw-core::{AppError, ErrorCode}` and `nsfw-config::Settings` existing already.

**Note on the spec's `IntoResponse for AppError` wording:** spec §7.3 says "`IntoResponse for AppError` produces the identical envelope." Read literally this is impossible — `AppError` is defined in `nsfw-core`, `IntoResponse` is defined in `axum`, and Rust's orphan rule forbids implementing a foreign trait for a foreign type from a third crate (`nsfw-api`). This plan uses the standard, idiomatic fix: a local newtype `ApiError(pub AppError)` in `nsfw-api`, with `impl IntoResponse for ApiError` (satisfies the orphan rule — the type is local) and `impl From<AppError> for ApiError` (so `?` on any `Result<T, AppError>`-returning call auto-converts in a handler that returns `Result<T, ApiError>`). Same wire behavior the spec describes; different, necessary Rust mechanics.

Every piece of code in this plan has already been verified end-to-end in a scratch workspace (built, `clippy -D warnings` clean, 11 integration tests passing against real axum request/response plumbing) before being written into this document — not just eyeballed. **Formatting note** (same convention as Phase 1): the code as transcribed into this plan's task-by-task order is not yet `rustfmt`-clean as a whole — `cargo fmt --all -- --check` on the fully assembled crate reports diffs (rustfmt wants `lib.rs`'s `pub mod` declarations reordered alphabetically, plus a few lines over the 100-char width). Run `cargo fmt --all` before every commit in this plan (Task 8 already accounts for this at the final gate), not a sign anything is wrong.

---

## File Structure

```
crates/
  nsfw-api/
    Cargo.toml
    src/
      main.rs           # binary entrypoint: builds Settings, wires the router, binds, serves
      lib.rs             # re-exports every module below (so main.rs and integration tests
                         #   both consume nsfw_api::{auth, error, health, request_id})
      error.rs           # ApiError newtype + IntoResponse impl (the error envelope)
      health.rs          # /health, /ready + ReadinessChecks (fakeable closures per dependency)
      request_id.rs      # x-request-id middleware
      auth.rs            # HMAC sign/verify + require_signed_request middleware
    tests/
      auth_test.rs                       # HMAC middleware, exercised through a real axum Router
      health_test.rs                     # /health, /ready, error-envelope shape
      request_id_and_fallback_test.rs    # request-id header behavior, 404 fallback shape
```

---

### Task 1: `nsfw-api` crate scaffold

**Files:**
- Create: `crates/nsfw-api/Cargo.toml`
- Create: `crates/nsfw-api/src/lib.rs`
- Create: `crates/nsfw-api/src/main.rs` (placeholder, replaced in Task 6)
- Modify: root `Cargo.toml` (no change needed — `members = ["crates/*"]` already picks this up)

- [ ] **Step 1: Create the crate manifest**

```toml
[package]
name = "nsfw-api"
edition.workspace = true
version.workspace = true

[dependencies]
nsfw-core = { path = "../nsfw-core" }
nsfw-config = { path = "../nsfw-config" }
tokio = { version = "1", features = ["full"] }
axum = "0.8.9"
tower = "0.5"
http = { workspace = true }
http-body-util = "0.1"
serde = { workspace = true }
serde_json = { workspace = true }
secrecy = { workspace = true }
chrono = { workspace = true }
hmac = "0.12"
sha2 = "0.10"
hex = "0.4"
uuid = { version = "1", features = ["v4"] }
utoipa = { version = "5", features = ["axum_extras"] }
utoipa-swagger-ui = { version = "9", features = ["axum"] }

[dev-dependencies]
tower = { version = "0.5", features = ["util"] }
```

- [ ] **Step 2: Create `lib.rs`** (modules added in later tasks)

```rust
// Modules added task-by-task in this plan.
```

- [ ] **Step 3: Create a placeholder `main.rs`**

```rust
fn main() {
    println!("nsfw-api placeholder");
}
```

- [ ] **Step 4: Verify the workspace still builds**

Run: `cargo build --workspace`
Expected: builds successfully, `nsfw-api` compiles as a new empty binary alongside the existing `nsfw-core`/`nsfw-config` libraries.

- [ ] **Step 5: Commit**

```bash
git add crates/nsfw-api/
git commit -m "chore: scaffold nsfw-api crate"
```

---

### Task 2: Error envelope — `ApiError`

**Files:**
- Create: `crates/nsfw-api/src/error.rs`
- Modify: `crates/nsfw-api/src/lib.rs`
- Test: `crates/nsfw-api/tests/health_test.rs` (this task only adds the envelope-shape test within it; the file is completed in Task 3)

- [ ] **Step 1: Write the failing test** (add to a new `tests/health_test.rs`)

```rust
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
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p nsfw-api --test health_test`
Expected: FAIL — `nsfw_api::error::ApiError` not defined.

- [ ] **Step 3: Implement**

```rust
// crates/nsfw-api/src/error.rs
use axum::Json;
use axum::response::{IntoResponse, Response};
use nsfw_core::AppError;
use serde::Serialize;

pub struct ApiError(pub AppError);

impl From<AppError> for ApiError {
    fn from(err: AppError) -> Self {
        Self(err)
    }
}

#[derive(Serialize)]
struct ErrorBody {
    code: String,
    message: String,
}

#[derive(Serialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.0.status;
        let body = ErrorEnvelope {
            error: ErrorBody {
                code: self.0.code.as_str().to_string(),
                message: self.0.message,
            },
        };
        (status, Json(body)).into_response()
    }
}
```

- [ ] **Step 4: Wire up the module**

```rust
// crates/nsfw-api/src/lib.rs
pub mod error;
```

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p nsfw-api --test health_test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/nsfw-api/src/error.rs crates/nsfw-api/src/lib.rs crates/nsfw-api/tests/health_test.rs
git commit -m "feat: add ApiError newtype producing the exact error envelope shape"
```

---

### Task 3: `/health` and `/ready`

**Files:**
- Create: `crates/nsfw-api/src/health.rs`
- Modify: `crates/nsfw-api/src/lib.rs`
- Modify: `crates/nsfw-api/tests/health_test.rs` (add the health/ready tests alongside Task 2's envelope test)

Per spec §9.1, `/ready` reports **seven** checks — `internal_auth`, `postgres`, `kvrocks`, `clickhouse`, `gpu`, `ffmpeg`, `ffprobe` — `200` only if all seven are ready, `503` otherwise. None of `postgres`/`kvrocks`/`clickhouse`/`gpu`/`ffmpeg`/`ffprobe` have real implementations yet (repositories/clients don't exist until Phase 3+), so `ReadinessChecks` takes injected closures — real checks get wired in later phases without touching this route's logic.

- [ ] **Step 1: Add the failing tests** (prepend to `tests/health_test.rs`, above the Task 2 test)

```rust
use nsfw_api::health::{self, ReadinessChecks};
use std::sync::Arc;

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
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn ready_returns_503_when_any_dependency_not_ready() {
    let app: Router = Router::new().route("/ready", get(health::ready).with_state(checks(false)));
    let response = app
        .oneshot(Request::builder().uri("/ready").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn ready_returns_200_when_all_dependencies_ready() {
    let app: Router = Router::new().route("/ready", get(health::ready).with_state(checks(true)));
    let response = app
        .oneshot(Request::builder().uri("/ready").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p nsfw-api --test health_test`
Expected: FAIL — `nsfw_api::health` module not defined.

- [ ] **Step 3: Implement**

```rust
// crates/nsfw-api/src/health.rs
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

#[derive(Serialize)]
pub struct ReadinessDependency {
    name: String,
    ready: bool,
}

#[derive(Serialize)]
pub struct ReadinessResponse {
    status: String,
    dependencies: Vec<ReadinessDependency>,
}

#[utoipa::path(get, path = "/health", responses((status = 200, description = "Liveness check")))]
pub async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}

pub async fn ready(State(checks): State<ReadinessChecks>) -> (StatusCode, Json<ReadinessResponse>) {
    let dependencies = vec![
        ReadinessDependency { name: "internal_auth".into(), ready: (checks.internal_auth)() },
        ReadinessDependency { name: "postgres".into(), ready: (checks.postgres)() },
        ReadinessDependency { name: "kvrocks".into(), ready: (checks.kvrocks)() },
        ReadinessDependency { name: "clickhouse".into(), ready: (checks.clickhouse)() },
        ReadinessDependency { name: "gpu".into(), ready: (checks.gpu)() },
        ReadinessDependency { name: "ffmpeg".into(), ready: (checks.ffmpeg)() },
        ReadinessDependency { name: "ffprobe".into(), ready: (checks.ffprobe)() },
    ];
    let all_ready = dependencies.iter().all(|d| d.ready);
    let status = if all_ready { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    let body = ReadinessResponse {
        status: if all_ready { "ready".into() } else { "not_ready".into() },
        dependencies,
    };
    (status, Json(body))
}
```

`#[utoipa::path(...)]` on `health` is required, not decorative — `utoipa::OpenApi`'s `#[openapi(paths(health::health))]` derive (Task 6) looks for a generated `__path_health` item that only exists once this attribute is applied. Omitting it produces a confusing `could not find __path_health in health` compile error.

- [ ] **Step 4: Wire up the module**

```rust
// crates/nsfw-api/src/lib.rs (append)
pub mod health;
```

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test -p nsfw-api --test health_test`
Expected: PASS (4 tests: 3 from this task + Task 2's envelope test).

- [ ] **Step 6: Commit**

```bash
git add crates/nsfw-api/src/health.rs crates/nsfw-api/src/lib.rs crates/nsfw-api/tests/health_test.rs
git commit -m "feat: add /health and /ready with fakeable ReadinessChecks"
```

---

### Task 4: Request-ID middleware

**Files:**
- Create: `crates/nsfw-api/src/request_id.rs`
- Modify: `crates/nsfw-api/src/lib.rs`
- Create: `crates/nsfw-api/tests/request_id_and_fallback_test.rs` (this task adds the request-id tests; Task 5 adds the 404-fallback test to the same file)

Per spec §9.1: every response gets `x-request-id` — echoed back if the request supplied one, generated (`Uuid::new_v4()`) otherwise. Ported as response-header-only, matching Python's current behavior exactly (no log-correlation threading).

- [ ] **Step 1: Write the failing tests**

```rust
// crates/nsfw-api/tests/request_id_and_fallback_test.rs
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
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
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
    assert_eq!(response.headers().get(REQUEST_ID_HEADER).unwrap(), "my-trace-id");
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p nsfw-api --test request_id_and_fallback_test`
Expected: FAIL — `nsfw_api::request_id` not defined.

- [ ] **Step 3: Implement**

```rust
// crates/nsfw-api/src/request_id.rs
use axum::extract::Request;
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;

pub const REQUEST_ID_HEADER: &str = "x-request-id";

pub async fn request_id_middleware(req: Request, next: Next) -> Response {
    let incoming = req
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let mut response = next.run(req).await;

    let id = incoming.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    if let Ok(value) = HeaderValue::from_str(&id) {
        response.headers_mut().insert(REQUEST_ID_HEADER, value);
    }
    response
}
```

- [ ] **Step 4: Wire up the module**

```rust
// crates/nsfw-api/src/lib.rs (append)
pub mod request_id;
```

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test -p nsfw-api --test request_id_and_fallback_test`
Expected: PASS (2 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/nsfw-api/src/request_id.rs crates/nsfw-api/src/lib.rs crates/nsfw-api/tests/request_id_and_fallback_test.rs
git commit -m "feat: add x-request-id middleware"
```

---

### Task 5: 404 fallback shape

**Files:**
- Modify: `crates/nsfw-api/tests/request_id_and_fallback_test.rs`

Per spec §7.3: an unmatched route (no handler at all — not an `AppError`) must produce `{"error":{"code":"404","message":"Not Found"}}` — `code` is the **literal status-code-as-string**, not a semantic code, unlike every other error response in this service. This is a standalone axum `Router::fallback` handler, deliberately not routed through `ApiError`/`AppError` at all, since Python's equivalent (Starlette's framework-level 404) never touches `AppError` either.

- [ ] **Step 1: Write the failing test** (append to the same test file)

```rust
use axum::response::IntoResponse;
use http_body_util::BodyExt;

async fn fallback_404() -> axum::response::Response {
    (
        StatusCode::NOT_FOUND,
        axum::Json(serde_json::json!({"error": {"code": "404", "message": "Not Found"}})),
    )
        .into_response()
}

#[tokio::test]
async fn unmatched_route_returns_literal_status_code_string_shape() {
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
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p nsfw-api --test request_id_and_fallback_test unmatched_route`
Expected: FAIL — `fallback_404` not yet wired to a router in this test, or route matches unexpectedly. (This is a same-file test-only addition; there's no separate "implementation" step because the fallback handler here IS the test's own local function — Task 6 promotes an equivalent handler into `main.rs` for the real binary.)

- [ ] **Step 3: Run to verify it passes**

Run: `cargo test -p nsfw-api --test request_id_and_fallback_test`
Expected: PASS (3 tests total in this file).

- [ ] **Step 4: Commit**

```bash
git add crates/nsfw-api/tests/request_id_and_fallback_test.rs
git commit -m "test: verify the literal-status-code-as-string 404 fallback shape"
```

---

### Task 6: HMAC auth middleware

**Files:**
- Create: `crates/nsfw-api/src/auth.rs`
- Modify: `crates/nsfw-api/src/lib.rs`
- Create: `crates/nsfw-api/tests/auth_test.rs`

This is the one genuinely tricky part (spec §9.4): the signature covers `SHA256(raw_body)`, but axum's request body is a one-shot stream — read once via `axum::body::to_bytes` for verification, then the request must be reconstructed with those same bytes so the eventual handler can still read the body. Header names, skew constant, and the deliberate non-disclosure quirk (missing-secret and bad-signature return the identical code) are ported exactly per spec §9.4.

- [ ] **Step 1: Write the failing tests**

```rust
// crates/nsfw-api/tests/auth_test.rs
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware;
use axum::routing::get;
use nsfw_api::auth::{SIGNATURE_HEADER, TIMESTAMP_HEADER, build_signature_message, require_signed_request};
use nsfw_config::Settings;
use std::collections::HashMap;
use std::sync::Arc;
use tower::ServiceExt;

fn settings_with_secret(secret: &str) -> Arc<Settings> {
    let mut vars = HashMap::new();
    vars.insert("INTERNAL_REQUEST_HMAC_SECRET".to_string(), secret.to_string());
    Arc::new(Settings::from_map(&vars).unwrap())
}

fn protected_app(settings: Arc<Settings>) -> Router {
    Router::new()
        .route("/protected", get(|| async { "ok" }))
        .layer(middleware::from_fn_with_state(settings, require_signed_request))
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
        .oneshot(Request::builder().uri("/protected").body(Body::empty()).unwrap())
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
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p nsfw-api --test auth_test`
Expected: FAIL — `nsfw_api::auth` not defined.

- [ ] **Step 3: Implement**

```rust
// crates/nsfw-api/src/auth.rs
use axum::body::Body;
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use hmac::{Hmac, Mac};
use nsfw_config::Settings;
use nsfw_core::{AppError, ErrorCode};
use secrecy::ExposeSecret;
use sha2::{Digest, Sha256};
use std::sync::Arc;

use crate::error::ApiError;

type HmacSha256 = Hmac<Sha256>;

pub const TIMESTAMP_HEADER: &str = "x-internal-timestamp";
pub const SIGNATURE_HEADER: &str = "x-internal-signature";

pub fn body_sha256_hex(body: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body);
    hex::encode(hasher.finalize())
}

pub fn build_signature_message(timestamp: &str, method: &str, path: &str, body: &[u8]) -> Vec<u8> {
    format!("{}\n{}\n{}\n{}", timestamp, method.to_uppercase(), path, body_sha256_hex(body)).into_bytes()
}

pub fn signature_has_valid_shape(signature: &str) -> bool {
    signature.len() == 64 && signature.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn verify_signature(secret: &str, timestamp: &str, method: &str, path: &str, body: &[u8], signature: &str) -> bool {
    if !signature_has_valid_shape(signature) {
        return false;
    }
    let message = build_signature_message(timestamp, method, path, body);
    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(&message);
    let Ok(sig_bytes) = hex::decode(signature) else {
        return false;
    };
    mac.verify_slice(&sig_bytes).is_ok()
}

pub async fn require_signed_request(
    State(settings): State<Arc<Settings>>,
    req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let (parts, body) = req.into_parts();
    let headers = parts.headers.clone();

    let timestamp_raw = headers.get(TIMESTAMP_HEADER).and_then(|v| v.to_str().ok()).map(str::to_string);
    let signature = headers.get(SIGNATURE_HEADER).and_then(|v| v.to_str().ok()).map(str::to_string);

    let (Some(timestamp_raw), Some(signature)) = (timestamp_raw, signature) else {
        return Err(ApiError::from(AppError::new(ErrorCode::AuthMissingHeaders, "missing internal auth headers")));
    };

    // Deliberately the same code/message as a bad signature -- avoids leaking whether
    // the secret is configured at all. Matches Python's AuthService exactly (spec §9.4).
    let secret = match settings.internal_request_secret() {
        Some(s) => s,
        None => return Err(ApiError::from(AppError::new(ErrorCode::AuthBadSignature, "invalid internal signature"))),
    };

    let timestamp: i64 = timestamp_raw
        .parse()
        .map_err(|_| ApiError::from(AppError::new(ErrorCode::AuthBadTimestamp, "timestamp must be unix seconds")))?;

    let now = chrono::Utc::now().timestamp();
    if (now - timestamp).abs() > settings.internal_request_max_skew_sec {
        return Err(ApiError::from(AppError::new(ErrorCode::AuthTimestampOutOfRange, "stale internal request timestamp")));
    }

    let body_bytes = axum::body::to_bytes(body, usize::MAX)
        .await
        .map_err(|_| ApiError::from(AppError::new(ErrorCode::ValidationError, "failed to read request body")))?;

    let secret_str = secret.expose_secret();
    if !verify_signature(secret_str, &timestamp_raw, parts.method.as_str(), parts.uri.path(), &body_bytes, &signature) {
        return Err(ApiError::from(AppError::new(ErrorCode::AuthBadSignature, "invalid internal signature")));
    }

    let reconstructed = Request::from_parts(parts, Body::from(body_bytes));
    Ok(next.run(reconstructed).await)
}
```

Note: `signature_has_valid_shape` is checked **inside** `verify_signature`, before the HMAC comparison — matching spec §9.4's "reject malformed signatures before constant-time comparison" rule. `mac.verify_slice(...)` (from the `hmac` crate's `Mac` trait) already performs the comparison in constant time internally, so no separate `subtle`/`ring::constant_time` dependency is needed — one less dependency than the spec's tech-stack table speculated.

- [ ] **Step 4: Wire up the module**

```rust
// crates/nsfw-api/src/lib.rs (append)
pub mod auth;
```

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test -p nsfw-api --test auth_test`
Expected: PASS (4 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/nsfw-api/src/auth.rs crates/nsfw-api/src/lib.rs crates/nsfw-api/tests/auth_test.rs
git commit -m "feat: port HMAC request-signing middleware"
```

---

### Task 7: Wire it all together — real `main.rs` with OpenAPI

**Files:**
- Modify: `crates/nsfw-api/src/main.rs`

- [ ] **Step 1: Replace the placeholder with the real binary**

```rust
// crates/nsfw-api/src/main.rs
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
    let v1_router: Router = Router::new()
        .layer(axum::middleware::from_fn_with_state(settings.clone(), auth::require_signed_request));

    let app: Router = Router::new()
        .route("/health", get(health::health))
        .route("/ready", get(health::ready).with_state(checks))
        .nest("/v1", v1_router)
        .merge(SwaggerUi::new("/docs").url("/openapi.json", ApiDoc::openapi()))
        .fallback(fallback_404)
        .layer(axum::middleware::from_fn(request_id::request_id_middleware));

    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8080);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await.expect("failed to bind");
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
```

- [ ] **Step 2: Verify it builds**

Run: `cargo build -p nsfw-api`
Expected: builds successfully.

- [ ] **Step 3: Smoke-test it manually**

```bash
INTERNAL_REQUEST_HMAC_SECRET=dev-secret PORT=8080 cargo run -p nsfw-api &
sleep 1
curl -s http://127.0.0.1:8080/health
curl -s http://127.0.0.1:8080/ready
curl -s http://127.0.0.1:8080/openapi.json | head -c 200
curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:8080/docs
kill %1
```

Expected: `/health` → `{"status":"ok"}`; `/ready` → `503` with all 7 dependencies `false` except possibly `internal_auth` (`true`, since the secret is set); `/openapi.json` → valid OpenAPI JSON starting with `{"openapi":...`; `/docs` → `303` redirecting to `/docs/` (curl without `-L` shows the redirect; `curl -sL` or hitting `/docs/` directly returns `200` with the Swagger UI HTML) — this is `utoipa-swagger-ui`'s own bare-path-redirect behavior, not a bug.

- [ ] **Step 4: Commit**

```bash
git add crates/nsfw-api/src/main.rs
git commit -m "feat: wire nsfw-api binary — router, OpenAPI, request-id, HMAC-gated /v1"
```

---

### Task 8: Phase 2 completion check

**Files:** none (verification only)

- [ ] **Step 1: Full workspace check**

Run: `cargo fmt --all -- --check` (run `cargo fmt --all` first if it fails, then re-check)
Run: `cargo clippy --workspace --all-targets -- -D warnings`
Run: `cargo test --workspace`

Expected: all clean. `nsfw-api` contributes 11 tests (4 auth + 4 health/error-envelope + 3 request-id/fallback) on top of Phase 1's 92, for 103 total across the workspace.

- [ ] **Step 2: Completion note**

- Commands run: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
- Known gaps carried forward on purpose: `/ready`'s six non-`internal_auth` checks are hardcoded `false` — they get real implementations once Phase 3 (repositories/clients) exists. `/v1` has no real routes yet (Phase 4/5). No Sentry integration yet (not in scope until a later phase per spec §4's tech-stack table). No graceful-shutdown signal handling on this binary yet — `nsfw-api` is a request/response server without the long-running-worker concerns that make spec §10's shutdown design necessary for `video-worker`/`flush-worker`; add it here later only if operationally needed.

- [ ] **Step 3: Final commit (if any formatting fixes were needed)**

```bash
git add -A
git commit -m "chore: phase 2 completion — fmt/clippy/test all green"
```

---

## What's Next

This plan covers Phase 2 only (spec §18). **Note:** the spec's phase order was later revised — the data layer phase (Postgres/ClickHouse/KVRocks repositories, migrations, DDL incl. the `excluded_videos` gap and the 39-vs-45-column reconciliation, in-memory fakes) moved from position 3 to position 4, deferred behind stateless endpoints (now position 3) since it has a real external gate (live-infra access) that isn't available yet. See spec §18 for the current order and rationale.
