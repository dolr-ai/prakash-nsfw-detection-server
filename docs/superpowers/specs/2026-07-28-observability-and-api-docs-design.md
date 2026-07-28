# Observability (tracing + Sentry) & OpenAPI Completion — Design Spec

Status: draft, pending spec-review + user sign-off
Target repo: `prakash-nsfw-detection-server` (this repo)
Date: 2026-07-28

## 1. Overview & Goals

The live moderation service (stateless image/text/image+prompt detection) currently has **no observability**: no structured logging, no error reporting, no request correlation. If moderation misbehaves in production, we are blind — no logs, no Sentry. Separately, the OpenAPI docs are incomplete (only `/health` is registered).

This spec closes both gaps as one effort:

1. **Structured logging** via `tracing` (JSON to stdout → journald).
2. **Sentry** error reporting, at parity with the Python service's capture sites, plus panic capture.
3. **Request correlation** — a real request id threaded through every log/event.
4. **Spans + timing** — per-request and per-call spans with latency, beyond what Python had.
5. **OpenAPI completion** — annotate + register the 3 moderation routes and `/ready`.

Non-goals: metrics/Prometheus endpoint (deferred — not in Python, nothing to scrape yet); observability for the video pipeline / data layer (not built, out of scope).

### Parity reference — Python's observability

Python (`app/core/sentry.py`, `gpu_moderation_service.py`, `image_detection_service.py`, `errors/http.py`):
- `init_sentry`: `dsn`, `send_default_pii` (default false), `environment`, FastAPI/Starlette integrations.
- `capture_exception(exc, tags, context)`: sets tags + a `nsfw_detector` context, captures.
- **6 capture sites** in the stateless paths:
  1. GPU `_capture_model_attempt_failure` — **every failed attempt**; tags `component=gpu_moderation`, `operation` (`visual_batch`/`image_generation`/`text`), `error_code`, `retry_remaining`; context `attempt`, `max_attempts`, `error_type`, `error_message`.
  2. Image `_capture_image_download_failure` — **every failed attempt**; tags `component=image_detection`, `operation=download_image_url`, `error_kind` (`timeout`/`http_status`/`request_error`), `retry_remaining`; context `attempt`, `max_attempts`, `status_code`, + `_safe_url_context`.
  3. `_safe_url_context(url)` → `{url_scheme, url_host, url_path[:160], url_port}` — **redacted** (no query/creds).
  4. `app_error_handler` — on `status_code >= 500`; tags `component=api`, `error_code`, `http_status`; context `method`, `path`, `error_code`, `error_message`.
  5. `unhandled_error_handler` — tags `component=api`, `operation=unhandled_exception`; context `method`, `path`.

The Rust port replicates all of these, with two deliberate improvements (below).

## 2. Architecture

**Principle:** `tracing` is the single instrumentation interface. Service code emits structured events + spans; only the binary knows about Sentry. This keeps the observability policy (levels, routing, redaction) in one place and keeps the library crates decoupled from Sentry.

### New crate: `nsfw-observability`

Owns the wiring so `main` stays thin and setup is unit-testable.

```
crates/nsfw-observability/
  Cargo.toml           # sentry, sentry-tracing, tracing, tracing-subscriber, nsfw-config
  src/
    lib.rs             # pub use init, Guard, redact
    init.rs            # init(&Settings) -> ObservabilityGuard  (sentry + subscriber stack)
    redact.rs          # safe_url(&str) -> SafeUrl  (_safe_url_context parity)
```

- `init(&Settings) -> ObservabilityGuard` — installs the Sentry client + the tracing subscriber stack; returns a guard (wrapping `sentry::ClientInitGuard`) held for process lifetime so events flush on exit.
- `redact::safe_url(url) -> SafeUrl` — `{ scheme, host, path (≤160 chars), port }`; no query, userinfo, or fragment. `SafeUrl` implements `tracing::Value`-friendly field access (its parts are logged as individual fields).

### Dependency direction

```
nsfw-core                       (stays pure — NO tracing; it has no emit sites)
nsfw-clients   ──→ tracing      (gpu client: emits nothing itself today; tracing dep only if a site is added)
nsfw-services  ──→ tracing      (GPU retry loop: warn per attempt, error on give-up)
nsfw-api       ──→ tracing      (image download, error responses, request span)
nsfw-api (bin) ──→ nsfw-observability ──→ sentry, sentry-tracing, tracing-subscriber
```

`nsfw-core` deliberately gains **no** `tracing` dependency — it is pure domain logic with no I/O and no failure-reporting sites. Only `nsfw-services` and `nsfw-api` emit events. `nsfw-clients` gets `tracing` only if we add a site there (none planned in this spec).

### Subscriber stack (installed once in `init`)

Built with `tracing_subscriber::registry()` + layers:

1. **`EnvFilter`** — level from `RUST_LOG` (default `info`). `RUST_LOG=debug` enables content logging locally.
2. **`fmt` layer** — JSON to stdout (`.json()`), so journald captures structured lines (matches how the other prakash services log).
3. **`sentry_tracing::layer()`** — maps tracing events to Sentry. Its **default `EventFilter` already does what we want**: `ERROR → Event`, `WARN/INFO → Breadcrumb`, `DEBUG/TRACE → Ignore`. So:
   - `error!` → Sentry event (alert)
   - `warn!`/`info!` → breadcrumb (context, no alert)
   - `debug!` → not sent to Sentry at all
   This is the linchpin of the content policy (§4): content only ever logs at `debug!`, so it can never reach Sentry, even with `RUST_LOG=debug`. No custom event filter is required; if the default ever changes, pin it explicitly.
4. **Sentry panic hook** — installed by `sentry::init` (panic integration is a default feature). Captures panics (the `panic!("…")` verify test delivers an event).

### `main` restructure (required)

Sentry must be initialized **before** the tokio runtime starts (its guard + background transport thread), and settings (DSN, environment) must load first. Current `main` is `#[tokio::main]` with settings loaded inside async main — that ordering won't work.

New shape:

```rust
fn main() {
    let _ = dotenvy::dotenv();
    let settings = Arc::new(Settings::from_env().expect("failed to load settings"));
    // Sentry + tracing subscriber installed here, before any async runtime.
    let _guard = nsfw_observability::init(&settings);

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
        .block_on(async move { serve(settings).await });
}
```

`#[tokio::main]` is dropped; the async body moves into `serve(settings)`. `_guard` is held to the end of `main`.

### Sentry config

Reads the three already-present-but-unused `Settings` fields: `sentry_dsn` (`Option<SecretString>`), `sentry_send_default_pii` (default false), `environment`. `init` no-ops if `sentry_dsn` is `None` (like Python) — the tracing subscriber still installs, so logging works without Sentry. `release` is set from `sentry::release_name!()`.

Crate versions: `sentry = "0.32"` (as provided), `sentry-tracing` pinned to the **matching minor** (`0.32`). Default features include `panic`, `backtrace`, `contexts`, and a TLS-capable transport (reaches `https://sentry.prakash.yral.com`).

## 3. Instrumentation points

Every emit site, its level, and its fields.

### A. Per-request span (`nsfw-api`, tower-http `TraceLayer`)

- span `http.request`, fields: `method`, `path`, `request_id`, `status`, `latency_ms`.
- one `info!` on completion → breadcrumb. All deeper events inherit `request_id` from the span.
- **Requires the request-id rework (§3.F).**

### B. GPU moderation (`nsfw-services`, all 3 methods)

Replaces Python's `_capture_model_attempt_failure`.
- span `gpu.moderate` per call: fields `operation` (`visual_batch`/`image_generation`/`text`), `max_attempts`, and on completion `latency_ms`, `attempts_used`.
- **failed non-final attempt** → `warn!` with `error_code`, `error_kind`, `retry_remaining=true`, `attempt` → breadcrumb.
- **retries exhausted (give-up)** → `error!` with `error_code`, `error_kind`, `attempt`, `max_attempts` → Sentry event.
- success → `info!` with `latency_ms`, `attempts_used`.
- content (prompt/text) → `debug!` only, never in warn/error fields.

**Improvement over Python (flagged, decided):** Python fires a Sentry *event* on every failed attempt including transient ones that recover. Here, per-attempt = breadcrumb, only the final give-up = event → one Sentry issue per genuine failure, full retry history preserved in the event's breadcrumb trail.

### C. Image download (`nsfw-api`)

Replaces `_capture_image_download_failure` + `_safe_url_context`.
- span `image.download`: fields `attempt`, `max_attempts`, `latency_ms`, plus the **redacted** URL parts (`url_scheme`, `url_host`, `url_path`, `url_port`) via `redact::safe_url`. Never the raw URL.
- failed non-final attempt → `warn!` with `error_kind` (`timeout`/`http_status`/`request_error`), `status_code` (if any), `retry_remaining=true`, redacted url → breadcrumb.
- give-up → `error!` with `error_kind`, redacted url → Sentry event.
- raw URL → `debug!` only.

### D. Error responses (`nsfw-api`, `ApiError::into_response`)

Replaces `app_error_handler` / `unhandled_error_handler`.
- `status >= 500` → `error!` with `error_code`, `http_status`, `method`, `path` → Sentry event.
- `4xx` → `debug!` (client errors, not alerts; matches Python not capturing sub-500).
- panics → Sentry panic hook (§2).

To get `method`/`path` into the error event, `ApiError::into_response` reads them from the current tracing span (set by the request span, §A) rather than taking them as parameters — keeps the `IntoResponse` signature unchanged.

### E. Startup

One `info!` at boot: `environment`, `release`, `gpu_configured` (bool), bound `port`. No secrets, no DSN value.

### F. Request-id rework (required) (`nsfw-api/src/request_id.rs`)

Current middleware generates the id at **response** time — too late for correlation. Rework to:
1. On request **entry**: read `x-request-id` header, else generate `Uuid::new_v4()`.
2. Store it in a request extension (so handlers/`ApiError` can read it) and open the request span with it (§A).
3. Run the inner service.
4. Echo the id on the response header (unchanged behavior).

The existing three request-id tests are updated; a new test asserts the id is present *during* handling (e.g. a handler that reads the extension and echoes it in the body), not just on the response header.

### Field/level parity table

| Python site | Rust target / level | Fields | Sentry |
|---|---|---|---|
| `_capture_model_attempt_failure` (non-final) | `gpu.moderate` / `warn!` | operation, error_code, error_kind, retry_remaining, attempt | breadcrumb |
| GPU give-up | `gpu.moderate` / `error!` | operation, error_code, error_kind, attempt, max_attempts | event |
| `_capture_image_download_failure` (non-final) | `image.download` / `warn!` | error_kind, status_code, retry_remaining, url_* (redacted) | breadcrumb |
| image give-up | `image.download` / `error!` | error_kind, url_* (redacted) | event |
| `app_error_handler` (5xx) | `nsfw_api` / `error!` | error_code, http_status, method, path | event |
| `unhandled_error_handler` | panic hook + catch-all | — | event |

## 4. Redaction, content policy & secret safety

Three-tier data classification, enforced structurally:

| Tier | Examples | Logs INFO+ | Logs DEBUG | Sentry |
|---|---|---|---|---|
| Secret | API key, HMAC secret, DSN | ❌ never | ❌ never | ❌ never |
| Content | text prompt, generation prompt, image bytes, raw image URL | ❌ | ✅ | ❌ never |
| Metadata | category, severity, is_nsfw, error_code, latency, status, redacted URL parts, byte sizes | ✅ | ✅ | ✅ |

Structural guarantees (why this cannot leak by accident):
- **Content → Sentry is impossible:** content only appears in `debug!` events; the sentry-tracing layer ignores DEBUG. `RUST_LOG=debug` in prod puts content in journald only, never Sentry.
- **Secrets never in a tracing field:** the API key lives inside `GpuOpenAiClient`, only passed to `reqwest.bearer_auth()`; never a span/event field. `SecretString`'s `Debug` redacts if one is `?`-formatted by accident. `Settings` is never logged whole — only the explicit safe subset at startup (§3.E).
- **URL redaction at the boundary:** `safe_url` returns only `{scheme, host, path[:160], port}` — query (signed-URL creds), userinfo, and fragment dropped. Only this form reaches INFO/Sentry; the raw URL is DEBUG-only.
- **`send_default_pii=false`** (Settings default) — Sentry SDK won't attach request bodies/headers/IPs.

**Deliberate deviation from Python (flagged):** Python logs content nowhere; this spec allows content at `debug!` for local debuggability. Risk (someone sets `RUST_LOG=debug` in prod → explicit content in journald) is contained to journald only, never Sentry, and journald on these boxes is access-controlled.

## 5. OpenAPI completion (bundled)

Register the currently-undocumented routes so `/docs` + `/openapi.json` are complete.

- Add `#[utoipa::path(...)]` to `moderation_routes::{detect_image_url, detect_image_base64, detect_text}` and `health::ready`, with request/response schema references and the documented error responses (401 auth, 422 validation, 503 gpu-not-configured, 502 model errors).
- Derive `utoipa::ToSchema` on the request DTOs (`ImageUrlDetectRequest`, `ImageBase64DetectRequest`, `TextDetectRequest`), the response (`ModerationModelOutput` — add the derive in `nsfw-core` behind the existing `serde` usage), and the error envelope + readiness response.
- Extend `#[openapi(paths(...), components(schemas(...)))]` in `main.rs` to list all routes + schemas.
- Document the HMAC header requirement (`X-Internal-Timestamp`, `X-Internal-Signature`) via a security scheme note in each `/v1` path's description (mirrors the Python docstrings).

This is annotation-only — no behavior change, no new endpoints. Verified by a test asserting `/openapi.json` contains all four paths.

## 6. Testing

- **Redaction unit tests** (`nsfw-observability`, pure): `safe_url` drops query/userinfo/fragment, truncates path to 160, handles missing port/path. Table-driven (`rstest`).
- **Level-barrier test:** using `tracing-test` (or a capturing layer), assert a `debug!` content field does **not** appear in an INFO-filtered layer's output — proves the content→Sentry barrier at the level the sentry layer sees.
- **Instrumentation integration** (extend existing route tests, `nsfw-api`): with a capturing subscriber (no real Sentry), assert a mocked GPU 500 emits a `warn!` per retry with `error_code`/`retry_remaining`/`attempt` and an `error!` on give-up; a forced 5xx emits an `error!` with `error_code`/`http_status`.
- **Request-id-during-handling test** (§3.F): a handler reads the id from the request extension; assert it's present and equals the response header.
- **OpenAPI completeness test:** `/openapi.json` includes `/v1/images/detect-url`, `/v1/images/detect-base64`, `/v1/text/detect`, `/ready`.
- **No live Sentry in tests:** `init` is DSN-gated; tests set no DSN, so the SDK is inert. The tracing→Sentry mapping is validated by asserting tracing events (the layer's input), not Sentry output.
- **Manual verify (once):** run the service against the real DSN and trigger `panic!` — confirm the event lands in `sentry.prakash.yral.com`.

## 7. File-level change summary

- **New:** `crates/nsfw-observability/` (`init.rs`, `redact.rs`, `lib.rs`, tests).
- **`nsfw-api/src/main.rs`:** drop `#[tokio::main]`; init observability before runtime; `info!` startup line; extend `#[openapi(...)]`.
- **`nsfw-api/src/request_id.rs`:** generate id at entry, store in extension + span, echo on response.
- **`nsfw-api/src/moderation_routes.rs`:** `#[utoipa::path]` + `ToSchema` derives + request-id span usage.
- **`nsfw-api/src/image_detection.rs`:** `warn!`/`error!` on download failures with redacted URL; `debug!` raw URL; span with timing.
- **`nsfw-api/src/error.rs`:** `error!` on 5xx (fields from span), `debug!` on 4xx.
- **`nsfw-api/src/health.rs`:** `#[utoipa::path]` on `ready`; `ToSchema` on readiness types.
- **`nsfw-services/src/gpu_moderation.rs`:** `warn!` per failed attempt, `error!` on give-up, `info!` + `latency_ms` on success; span per call; content at `debug!`.
- **`nsfw-core/src/model_output.rs`:** add `utoipa::ToSchema` derive on `ModerationModelOutput`.
- **`Cargo.toml`s:** `tracing` for nsfw-services/nsfw-api; `nsfw-observability` deps; `tower-http` (trace feature) for nsfw-api; `utoipa` schema features.

## 8. Open items

1. **Sentry crate version** — spec pins `sentry`/`sentry-tracing` to `0.32`. If the repo later standardizes on a newer Sentry across services, bump both together (they must share a minor).
2. **`/metrics`** — explicitly deferred; revisit if the team wants Prometheus scraping.
3. **DSN provisioning** — the DSN (`…@sentry.prakash.yral.com/10`) is supplied via `SENTRY_DSN` env/Vault, same pattern as the other secrets; not hardcoded.
