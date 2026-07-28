# Observability (tracing + Sentry) & OpenAPI Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add structured `tracing` logging + Sentry error reporting (at Python parity, plus per-request/per-call spans and request correlation) and complete the OpenAPI docs, without changing any existing HTTP behavior.

**Architecture:** `tracing` is the single instrumentation interface. Service/API crates emit structured events + spans; a new `nsfw-observability` crate owns Sentry + subscriber wiring so only the binary knows about Sentry. Content is confined to `debug!` (which the sentry-tracing layer ignores), making content→Sentry structurally impossible. One `http.request` span is owned by the request-id middleware; error/GPU/download events emitted inside it inherit `request_id`/`method`/`path` via sentry-tracing span propagation.

**Tech Stack:** Rust 2024, axum 0.8, `tracing` + `tracing-subscriber` (env-filter, fmt/json), `sentry` 0.32 + `sentry-tracing` 0.32, `utoipa` 5, `tracing-test` (dev).

**Spec:** [docs/superpowers/specs/2026-07-28-observability-and-api-docs-design.md](2026-07-28-observability-and-api-docs-design.md)

---

## Conventions for every task

- Run `cargo fmt` before each commit; run `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` in the final verify task (and whenever a task says so).
- Commit messages: conventional commits, no attribution footer (repo convention).
- Work on a feature branch off `origin/main` — never commit to `main` directly.

## File structure (what each new/changed file owns)

- **`crates/nsfw-observability/`** (new crate) — process-wide observability wiring.
  - `src/redact.rs` — `SafeUrl` + `safe_url(&str) -> SafeUrl`: URL reduced to `{scheme, host, path[:160], port}`. Pure, no deps beyond `url`.
  - `src/init.rs` — `init(&Settings) -> ObservabilityGuard`: installs Sentry (DSN-gated) + the tracing subscriber stack; returns a guard held for process lifetime.
  - `src/lib.rs` — re-exports `init`, `ObservabilityGuard`, `safe_url`, `SafeUrl`.
- **`crates/nsfw-services/src/gpu_moderation.rs`** — gains a `gpu.moderate` span, `warn!` per failed attempt, `error!` on give-up, `info!` on success, and a private `classify_error` mapping `GpuModerationError` → `(error_kind, error_code)`.
- **`crates/nsfw-api/src/request_id.rs`** — reworked: id generated/extracted at entry, stored in a request extension, opens the single `http.request` span, echoes id on response.
- **`crates/nsfw-api/src/image_detection.rs`** — `image.download` span with redacted URL fields; `warn!`/`error!` on failures; raw URL only at `debug!`.
- **`crates/nsfw-api/src/error.rs`** — `error!` on 5xx (own fields `error_code`,`http_status`), `debug!` on 4xx.
- **`crates/nsfw-api/src/main.rs`** — plain `fn main`, observability init before the tokio runtime, startup `info!`, extended `#[openapi(...)]`, no `TraceLayer`.
- **`crates/nsfw-api/src/health.rs`**, **`moderation_routes.rs`**, **`crates/nsfw-core/src/model_output.rs`** — `#[utoipa::path]` + `ToSchema` derives for OpenAPI completeness.

---

## Task 1: Scaffold `nsfw-observability` crate + URL redaction (TDD)

**Files:**
- Create: `crates/nsfw-observability/Cargo.toml`
- Create: `crates/nsfw-observability/src/lib.rs`
- Create: `crates/nsfw-observability/src/redact.rs`

- [ ] **Step 1: Create the crate manifest**

`crates/nsfw-observability/Cargo.toml`:

```toml
[package]
name = "nsfw-observability"
edition.workspace = true
version.workspace = true

[dependencies]
nsfw-config = { path = "../nsfw-config" }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
sentry = "0.32"
sentry-tracing = "0.32"
secrecy = { workspace = true }
url = "2"

[dev-dependencies]
rstest = { workspace = true }
```

- [ ] **Step 2: Add workspace dep entries** (so `tracing`/`tracing-subscriber` versions are shared)

In root `Cargo.toml` under `[workspace.dependencies]`, append:

```toml
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json", "fmt"] }
```

- [ ] **Step 3: Write the failing redaction tests**

`crates/nsfw-observability/src/redact.rs`:

```rust
//! Redacts URLs to a safe, loggable subset: never query strings (which carry signed-URL
//! credentials), userinfo, or fragments. Mirrors Python's `_safe_url_context`.

use url::Url;

/// The only URL representation allowed at INFO+ / Sentry. Raw URLs stay at `debug!`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SafeUrl {
    pub scheme: String,
    pub host: String,
    pub path: String,
    pub port: Option<u16>,
}

/// Path is truncated to 160 chars to bound log size (Python parity). Unparseable input
/// yields an all-empty `SafeUrl` rather than leaking the raw string.
pub fn safe_url(raw: &str) -> SafeUrl {
    match Url::parse(raw) {
        Ok(url) => {
            let mut path = url.path().to_string();
            if path.len() > 160 {
                path.truncate(160);
            }
            SafeUrl {
                scheme: url.scheme().to_string(),
                host: url.host_str().unwrap_or_default().to_string(),
                path,
                port: url.port(),
            }
        }
        Err(_) => SafeUrl::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn drops_query_userinfo_and_fragment() {
        let s = safe_url("https://user:pw@cdn.example.com:8443/a/b.jpg?sig=SECRET#frag");
        assert_eq!(s.scheme, "https");
        assert_eq!(s.host, "cdn.example.com");
        assert_eq!(s.path, "/a/b.jpg");
        assert_eq!(s.port, Some(8443));
        // Nothing secret survives.
        let rendered = format!("{s:?}");
        assert!(!rendered.contains("SECRET"));
        assert!(!rendered.contains("pw"));
    }

    #[test]
    fn truncates_long_path_to_160_chars() {
        let long = "a".repeat(500);
        let s = safe_url(&format!("https://h.example/{long}"));
        assert_eq!(s.path.len(), 160);
    }

    #[rstest]
    #[case("not a url")]
    #[case("")]
    #[case("ftp:::::broken")]
    fn unparseable_input_yields_empty_safe_url(#[case] input: &str) {
        assert_eq!(safe_url(input), SafeUrl::default());
    }

    #[test]
    fn missing_port_is_none() {
        let s = safe_url("https://h.example/x");
        assert_eq!(s.port, None);
    }
}
```

`crates/nsfw-observability/src/lib.rs` (init module added in Task 2):

```rust
pub mod redact;

pub use redact::{SafeUrl, safe_url};
```

- [ ] **Step 4: Add the crate to the workspace build & run tests**

The workspace uses `members = ["crates/*"]`, so the new crate is picked up automatically.

Run: `cargo test -p nsfw-observability`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/nsfw-observability/Cargo.toml crates/nsfw-observability/src/lib.rs crates/nsfw-observability/src/redact.rs Cargo.toml
git commit -m "feat(observability): add nsfw-observability crate with URL redaction"
```

---

## Task 2: Observability `init()` + guard

**Files:**
- Create: `crates/nsfw-observability/src/init.rs`
- Modify: `crates/nsfw-observability/src/lib.rs`

- [ ] **Step 1: Write `init.rs`**

`crates/nsfw-observability/src/init.rs`:

```rust
//! Installs the process-wide Sentry client + tracing subscriber stack. Call ONCE from
//! `main`, before the async runtime starts, and hold the returned guard for the whole
//! process so buffered Sentry events flush on shutdown.

use nsfw_config::Settings;
use secrecy::ExposeSecret;
use sentry::ClientInitGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// Held for process lifetime. When `sentry_dsn` is unset the inner guard is `None`
/// (logging still works, Sentry is inert) — matching Python's no-DSN behavior.
#[must_use = "hold the guard for the process lifetime so Sentry events flush on exit"]
pub struct ObservabilityGuard {
    _sentry: Option<ClientInitGuard>,
}

pub fn init(settings: &Settings) -> ObservabilityGuard {
    // 1) Sentry — DSN-gated. Installs the default integrations, including the panic hook.
    let sentry_guard = settings.sentry_dsn.as_ref().map(|dsn| {
        sentry::init((
            dsn.expose_secret().to_string(),
            sentry::ClientOptions {
                release: sentry::release_name!(),
                environment: Some(settings.environment.clone().into()),
                send_default_pii: settings.sentry_send_default_pii,
                ..Default::default()
            },
        ))
    });

    // 2) Subscriber stack. EnvFilter (RUST_LOG, default info) + JSON fmt (-> journald) +
    //    sentry-tracing (default EventFilter: ERROR->event, WARN/INFO->breadcrumb,
    //    DEBUG/TRACE->ignored — this is the content->Sentry barrier, see spec §2/§4).
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer().json())
        .with(sentry_tracing::layer())
        .init();

    ObservabilityGuard {
        _sentry: sentry_guard,
    }
}
```

- [ ] **Step 2: Re-export from `lib.rs`**

`crates/nsfw-observability/src/lib.rs`:

```rust
pub mod init;
pub mod redact;

pub use init::{ObservabilityGuard, init};
pub use redact::{SafeUrl, safe_url};
```

- [ ] **Step 3: Build (no unit test — global subscriber init can't be re-run per test)**

Run: `cargo build -p nsfw-observability`
Expected: compiles clean.

> Note: `init()` is deliberately not unit-tested — `.init()` installs a *global* default subscriber and would panic if called by multiple tests. It's exercised end-to-end by the binary and the manual Sentry panic verification (spec §6). The testable logic (redaction) lives in Task 1.

- [ ] **Step 4: Commit**

```bash
git add crates/nsfw-observability/src/init.rs crates/nsfw-observability/src/lib.rs
git commit -m "feat(observability): add init() installing Sentry + tracing subscriber stack"
```

---

## Task 3: Wire `tracing` into services/api and depend on the new crate

**Files:**
- Modify: `crates/nsfw-services/Cargo.toml`
- Modify: `crates/nsfw-api/Cargo.toml`

- [ ] **Step 1: Add `tracing` to `nsfw-services`**

In `crates/nsfw-services/Cargo.toml` `[dependencies]`, add:

```toml
tracing = { workspace = true }
```

And in `[dev-dependencies]`, add (for Task 6 capture assertions):

```toml
tracing-test = "0.2"
```

- [ ] **Step 2: Add `tracing` + `nsfw-observability` to `nsfw-api`**

In `crates/nsfw-api/Cargo.toml` `[dependencies]`, add:

```toml
tracing = { workspace = true }
nsfw-observability = { path = "../nsfw-observability" }
```

And in `[dev-dependencies]`, add:

```toml
tracing-test = "0.2"
```

- [ ] **Step 3: Build to confirm resolution**

Run: `cargo build -p nsfw-services -p nsfw-api`
Expected: compiles clean (deps unused for now — that's fine, no `deny(warnings)` on unused crate deps).

- [ ] **Step 4: Commit**

```bash
git add crates/nsfw-services/Cargo.toml crates/nsfw-api/Cargo.toml
git commit -m "chore: wire tracing + nsfw-observability into services and api crates"
```

---

## Task 4: Rework request-id middleware — id at entry + request span (TDD)

**Files:**
- Modify: `crates/nsfw-api/src/request_id.rs`
- Test: `crates/nsfw-api/tests/request_id_and_fallback_test.rs` (extend)

- [ ] **Step 1: Write the failing test — id is available *during* handling**

Append to `crates/nsfw-api/tests/request_id_and_fallback_test.rs` a test that mounts a tiny handler reading the id from the request extension and echoing it in the body, then asserts the body id equals the `x-request-id` response header. (Import the extension type the middleware sets — `nsfw_api::request_id::RequestId`.)

```rust
use axum::{routing::get, Router, extract::Extension};
use nsfw_api::request_id::{request_id_middleware, RequestId, REQUEST_ID_HEADER};
use tower::ServiceExt; // oneshot

#[tokio::test]
async fn request_id_is_available_to_handlers_during_request() {
    async fn echo_id(Extension(id): Extension<RequestId>) -> String {
        id.0
    }
    let app = Router::new()
        .route("/echo", get(echo_id))
        .layer(axum::middleware::from_fn(request_id_middleware));

    let resp = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/echo")
                .header(REQUEST_ID_HEADER, "abc-123")
                .body(axum::body::Body::empty())
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
    assert_eq!(body_id, "abc-123", "handler must see the same id the response carries");
}
```

- [ ] **Step 2: Run it — fails to compile (`RequestId` doesn't exist yet)**

Run: `cargo test -p nsfw-api --test request_id_and_fallback_test request_id_is_available_to_handlers_during_request`
Expected: FAIL (compile error: no `RequestId`).

- [ ] **Step 3: Rewrite `request_id.rs`**

```rust
use axum::extract::Request;
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;
use tracing::Instrument;

pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// Correlation id, stored in the request's extensions so handlers and `ApiError` can read
/// it, and set as a field on the `http.request` span so every downstream event carries it.
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

pub async fn request_id_middleware(mut req: Request, next: Next) -> Response {
    // 1) Extract or generate AT ENTRY (was previously generated at response time — too
    //    late for any event emitted during handling to be correlated).
    let id = req
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let method = req.method().to_string();
    let path = req.uri().path().to_string();

    // 2) Make it visible to handlers/ApiError.
    req.extensions_mut().insert(RequestId(id.clone()));

    // 3) One span, owned here (no TraceLayer). `status`/`latency_ms` are recorded at close.
    //    Fields declared with `tracing::field::Empty` are filled in after the call.
    let span = tracing::info_span!(
        "http.request",
        method = %method,
        path = %path,
        request_id = %id,
        status = tracing::field::Empty,
        latency_ms = tracing::field::Empty,
    );

    let start = std::time::Instant::now();
    // 4) Run downstream INSIDE the span so its fields propagate to Sentry events.
    let mut response = next.run(req).instrument(span.clone()).await;

    span.record("status", response.status().as_u16());
    span.record("latency_ms", start.elapsed().as_millis() as u64);
    span.in_scope(|| {
        tracing::info!("request completed");
    });

    // 5) Echo the id (unchanged behavior).
    if let Ok(value) = HeaderValue::from_str(&id) {
        response.headers_mut().insert(REQUEST_ID_HEADER, value);
    }
    response
}
```

- [ ] **Step 4: Run the new test + the existing request-id tests**

Run: `cargo test -p nsfw-api --test request_id_and_fallback_test`
Expected: PASS (new test + all pre-existing tests in the file). If a pre-existing test asserted id was *generated* only on the response with no incoming header, it still holds (generation path unchanged).

- [ ] **Step 5: Commit**

```bash
git add crates/nsfw-api/src/request_id.rs crates/nsfw-api/tests/request_id_and_fallback_test.rs
git commit -m "feat(api): generate request id at entry and open http.request span"
```

---

## Task 5: Restructure `main` — init before runtime, startup log, drop `#[tokio::main]`

**Files:**
- Modify: `crates/nsfw-api/src/main.rs`

- [ ] **Step 1: Replace the `#[tokio::main]` entrypoint**

Change the top of `main.rs` — remove `#[tokio::main] async fn main()`, add a plain `fn main()` that loads settings, inits observability, then builds the runtime. Move the existing async body into `async fn serve(settings: Arc<Settings>)`.

Replace lines 21–25 (`#[tokio::main] async fn main() { ... let settings = ...; let http_client ...`) through the end of the current `main` body with:

```rust
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
```

Everything from `let gpu_service ...` down to `axum::serve(...)` stays, but now lives inside `serve`. The `settings` binding is the function parameter (delete the old `let settings = Arc::new(...)` line inside the async body — it's now created in `main`).

- [ ] **Step 2: Add the startup `info!`**

Immediately after `let http_client = reqwest::Client::new();` inside `serve`, add:

```rust
    tracing::info!(
        environment = %settings.environment,
        gpu_configured = settings.is_gpu_configured(),
        "nsfw-api starting"
    );
```

And just before `axum::serve(listener, app)`, add the bound-port log:

```rust
    tracing::info!(port, "listening");
```

(`port` is already in scope from the existing `let port: u16 = ...`.)

- [ ] **Step 3: Build**

Run: `cargo build -p nsfw-api`
Expected: compiles clean. (No `TraceLayer` added — the request span is owned by the request-id middleware from Task 4, which `main` already layers on.)

- [ ] **Step 4: Smoke the existing integration tests still pass**

Run: `cargo test -p nsfw-api`
Expected: PASS (route/auth/health/request-id tests unaffected).

- [ ] **Step 5: Commit**

```bash
git add crates/nsfw-api/src/main.rs
git commit -m "feat(api): init observability before runtime and log startup"
```

---

## Task 6: GPU moderation instrumentation (TDD)

**Files:**
- Modify: `crates/nsfw-services/src/gpu_moderation.rs`

- [ ] **Step 1: Write the failing capture test**

Add to the `#[cfg(test)] mod tests` in `gpu_moderation.rs` a `#[traced_test]` test (from `tracing-test`) asserting that a persistently-malformed response produces a `warn!` per non-final attempt and an `error!` on give-up, carrying `error_kind`/`error_code`. Add `use tracing_test::traced_test;` in the test module.

```rust
    #[traced_test]
    #[tokio::test]
    async fn emits_warn_per_retry_and_error_on_giveup() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{"message": {"content": "not json"}}]
            })))
            .mount(&server)
            .await;
        let service = service_with_mock(&server, 3).await;
        assert!(service.moderate_text("hello").await.is_err());

        // 2 non-final attempts -> warn; final give-up -> error.
        assert!(logs_contain("error_kind=\"model_output\""));
        assert!(logs_contain("retry_remaining=true"));
        // give-up event names max_attempts
        assert!(logs_contain("max_attempts=3"));
    }
```

- [ ] **Step 2: Run — fails (no such log fields emitted yet)**

Run: `cargo test -p nsfw-services emits_warn_per_retry_and_error_on_giveup`
Expected: FAIL (assertions on log content).

- [ ] **Step 3: Add the classifier + instrument both retry loops**

Add a private free function near the bottom of `gpu_moderation.rs`:

```rust
/// Maps an error to stable `(error_kind, error_code)` strings so Sentry groups issues
/// consistently. Spec §3.B. No content, no secrets — variant/HTTP status only.
fn classify_error(err: &GpuModerationError) -> (&'static str, String) {
    match err {
        GpuModerationError::Client(GpuClientError::Request(e)) => {
            let code = if e.is_timeout() {
                "timeout".to_string()
            } else if let Some(status) = e.status() {
                format!("http_{}", status.as_u16())
            } else {
                "request".to_string()
            };
            ("client", code)
        }
        GpuModerationError::Client(GpuClientError::MissingContent) => {
            ("client", "missing_content".to_string())
        }
        GpuModerationError::ModelOutput(ModelOutputError::InvalidJson) => {
            ("model_output", "invalid_json".to_string())
        }
        GpuModerationError::ModelOutput(ModelOutputError::InvalidSchema) => {
            ("model_output", "invalid_schema".to_string())
        }
        GpuModerationError::PromptNotConfigured(_) => {
            ("config", "prompt_not_configured".to_string())
        }
    }
}
```

Add the import at the top: `use nsfw_clients::gpu::GpuClientError;` (already have `ImageInput`, extend the `use` line) and ensure `ModelOutputError` is imported from `nsfw_core` (extend the existing `use nsfw_core::{...}` line).

Then, in **both** `moderate_image_generation` and `moderate_text`, wrap the retry loop in a span and add attempt/give-up logging. Concretely, replace the `for attempt in 1..=max_attempts { ... }` loop's `Err(err)` arm and add a span. For `moderate_text` (apply the parallel change to `moderate_image_generation`, using `operation = "image_generation"`):

Wrap the loop body by creating a span before the loop:

```rust
        let span = tracing::info_span!("gpu.moderate", operation = "text", max_attempts);
        let _enter = span.enter();
        let start = std::time::Instant::now();
```

Change the `Ok(value) => return Ok(value),` arm to log success first:

```rust
                Ok(value) => {
                    tracing::info!(
                        attempts_used = attempt,
                        latency_ms = start.elapsed().as_millis() as u64,
                        "gpu moderation succeeded"
                    );
                    return Ok(value);
                }
```

Change the `Err(err)` arm to log per-attempt / give-up:

```rust
                Err(err) => {
                    let (error_kind, error_code) = classify_error(&err);
                    if attempt < max_attempts {
                        tracing::warn!(
                            operation = "text",
                            error_kind,
                            error_code = %error_code,
                            attempt,
                            retry_remaining = true,
                            "gpu moderation attempt failed"
                        );
                    } else {
                        tracing::error!(
                            operation = "text",
                            error_kind,
                            error_code = %error_code,
                            attempt,
                            max_attempts,
                            "gpu moderation failed after retries"
                        );
                    }
                    last_error = Some(err);
                    if attempt < max_attempts {
                        sleep_before_retry(attempt, max_attempts, self.config.retry_base_delay_seconds).await;
                    }
                }
```

> Content note: the prompt/text is NEVER a field on these `warn!`/`error!`/`info!` events. If richer local debugging is ever wanted, add it only at `tracing::debug!` (sentry ignores DEBUG). Not required by this plan.

- [ ] **Step 4: Run the capture test + the existing GPU tests**

Run: `cargo test -p nsfw-services`
Expected: PASS (new capture test + the 4 existing tests, which are behavior-only and unaffected).

- [ ] **Step 5: Commit**

```bash
git add crates/nsfw-services/src/gpu_moderation.rs
git commit -m "feat(services): instrument GPU moderation with spans, retry warns, give-up errors"
```

---

## Task 7: Image-download instrumentation with redacted URL (TDD)

**Files:**
- Modify: `crates/nsfw-api/src/image_detection.rs`

- [ ] **Step 1: Write the failing capture test**

Add a `#[cfg(test)] mod tests` to `image_detection.rs` (the file currently has none) with a `#[traced_test]` test: point the service at a mock server that returns 500 for every attempt, then assert a `warn!`/`error!` was emitted with redacted URL fields (`url_host`) and `error_kind`, and that the raw query string never appears in logs.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use nsfw_config::Settings;
    use tracing_test::traced_test;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn service(server_uri: &str) -> ImageDetectionService {
        // GPU not configured is fine — the download fails before GPU is reached.
        let settings = std::sync::Arc::new(
            Settings::from_map(&std::collections::HashMap::from([(
                "IMAGE_DOWNLOAD_MAX_ATTEMPTS".to_string(),
                "2".to_string(),
            )]))
            .unwrap(),
        );
        let _ = server_uri;
        ImageDetectionService::new(settings, None, reqwest::Client::new())
    }

    #[traced_test]
    #[tokio::test]
    async fn logs_redacted_url_on_download_failure() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let svc = service(&server.uri()).await;
        let url = format!("{}/img.jpg?sig=TOPSECRET", server.uri());

        let result = svc.download_image_with_retries(&url).await;
        assert!(result.is_err());

        assert!(logs_contain("error_kind"));
        assert!(logs_contain("url_host"));
        // The signed-query credential must never be logged.
        assert!(!logs_contain("TOPSECRET"));
    }
}
```

> `download_image_with_retries` is currently private (`async fn`). Make it `pub(crate)` so the test module (same crate, but a child module can already see it — actually a child `mod tests` can call private methods). Keep it private; the test is an inner module of the same file and has access.

- [ ] **Step 2: Run — fails (no url_host/error_kind in logs yet)**

Run: `cargo test -p nsfw-api --lib image_detection`
Expected: FAIL.

- [ ] **Step 3: Instrument `download_image_with_retries`**

At the top of `image_detection.rs`, add `use nsfw_observability::safe_url;`.

Inside `download_image_with_retries`, compute the redacted URL once and open a span:

```rust
        let redacted = safe_url(image_url);
        let span = tracing::info_span!(
            "image.download",
            url_scheme = %redacted.scheme,
            url_host = %redacted.host,
            url_path = %redacted.path,
            url_port = redacted.port,
            max_attempts,
        );
        let _enter = span.enter();
        tracing::debug!(image_url, "downloading image"); // raw URL only at debug
```

In the loop, when a non-final attempt fails set an `error_kind` and `warn!`; when the loop exhausts, `error!` before building the returned error. Add a small helper mapping the `DownloadFailure` to a kind string. Concretely, add after the `match request.send()...` block, inside `if attempt < max_attempts { ... }`, a warn:

```rust
            if attempt < max_attempts {
                if let Some(kind) = last_error.as_ref().map(download_kind) {
                    tracing::warn!(error_kind = kind, attempt, retry_remaining = true, "image download attempt failed");
                }
                sleep_before_retry(
                    attempt,
                    max_attempts,
                    self.settings.image_download_retry_base_delay_seconds,
                )
                .await;
            }
```

And immediately before the final `Err(match last_error { ... })`, add:

```rust
        if let Some(kind) = last_error.as_ref().map(download_kind) {
            tracing::error!(error_kind = kind, attempts = max_attempts, "image download failed after retries");
        }
```

Add the helper near `sleep_before_retry`:

```rust
fn download_kind(f: &DownloadFailure) -> &'static str {
    match f {
        DownloadFailure::Timeout => "timeout",
        DownloadFailure::HttpStatus => "http_status",
        DownloadFailure::Request => "request",
    }
}
```

> The non-retryable 4xx early-return path (line ~141) does not log an event — it's a client error, consistent with the 4xx→debug policy. Leave it as-is.

- [ ] **Step 4: Run the test + existing api tests**

Run: `cargo test -p nsfw-api`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/nsfw-api/src/image_detection.rs
git commit -m "feat(api): instrument image download with redacted URL spans and failure logs"
```

---

## Task 8: Error-response instrumentation (TDD)

**Files:**
- Modify: `crates/nsfw-api/src/error.rs`

- [ ] **Step 1: Write the failing capture test**

Add a `#[cfg(test)] mod tests` to `error.rs` asserting a 5xx `AppError` logs `error!` with `error_code`/`http_status`, and a 4xx logs at `debug!` (no error-level line). Use `nsfw_core::{AppError, ErrorCode}` — pick a known 5xx code (`ModelModerationFailed`) and a 4xx code (`ValidationError`). Verify the actual HTTP status each maps to via `nsfw-core` first; adjust the assertion to the real numbers.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;
    use nsfw_core::{AppError, ErrorCode};
    use tracing_test::traced_test;

    #[traced_test]
    #[test]
    fn logs_error_event_for_5xx() {
        let err = ApiError::from(AppError::new(ErrorCode::ModelModerationFailed, "boom"));
        let _ = err.into_response();
        assert!(logs_contain("http_status"));
        assert!(logs_contain("error_code"));
    }

    #[traced_test]
    #[test]
    fn does_not_error_log_for_4xx() {
        let err = ApiError::from(AppError::new(ErrorCode::ValidationError, "bad"));
        let _ = err.into_response();
        // 4xx must not produce an ERROR-level event (would page on-call).
        assert!(!logs_contain("ERROR"));
    }
}
```

- [ ] **Step 2: Run — fails (no logging in `into_response` yet)**

Run: `cargo test -p nsfw-api --lib error`
Expected: FAIL.

- [ ] **Step 3: Add the logging to `into_response`**

In `impl IntoResponse for ApiError`, after computing `let status = self.0.status;` and before building the body, branch on server vs client error. Note `code` is consumed into the body, so capture the string first:

```rust
    fn into_response(self) -> Response {
        let status = self.0.status;
        let code = self.0.code.as_str().to_string();
        if status.is_server_error() {
            // 5xx -> ERROR (Sentry event). method/path/request_id come from the entered
            // http.request span via sentry-tracing propagation (spec §3.D), not fields here.
            tracing::error!(error_code = %code, http_status = status.as_u16(), "request failed");
        } else {
            // 4xx -> client error, not an alert.
            tracing::debug!(error_code = %code, http_status = status.as_u16(), "request rejected");
        }
        let body = ErrorEnvelope {
            error: ErrorBody {
                code,
                message: self.0.message,
            },
        };
        (status, Json(body)).into_response()
    }
```

- [ ] **Step 4: Run the test + full api suite**

Run: `cargo test -p nsfw-api`
Expected: PASS. If `does_not_error_log_for_4xx` is brittle against the substring `"ERROR"`, tighten it to assert absence of the `"request failed"` message instead.

- [ ] **Step 5: Commit**

```bash
git add crates/nsfw-api/src/error.rs
git commit -m "feat(api): log 5xx as error events and 4xx at debug in ApiError"
```

---

## Task 9: OpenAPI completion (TDD)

**Files:**
- Modify: `crates/nsfw-core/Cargo.toml`, `crates/nsfw-core/src/model_output.rs`
- Modify: `crates/nsfw-api/src/moderation_routes.rs`, `crates/nsfw-api/src/health.rs`, `crates/nsfw-api/src/main.rs`
- Test: `crates/nsfw-api/tests/openapi_test.rs` (new)

- [ ] **Step 1: Write the failing completeness test**

`crates/nsfw-api/tests/openapi_test.rs`:

```rust
//! The generated OpenAPI document must describe every public route, not just /health.
use utoipa::OpenApi;

#[test]
fn openapi_documents_all_routes() {
    // Rebuild the same ApiDoc the binary serves. Keep this list in sync with main.rs.
    let json = nsfw_api::api_doc::ApiDoc::openapi()
        .to_json()
        .expect("serialize openapi");
    for path in [
        "/health",
        "/ready",
        "/v1/images/detect-url",
        "/v1/images/detect-base64",
        "/v1/text/detect",
    ] {
        assert!(json.contains(path), "openapi.json missing path {path}");
    }
}
```

> This requires `ApiDoc` to be reachable from the library (currently it lives in `main.rs`, not exported). Step 4 moves it into a `pub mod api_doc` in the library so both the binary and this test share one definition.

- [ ] **Step 2: Run — fails to compile (`nsfw_api::api_doc` missing)**

Run: `cargo test -p nsfw-api --test openapi_test`
Expected: FAIL (compile error).

- [ ] **Step 3: Add `ToSchema` derives**

`crates/nsfw-core/Cargo.toml` `[dependencies]` add:

```toml
utoipa = "5"
```

`crates/nsfw-core/src/model_output.rs` — change the `ModerationModelOutput` derive line:

```rust
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ModerationModelOutput {
```

`crates/nsfw-api/src/moderation_routes.rs` — add `utoipa::ToSchema` to each request DTO derive:

```rust
#[derive(Deserialize, utoipa::ToSchema)]
pub struct ImageUrlDetectRequest { /* unchanged fields */ }

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ImageBase64DetectRequest { /* unchanged fields */ }

#[derive(Deserialize, utoipa::ToSchema)]
pub struct TextDetectRequest { /* unchanged fields */ }
```

`crates/nsfw-api/src/health.rs` — add `utoipa::ToSchema` to `ReadinessDependency` and `ReadinessResponse` derives:

```rust
#[derive(Serialize, utoipa::ToSchema)]
pub struct ReadinessDependency { /* ... */ }

#[derive(Serialize, utoipa::ToSchema)]
pub struct ReadinessResponse { /* ... */ }
```

- [ ] **Step 4: Add `#[utoipa::path]` annotations to the routes**

`crates/nsfw-api/src/health.rs` — annotate `ready` (the `health` fn already has one):

```rust
#[utoipa::path(
    get, path = "/ready",
    responses(
        (status = 200, description = "All dependencies ready", body = ReadinessResponse),
        (status = 503, description = "One or more dependencies not ready", body = ReadinessResponse),
    )
)]
pub async fn ready(/* unchanged */) -> (StatusCode, Json<ReadinessResponse>) {
```

`crates/nsfw-api/src/moderation_routes.rs` — annotate the three handlers. Document the HMAC header requirement in the description (spec §5). Example for `detect_image_url` (mirror for the other two, swapping request body type + path):

```rust
#[utoipa::path(
    post, path = "/v1/images/detect-url",
    request_body = ImageUrlDetectRequest,
    responses(
        (status = 200, description = "Moderation verdict", body = ModerationModelOutput),
        (status = 401, description = "Missing/invalid internal HMAC signature"),
        (status = 422, description = "Validation error"),
        (status = 502, description = "Model moderation failed"),
        (status = 503, description = "GPU moderation not configured"),
    ),
    description = "Requires internal HMAC headers: X-Internal-Timestamp, X-Internal-Signature."
)]
pub async fn detect_image_url(/* unchanged */) -> Result<Json<ModerationModelOutput>, ApiError> {
```

For `detect_image_base64` use `path = "/v1/images/detect-base64"`, `request_body = ImageBase64DetectRequest`. For `detect_text` use `path = "/v1/text/detect"`, `request_body = TextDetectRequest`.

- [ ] **Step 5: Move `ApiDoc` into the library and register everything**

Create `crates/nsfw-api/src/api_doc.rs`:

```rust
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::health::health,
        crate::health::ready,
        crate::moderation_routes::detect_image_url,
        crate::moderation_routes::detect_image_base64,
        crate::moderation_routes::detect_text,
    ),
    components(schemas(
        crate::moderation_routes::ImageUrlDetectRequest,
        crate::moderation_routes::ImageBase64DetectRequest,
        crate::moderation_routes::TextDetectRequest,
        crate::health::ReadinessResponse,
        crate::health::ReadinessDependency,
        nsfw_core::ModerationModelOutput,
    ))
)]
pub struct ApiDoc;
```

Add `pub mod api_doc;` to `crates/nsfw-api/src/lib.rs`.

In `crates/nsfw-api/src/main.rs`: delete the local `#[derive(OpenApi)] struct ApiDoc;` block (lines 17–19) and the now-unused `use utoipa::OpenApi;`; change the SwaggerUi line to use the shared type:

```rust
        .merge(SwaggerUi::new("/docs").url("/openapi.json", nsfw_api::api_doc::ApiDoc::openapi()))
```

(Keep `use utoipa_swagger_ui::SwaggerUi;`. `nsfw_api::api_doc::ApiDoc::openapi()` needs `OpenApi` in scope where called — reference it via the trait: add `use utoipa::OpenApi;` back only if the method isn't resolved; the derive implements the inherent-looking `::openapi()` through the trait, so keep the `use utoipa::OpenApi;` import in `main.rs`.)

- [ ] **Step 6: Run the completeness test + full api suite**

Run: `cargo test -p nsfw-api`
Expected: PASS (openapi_test + all existing tests).

- [ ] **Step 7: Commit**

```bash
git add crates/nsfw-core/Cargo.toml crates/nsfw-core/src/model_output.rs \
  crates/nsfw-api/src/moderation_routes.rs crates/nsfw-api/src/health.rs \
  crates/nsfw-api/src/api_doc.rs crates/nsfw-api/src/lib.rs crates/nsfw-api/src/main.rs \
  crates/nsfw-api/tests/openapi_test.rs
git commit -m "feat(api): complete OpenAPI docs for all routes and schemas"
```

---

## Task 10: Full workspace verification

**Files:** none (verification + final commit if fmt changes anything)

- [ ] **Step 1: Format**

Run: `cargo fmt --all`

- [ ] **Step 2: Lint (warnings as errors)**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: no warnings. Fix any that appear (common: unused imports left from the `main.rs` restructure).

- [ ] **Step 3: Test the whole workspace**

Run: `cargo test --workspace`
Expected: all green.

- [ ] **Step 4: Confirm the binary builds in release (mirrors the Docker build)**

Run: `cargo build --release -p nsfw-api`
Expected: compiles.

- [ ] **Step 5: Commit any formatting/lint fixups**

```bash
git add -A
git commit -m "chore: fmt + clippy fixups for observability and openapi work" || echo "nothing to commit"
```

- [ ] **Step 6: Manual Sentry verification (once, out-of-band — not a blocker for merge)**

With `SENTRY_DSN` set to the real DSN and the service running locally, trigger a panic (or a forced 5xx) and confirm the event lands in `sentry.prakash.yral.com`. Documented in spec §6; do this before relying on production alerting.

---

## Notes on parity & policy (carried from the spec)

- **Content→Sentry is structurally impossible:** content (prompts, text, raw URLs, image bytes) is only ever emitted at `tracing::debug!`, and the sentry-tracing layer's default EventFilter ignores DEBUG. `RUST_LOG=debug` puts content in journald only.
- **No secrets in tracing fields:** the API key stays inside `GpuOpenAiClient`; `Settings` is never logged whole (only `environment`/`gpu_configured`/`port` at startup).
- **Improvement over Python:** per-attempt failures are `warn!` (breadcrumb), only the final give-up is `error!` (event) — one Sentry issue per genuine failure instead of one per transient retry.
