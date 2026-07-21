# Rust Port of the NSFW Detection Service — Design Spec

Status: draft, pending spec-review + user sign-off
Source service: `/Users/prk-jr/Desktop/work/dolr/ansuman-nsfw-detection-server` (Python/FastAPI, `yral-nsfw-detector`)
Target repo: `/Users/prk-jr/Desktop/work/dolr/prakash-nsfw-detection-server` (this repo)

## 1. Overview & Goals

Port the existing Python NSFW detection service to Rust. The Python service is itself a from-scratch rewrite (see its own `plan.md`) of an older gRPC pipeline into a FastAPI REST API plus two background worker processes. This spec targets a full port of that current (REST + workers) system — not the legacy gRPC code, which stays retired.

Goals:
- Byte-for-byte wire and **business-rule** parity with the current Python service — every status code, error code, threshold, mapping table, and quirk in §5 — **except** for three categories of deliberate change: (a) a real fix for the one operational gap this spec fully resolves — the ClickHouse flush worker's missing scheduling loop (§11); (b) explicit, called-out performance improvements that Rust's concurrency model makes cheap (§6); and (c) closing the `excluded_videos` ClickHouse DDL gap (§13.2), a genuinely missing table rather than a behavior change. This list is exhaustive **for business-rule and wire-format behavior specifically**. It does not cover operational/robustness hardening that has no Python behavior to diverge from in the first place — graceful shutdown handling for `video-worker` (§10 step 4) and `flush-worker` (§11), since Python has none at all today, not a different implementation of shutdown; and a bounded-redirect policy on the shared HTTP client (§12) replacing Python's unbounded `follow_redirects=True`, which only changes behavior on a pathological redirect chain. Anything not covered by (a)/(b)/(c) or this hardening carve-out should match Python exactly, including its quirks (§5).
- Preserve every legacy-compatibility contract: `yral.video_nsfw_agg` old-schema table, `offchain:video_nsfw:{video_id}` KVRocks runtime key, the `/v1/videos/{video_id}/status` read path — downstream readers of these must not need to change.
- A second gap found in the source — neither background worker has a deployment definition anywhere in the source repo — is surfaced and flagged (§15, §17 item 1) but its resolution (which supervision mechanism to use) is explicitly deferred, not decided, by this spec.

Non-goals:
- Porting `app/legacy/` (old gRPC server, GCS/BigQuery pipeline, local ML classifiers). Explicitly out of scope, same as it was for the Python rewrite.
- Changing the off-chain caller (separate repo) — this spec only covers the service side of the HMAC-signed contract.
- Fly.io deployment — `fly.toml` in the source repo is confirmed stale (internal port mismatch vs. the actual FastAPI app; it's a leftover from the pre-rewrite gRPC service). Bare-metal + HAProxy + Docker Compose is the real, actively deployed production path and is the only target this spec covers.

## 2. How This Spec Was Produced

The source repo has its own `plan.md`, a genuinely detailed design doc, written before the Python implementation existed. Reading the *actual current code* against that plan turned up real drift — most importantly, the NSFW classification threshold logic in production does **not** match what `plan.md` describes. This spec is built from the **actual code**, not from `plan.md`'s prose, wherever the two disagree. Every place they disagree is called out explicitly below so the disagreement is a decision, not an accident.

That code-reading pass is written up in full in a companion reference document: [2026-07-21-python-service-source-audit.md](2026-07-21-python-service-source-audit.md), in the same directory as this spec. Every "per the audit §N" reference below points at that file's numbered sections. Implementers should treat that file, not `plan.md`, as the source of truth for exact field lists, env var names, and method signatures wherever this spec says to "transcribe" or "port verbatim" rather than reproducing the full detail inline.

## 3. Workspace & Crate Architecture

Cargo workspace (per decision: separate crates over a single multi-binary package, for build isolation as the codebase grows):

```
prakash-nsfw-detection-server/
  Cargo.toml                    # [workspace] members = ["crates/*"]
  crates/
    core/                       # domain models, error types, moderation policy, legacy mapping,
                                 # model-response parsing/validation — zero I/O, zero async deps
    config/                     # static Settings (env-loaded) + RuntimeConfig (KVRocks-backed, polled)
    repositories/                # trait defs (async-trait) + postgres/clickhouse/kvrocks impls
                                 #   + in-memory fakes behind a `test-fakes` feature
    clients/                     # gpu (openai-compatible chat completions), storj-interface, video download
    services/                    # orchestration layer, shared by both binaries below: GpuModerationService
                                 #   (retry loop + the single shared Arc<Semaphore> from §6.2), QueueService,
                                 #   AggregationService (wraps core::aggregate), VideoDetectionService,
                                 #   ImageDetectionService, TextDetectionService, ManualBanService,
                                 #   ReadinessService, ClickHouseFlushService — depends on `core`, `repositories`
                                 #   (via trait objects), and `clients`; has no HTTP-framework or CLI-entrypoint
                                 #   concerns of its own
    api/                         # axum binary: routes, HMAC middleware, health/ready, admin config API, OpenAPI
    video-worker/                 # binary: queue consumer, ffmpeg pipeline, GPU batching, finalize+commit
    flush-worker/                  # binary: continuous-loop KVRocks -> ClickHouse buffer flush
```

Dependency direction, same discipline as the Python service: `routes -> services -> repositories -> clients/database`. `core` has no dependency on `repositories`/`clients`/`api` — it's pure domain logic and is where most of the high-value unit tests live (moderation policy, legacy mapping, model-response parsing, aggregation), runnable with no tokio runtime.

The `services` crate exists specifically so logic needed by more than one binary has exactly one implementation. Concretely: `GpuModerationService` is used by `api` (stateless `/v1/images/*` and `/v1/text/detect` routes, §18 Phase 4) **and** by `video-worker` (frame-batch dispatch, §6.2/§10) — without a shared crate, these would end up as two hand-written copies of the same retry loop and semaphore, silently breaking §6.3's "applied identically at all three sites" guarantee for the jittered backoff formula. `video-worker` and `flush-worker` binaries are thin: they own the consumer/scheduling loop and call into `services` for everything else, same as `api` does for its routes.

## 4. Tech Stack

| Concern | Choice | Rationale |
|---|---|---|
| Async runtime | `tokio` | universal default, required by every other choice below |
| Web framework | `axum` | tower-native middleware fits the HMAC-auth-as-layer and request-id patterns; strong ecosystem overlap with sqlx/tracing |
| Postgres | `sqlx` (`postgres`, `runtime-tokio`, `macros` features) | async-native; built-in migration runner replaces Alembic; compile-time query checking available |
| ClickHouse | `clickhouse` crate (clickhouse.rs) | async-native HTTP client. Python's sync `clickhouse-connect` client blocks its event loop in the flush path too, but this is a smaller concern there than it sounds — the audit notes it's "acceptable since the flush worker is a separate single-purpose process," not shared with request-serving code. Using an async client in Rust isn't fixing an urgent bug so much as avoiding introducing one, since §11's redesign turns the flush worker into a long-running continuous-loop process where a blocking call has more opportunity to matter |
| Redis/KVRocks | `redis` crate + `deadpool-redis` pooling | needs cluster-mode + Streams support; see §13.3 for the concrete open decision on cluster-mode read path |
| HTTP client | `reqwest` | video download, Storj client, image download |
| GPU client | hand-rolled `reqwest` request/response structs, not an OpenAI SDK crate | the actual call surface is one shape (chat completions, image_url content blocks) — a full SDK crate is unneeded surface area |
| Retry | hand-rolled, exact backoff formula ported (see §6.3) | the exact constants are load-bearing business behavior, not a place for a generic retry crate's defaults |
| Error handling | `thiserror` domain error enum in `core`; `axum::response::IntoResponse` impl in `api` | mirrors Python's `AppError(code, message, status_code)` triple exactly |
| Config | hand-rolled `serde`/`std::env` struct for static `Settings`; separate `RuntimeConfig` (§8.2) | needs to replicate pydantic-settings' trailing-space-alias quirk (§8.1) and fail-fast-on-missing-required behavior |
| Sentry | `sentry` crate + tower layer | parity with Python's breadcrumbs/tags on GPU retry attempts and 5xx responses |
| Logging/tracing | `tracing` + `tracing-subscriber` (structured JSON output) | needed for the structured-log requirement on every admin-config change (§8.2) and general parity with Python's existing structured logging; not present in Python's own dependency list but required to meet this spec's own logging requirements |
| Video probing/extraction | `tokio::process::Command` shelling to system `ffmpeg`/`ffprobe` | same approach as Python — no native Rust video decode needed |
| OpenAPI | `utoipa` + `utoipa-swagger-ui` | axum has no FastAPI-style auto docs; this replicates `/openapi.json` + a Swagger UI, mounted only in the `api` crate |
| Testing | `#[tokio::test]`, `mockall` (repo/client trait mocks), `rstest` (parameterized threshold/mapping tests), `testcontainers-rs` (real ephemeral Postgres/ClickHouse/Redis for repository integration tests) | see §14 |
| Coverage | `cargo-llvm-cov`, 80%+ target | existing project convention |

## 5. Behavioral Parity Policy

Per explicit decision: **exact parity on business behavior, real fixes on operational gaps that have no working implementation today.**

Concretely:
- **Preserved exactly** (even where it looks like a bug): manual-ban endpoint accepting-but-discarding most request fields (§9.3); the hardcoded `"explicit"`/`"VERY_UNLIKELY"`/`"banned"` values on manual ban; the `service_unavailable` error code being reused for genuinely unhandled exceptions; the KVRocks video-id lookup key's stale-pointer behavior on resubmission (§13.3); the dual independent `attempts` counters between KVRocks and Postgres (§10, step 2); the `FAILED_RETRYABLE` re-enqueue gap (below); Python's `CATEGORY_BLOCK_THRESHOLDS` table taking precedence over `plan.md`'s prose rule (§7.1) — **this is the single most consequential parity requirement in this spec.**

  **`FAILED_RETRYABLE` re-enqueue quirk**: `enqueue_video_job`'s idempotency check looks up an existing job by the unique key `(video_id, source_object_version, policy_version)`. If a job is found and its status is `Queued`, `Processing`, or any of `TERMINAL_VIDEO_STATUSES` (`Classified`, `FailedTerminal`, `Superseded`), enqueue short-circuits and returns the existing job unchanged. `FailedRetryable` is the one status **not** in that checked set — a job stuck in `FailedRetryable` under the same unique key falls through and gets a **new** job/job_id on resubmission, rather than being returned as-is or having its retry accelerated. This looks like an oversight in the original status-set construction, not an intentional design, but per the parity decision above it is preserved as-is: the Rust `enqueue_video_job` implementation must replicate this exact "all statuses except `FailedRetryable`" short-circuit set, not a more sensible "any existing job at this key blocks re-enqueue" rule.
- **Fixed, because there's nothing working to be compatible with**: ClickHouse flush worker becomes a real continuous loop instead of a one-shot process nothing ever re-invokes (§11); `excluded_videos` gets an actual `CREATE TABLE` (§13.2) since none exists anywhere in the source repo.
- **Deliberate performance improvements, called out as behavior changes**: §6.

## 6. Performance Improvements Over Python Baseline

These are intentional deltas, confirmed with the user, not silent changes:

### 6.1 Worker concurrency
Python's video worker reads exactly one queue message at a time (`queue_read_count=1`) and awaits it fully before reading the next — one job in flight per OS process, full stop. Scaling is horizontal-process-only.

Rust `video-worker`: a bounded `tokio::sync::Semaphore` (permits = `RuntimeConfig.worker_max_concurrent_jobs`, tunable) gates how many job tasks run concurrently within one process. The consumer loop claims a message, spawns a task (`tokio::spawn`) if a permit is available, and continues claiming — it does not await job completion inline.

### 6.2 Concurrent frame-batch dispatch within a job
Within one video's processing, all frame batches are submitted to the GPU via `futures::stream::iter(batches).map(...).buffer_unordered(gpu_max_concurrency)` instead of Python's sequential per-batch await. Bounded by the same concurrency-5 (default) semaphore that already exists for GPU calls — this does not increase total GPU load, it just lets one video's batches interleave instead of serializing. Results are tagged with batch index and reassembled into frame order before aggregation (order matters for `frame_index` continuity checks).

**Important construction detail, given §6.1's worker-concurrency change**: the GPU semaphore must be a single `Arc<Semaphore>` constructed once per `video-worker` process and shared across every concurrently-running job task — not a fresh semaphore per job. A per-job semaphore would let effective GPU concurrency scale as `worker_max_concurrent_jobs × gpu_max_concurrency` instead of staying capped at `gpu_max_concurrency` (5) process-wide, silently multiplying load on the GPU endpoint as §6.1's job concurrency increases.

### 6.3 GPU retry jitter
Python's backoff: `min(base_delay * 2**(attempt-1), cap)`, deterministic, no jitter — despite `plan.md` claiming jitter is used. Confirmed by reading the actual retry helper. This is a real risk: correlated GPU 5xx blips cause every worker to retry in lockstep.

Rust: `min(base_delay * 2^(attempt-1), cap) * rand(0.5..1.5)` (full-jitter style), applied identically at all three sites that currently share this formula in Python: GPU moderation retries, image-download retries, and KVRocks pool-exhaustion read retries.

### 6.4 Explicit Postgres pool sizing
Python's async engine is created with `pool_pre_ping=True` and nothing else — pool size and overflow are whatever SQLAlchemy's dialect defaults happen to be (5 / 10), never deliberately chosen.

Rust: `sqlx::postgres::PgPoolOptions` configured explicitly per binary (new settings `POSTGRES_POOL_MAX_CONNECTIONS`, `POSTGRES_POOL_MIN_CONNECTIONS`, `POSTGRES_POOL_ACQUIRE_TIMEOUT_SECONDS`), sized independently for `api` (read-mostly status lookups) vs `video-worker` (transactional writes) since — unlike Python's one shared process — they are now separate binaries with independent pools by construction.

## 7. Domain Model & Error Handling (`core` crate)

### 7.1 Enums / constants — ported from actual code, not `plan.md`

```rust
pub const MODERATION_CATEGORIES: [&str; 11] = [
    "safe", "suggestive", "nudity", "porn", "gore", "violence",
    "self_harm", "hate_or_extremism", "drugs", "unknown", "sexual_minor_content",
];

// CATEGORY_BLOCK_THRESHOLDS — this table, not plan.md's
// "top_category in unsafe_categories OR overall_severity >= 3" rule, is ground truth.
// Confirmed against tests/unit/services/test_moderation_policy.py in the source repo.
pub const CATEGORY_BLOCK_THRESHOLDS: &[(&str, u8)] = &[
    ("sexual_minor_content", 3),
    ("porn", 4), ("gore", 4), ("violence", 4), ("self_harm", 4),
    ("hate_or_extremism", 4), ("drugs", 4), ("unknown", 4),
    ("suggestive", 5), ("nudity", 5),
    // "safe" is absent — never triggers is_nsfw
];

pub const RISK_ORDER: [&str; 11] = [
    "sexual_minor_content", "porn", "nudity", "gore", "violence",
    "self_harm", "hate_or_extremism", "drugs", "suggestive", "unknown", "safe",
];

pub enum VideoJobStatus {
    Queued, Processing, Classified, FailedRetryable, FailedTerminal,
    Superseded, // unused by any code path today; kept for wire/API compatibility
}
pub const TERMINAL_VIDEO_STATUSES: [VideoJobStatus; 3] =
    [Classified, FailedTerminal, Superseded];
```

`compute_is_nsfw(categories) -> bool`: `true` iff any category's score meets or exceeds its threshold in `CATEGORY_BLOCK_THRESHOLDS`. Note the asymmetry that must be preserved exactly: `nudity=4` ("strong") does **not** flag NSFW; only `nudity=5` ("explicit/severe") does, while `porn=4` already does. This is confirmed, tested, current production behavior.

`compute_overall_severity(top_category, categories) -> u8`: `categories[top_category]` — the model's own asserted severity for its chosen top category (schema-level validation separately enforces this equals the max unsafe severity across categories).

### 7.2 Domain structs

Direct ports of `app/models/*.py` dataclasses (audit §10), field-for-field:

```rust
pub struct FrameModerationResult {
    pub frame_index: i32,
    pub frame_timestamp_seconds: f64,
    pub top_category: String,
    pub is_nsfw: bool,
    pub overall_severity: u8,
    pub categories: HashMap<String, u8>,
    pub reason: String,
    pub raw_response: serde_json::Value, // full parsed model output, incl. its own computed fields
}

pub struct StorageAction {
    pub action_id: String, pub job_id: String, pub video_id: String, pub publisher_user_id: String,
    pub action_type: String, pub threshold: f64, pub final_score: f64,
    pub request_url: String, pub request_body: serde_json::Value,
    pub response_status: Option<i32>, pub response_body: Option<String>,
    pub status: String, pub created_at: DateTime<Utc>, pub completed_at: Option<DateTime<Utc>>,
}

pub struct VideoJob {
    pub job_id: String, pub video_id: String, pub source_object_version: String, pub policy_version: String,
    pub status: VideoJobStatus, pub publisher_user_id: String,
    pub post_id: Option<String>, pub canister_id: Option<String>,
    pub source_video_uri: String, pub upload_event_id: Option<String>, pub trace_id: Option<String>,
    pub attempts: i32, // default 0
    pub last_error_code: Option<String>, pub last_error_message: Option<String>,
    pub created_at: Option<DateTime<Utc>>, pub updated_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>, pub finished_at: Option<DateTime<Utc>>,
}

// Kept as a struct even though nothing currently persists it beyond duration_seconds/
// frames_extracted (see §17 item 3) — width/height/fps/codec_name/has_video_stream
// are computed by ffprobe parsing but discarded in the source today.
pub struct VideoMetadata {
    pub job_id: String, pub video_id: String, pub duration_seconds: f64,
    pub width: Option<i32>, pub height: Option<i32>, pub fps: Option<f64>,
    pub codec_name: Option<String>, pub has_video_stream: bool, pub frames_extracted: i32,
}

pub struct VideoModerationResult {
    pub job_id: String, pub video_id: String, pub policy_version: String,
    pub prompt_version: String, pub aggregation_version: String,
    pub final_is_nsfw: bool, pub final_score: f64, pub final_top_category: String,
    pub max_overall_severity: u8, pub nsfw_frame_count: i32, pub total_frame_count: i32,
    pub move_required: bool, pub move_threshold: f64,
    pub max_category_severities: HashMap<String, u8>,
    pub legacy_nsfw_ec: String, pub legacy_nsfw_gore: String,
    pub final_response: serde_json::Value,
    pub created_at: DateTime<Utc>, pub updated_at: DateTime<Utc>,
}
```

### 7.3 Error handling

One `AppError` enum (`thiserror`), carrying `{ code: ErrorCode, message: String, status: axum::http::StatusCode }` — mirrors Python's `AppError(code, message, status_code)` exactly, including the ad-hoc string codes Python never promoted into its `codes.py` registry (`gpu_not_configured`, `storj_not_configured`, `invalid_image_base64`, `empty_image`, `image_too_large`). `IntoResponse for AppError` produces the identical `{"error":{"code":"...","message":"..."}}` envelope, same status codes, for every code in the table below.

**Complete error code table** (ported verbatim, including the two known inconsistencies — flagged, not fixed, per §5):

| Code | HTTP status | Notes |
|---|---|---|
| `auth_missing_headers` | 401 | also synthesized when FastAPI-equivalent header extraction fails |
| `auth_bad_timestamp` | 401 | non-numeric timestamp |
| `auth_timestamp_out_of_range` | 401 | `abs(now - ts) > max_skew_sec` |
| `auth_bad_signature` | 401 | covers both "secret not configured" and "signature mismatch" — deliberately the same code, avoids leaking config state |
| `not_found` | 404 | video job/result not found in either queue store or Postgres |
| `service_unavailable` | 503 | manual-ban service not configured; **also** reused (inconsistently, preserved per parity) as the catch-all for genuinely unhandled exceptions, which return 500 |
| `queue_unavailable` | 503 | Redis connection/timeout/max-connections errors on detect & status routes |
| `validation_error` | 422 | request schema validation failures |
| `model_moderation_failed` | 503 | GPU retries exhausted |
| `model_response_invalid_json` | 502 | GPU response isn't parseable JSON |
| `model_response_invalid_schema` | 502 | wrong array length, missing fields, bad category values |
| `image_download_failed` | 400 | default status, no override |
| `image_download_timeout` | 504 | after retries exhausted |
| `image_download_upstream_error` | 502 | 5xx from image host after retries exhausted |
| `video_download_empty` | 400 | |
| `video_too_large` | 400 | message includes byte count |
| `video_no_stream` | 400 | |
| `video_probe_failed` | 400 | |
| `video_extraction_failed` | 400 | |
| `gpu_not_configured` | 503 | ad-hoc string in source, not in its `codes.py` |
| `invalid_image_base64` | 400 | ad-hoc string in source |
| `empty_image` | 400 | ad-hoc string in source |
| `image_too_large` | 400 | ad-hoc string in source |
| `storj_not_configured` | 503 | ad-hoc string in source |

Two codes are declared in Python's `codes.py` but never raised anywhere (`not_implemented`, `queue_error`) — carry them in the Rust `ErrorCode` enum for wire/registry completeness, but no call site should ever produce them, matching Python. Separately, an unmatched route (no handler at all, not an `AppError`) falls through to Starlette's framework-level 404 in Python, which produces a differently-shaped body: `{"error":{"code":"404","message":"Not Found"}}` — note `code` is the **status code as a literal string**, not a semantic code, unlike every other row in this table. Axum's equivalent is its `Router::fallback` handler; it must produce this same literal-status-code-as-string shape for parity, not the semantic-code shape used elsewhere.

## 8. Configuration

### 8.1 Static `Settings` (env-loaded, restart required)

Full field list, types, defaults, and env var names/aliases are specified in the source audit and must be ported **exactly**, including two quirks that are easy to silently drop:

- `API_BASE_URL`, `API_KEY`, `MODEL_NAME` each also accept a **trailing-space** alias (`"API_BASE_URL "`, etc.) — a historical `.env` typo compat shim. If the Rust config loader doesn't also accept the trailing-space variant, production config silently breaks on cutover. Preserve it.
- `default_policy_version` and `clickhouse_secondary_database_url` are dead settings in Python (declared, never read). Not worth porting as functioning config — either omit them or keep as inert fields with a comment noting they're vestigial, implementer's choice, but do not wire them to new behavior.

All settings, their exact env var names, types, and defaults are enumerated in the companion audit document's §1 table (`2026-07-21-python-service-source-audit.md`, same directory as this spec) — Phase 1 implementation must transcribe that table directly into the `config` crate rather than re-deriving it from `plan.md`, which is missing several of these (GPU/image retry backoff vars, KVRocks pool retry vars, KVRocks mTLS PEM-vs-path handling).

The Rust `Settings` struct **extends** that table rather than mirroring it 1:1 — it also needs the new Postgres pool-sizing fields from §6.4 (`POSTGRES_POOL_MAX_CONNECTIONS`, `POSTGRES_POOL_MIN_CONNECTIONS`, `POSTGRES_POOL_ACQUIRE_TIMEOUT_SECONDS`) and the runtime-config poll interval from §8.2 (`RUNTIME_CONFIG_POLL_INTERVAL_SECONDS`), neither of which exist in Python. "Transcribe exactly" applies to the audit's existing fields; it does not mean omitting these Rust-only additions.

Helper methods to port: `internal_request_secret()`, `is_kvrocks_configured()`, `is_gpu_configured()`, `is_clickhouse_configured()`, `is_postgres_configured()`.

### 8.2 Runtime-configurable tunables (new — not present in Python)

Explicitly requested and confirmed during design (not an unrequested addition): a second, smaller config surface that's live-tunable via an authenticated admin API without a restart.

```
move_threshold, category_block_thresholds (all 10 non-safe categories),
frame_batch_size, gpu_max_concurrency, gpu_max_attempts, gpu_retry_base_delay_seconds,
queue_max_attempts, clickhouse_flush_batch_size, clickhouse_flush_interval_seconds,
worker_max_concurrent_jobs
```

Secrets, DB/Redis/ClickHouse URLs, and the HMAC secret stay env-only — hot-swapping a connection pool or rotating the HMAC secret via HTTP is a materially different (and riskier) feature not included here.

**Storage & propagation**: canonical copy lives in KVRocks at `nsfw:config:runtime` (JSON, with `version` + `updated_at`). All three binaries (`api`, `video-worker`, `flush-worker`) hold `Arc<RwLock<RuntimeConfig>>`, seeded at startup from KVRocks (falling back to env-derived defaults on first boot / missing key), refreshed on a poll interval (`RUNTIME_CONFIG_POLL_INTERVAL_SECONDS`, default 15s). Every call site that currently reads a tunable from `Settings` in the Python code instead reads from the process's `RuntimeConfig` snapshot.

**Admin API** (new surface):
- `GET /v1/admin/config` — current effective config + version/updated_at.
- `PATCH /v1/admin/config` — partial update, validated before write (`move_threshold` in `0.0..=1.0`, thresholds in `0..=5`), bumps version, writes to KVRocks.
- Same `/v1` HMAC auth as every other endpoint — no second auth tier, since `authz.py` in the source is confirmed empty/reserved and HMAC is the only access control that exists today.
- Every successful `PATCH` is logged (structured log + Sentry breadcrumb) with old/new values and caller context — this can change moderation behavior in production, so it needs an audit trail even without a dedicated audit table.

## 9. API Layer

### 9.1 Routing

- Unauthenticated: `GET /health` (always `200 {"status":"ok"}`), `GET /ready` (seven checks, ported exactly from Python's `ReadinessService`: `internal_auth` (HMAC secret configured — omitting this would mean `/ready` reports healthy even when the service can't authenticate any `/v1` request, silently breaking the wire-parity goal), `postgres`, `kvrocks`, `clickhouse`, `gpu`, `ffmpeg`, `ffprobe`; `200` if all seven ready, `503` otherwise), `GET /openapi.json`, `GET /docs` (Swagger UI — inherits Python's current lack of auth on its docs endpoint; flagged, not newly introduced).
- `RequestIdMiddleware` equivalent: every response (including error responses) gets an `x-request-id` header — either echoed back if the request supplied one, or generated (`Uuid::new_v4()`) if not. Ported as-is; per the audit, Python doesn't thread this ID into log correlation anywhere else today (it's response-header-only), and this spec doesn't add that either — just the header, matching current behavior exactly.
- `/v1/*` — HMAC-gated via an axum `middleware::from_fn` applied at the router level (matches Python's router-level dependency, not a per-route check).

### 9.2 Routes (exact parity)

| Route | Request | Response | Success status |
|---|---|---|---|
| `POST /v1/videos/detect` | `VideoDetectRequest` | `VideoDetectResponse` | `202` |
| `GET /v1/videos/{video_id}/status` | — | `VideoStatusResponse` | `200`; `404` if not found in queue store or Postgres |
| `POST /v1/videos/{video_id}/ban` | `VideoBanRequest` | `VideoBanResponse` | `200` |
| `POST /v1/images/detect-url` | `ImageUrlDetectRequest` | `ModerationDetectResponse` | `200` |
| `POST /v1/images/detect-base64` | `ImageBase64DetectRequest` | `ModerationDetectResponse` | `200` |
| `POST /v1/text/detect` | `TextDetectRequest` | `ModerationDetectResponse` | `200` |
| `GET /v1/admin/config` | — | `RuntimeConfig` + metadata | `200` |
| `PATCH /v1/admin/config` | partial `RuntimeConfig` | updated `RuntimeConfig` + metadata | `200` |

Request/response structs are field-for-field ports of the pydantic schemas (audit §2/§3) — same field names, same defaults (`policy_version` defaults `"nsfw_policy_v1"`, `source_object_version` defaults `""`), same optionality, same `ModerationDetectResponse` self-consistency validation (`overall_severity`/`is_nsfw` re-derived from `categories`/`top_category` and rejected if the caller-supplied values don't match — this schema doubles as a validator in Python and must in Rust too).

### 9.3 Manual ban endpoint — preserved quirk

`POST /v1/videos/{video_id}/ban`: request accepts `publisher_user_id`, `post_id`, `canister_id`, `reason`, `source`, `moderator_id`, but **only `video_id` (path) and `trace_id` are actually used.** `exclusion_reason`/`nsfw_ec`/`nsfw_gore` are hardcoded (`"banned"` / `"explicit"` / `"VERY_UNLIKELY"`) regardless of any request content — no classifier runs. Writes legacy ClickHouse row **first**, `excluded_videos` row **second** (comment in source: avoid publishing exclusion before compatibility data), both synchronously in the request path — this is the one write path in the whole service that isn't queue-async. Preserved exactly per §5; flagged here as a known latent product gap (fields accepted by the API but silently dropped) for the port owner to decide on separately, not fixed as part of this port.

### 9.4 HMAC authentication

The one genuinely tricky part in axum: the signature covers `SHA256(raw_body)`, but `Json<T>` extraction consumes the body. Middleware buffers the full body (`axum::body::to_bytes`), computes `HMAC-SHA256(secret, "{timestamp}\n{METHOD_UPPERCASED}\n{path}\n{sha256_hex(body)}")` (method uppercased inside the signing function, so lowercase-method signing still validates — matches a real source unit test), constant-time-compares via `subtle` or `ring::constant_time`, rejecting malformed-shape signatures (not exactly 64 hex chars) *before* the comparison — then reconstructs the request with the buffered bytes so the downstream `Json<T>` extractor still works.

Header names: `x-internal-timestamp`, `x-internal-signature` (case-insensitive per HTTP, matched lowercase in source). Skew constant: `internal_request_max_skew_sec`, default 300s, symmetric (`abs(now - timestamp) > max_skew`). Missing-secret-configured and bad-signature return the *same* code/message (`auth_bad_signature`) — a deliberate non-disclosure choice in the source, preserved.

## 10. Video Worker Pipeline (`video-worker` binary)

Stage order (per `plan.md`'s "Worker Transaction And Publish Ordering," cross-checked against actual `video_processing_service.py`):

1. **Claim** — `XREADGROUP` against stream `nsfw:queue:video_detection`, group `nsfw_video_workers` (both names ported verbatim). Streams-only; the list-based fallback `plan.md` describes as a contingency was never actually implemented in Python either, so it stays out of scope here too.
2. **Per-job task** (spawned, bounded by `worker_max_concurrent_jobs` semaphore, §6.1):
   - Re-fetch job status; no-op if already terminal (idempotent-redelivery guard).
   - If Postgres already shows `Classified`, sync queue status and return (crash-recovery guard for a crash between Postgres commit and queue-status update).
   - `mark_processing` — increments attempts. **Preserve the dual-counter quirk**: KVRocks and Postgres each track `attempts` independently, kept in sync by convention (both derive from the same base value at each transition) rather than by a single source of truth. Not fixed in this port — flagged as inherited debt.
   - Download: `reqwest` streamed GET, enforce `video_max_bytes` (default 512 MiB) and `video_download_timeout_seconds` (default 120s), **single attempt** — no retry at this layer (retry happens at the whole-job level via queue retry, matching Python). Python's `download_video` has no logging or Sentry call at all today (a `redact_url()` helper exists in `app/utils/redaction.py` but is never called from application code — its only caller is the ad-hoc `scripts/real_video_pipeline_smoke.py`) — so there is no URL-redaction behavior to preserve here; port it as a bare download with no error-path logging, matching current behavior. (URL redaction *does* exist and must be ported, but on the image-download path — see §12.)
   - `ffprobe -v error -print_format json -show_format -show_streams`, timeout `ffprobe_timeout_seconds` (default 30s). Parse duration/width/height/fps/codec/has_video_stream from the first video-typed stream; `NoVideoStreamError` if none.
   - `ffmpeg -loglevel error -i <src> -vf fps=1 -q:v 3 frame-%06d.jpg`, timeout `ffmpeg_timeout_seconds` (default 300s). Batch frames into groups of `frame_batch_size` (default 5, from `RuntimeConfig`). Frame timestamp is derived from position (`frame_index` as float seconds), not actual PTS — an approximation inherited from the source, preserved.
   - GPU batches dispatched **concurrently** (§6.2), each with the jittered retry loop (§6.3, base 0.25s, cap 2.0s, `gpu_max_attempts` default 3), results reassembled into frame order.
   - `aggregate()` (pure function in `core`, §7.1 logic exactly): `final_is_nsfw = any frame is_nsfw`, `final_score = max_overall_severity / 5.0`, `move_required = final_score >= move_threshold` (default 0.8), top-category tie-break via `RISK_ORDER`. Empty frame list is a precondition failure (`Result::Err`, not a panic) — but **not** unconditionally terminal: in Python it's a plain exception with no `AppError` code, which falls into the default-retryable branch of `classify_processing_error` (§10 step 2) same as any other unclassified error, so it only becomes `FailedTerminal` once `queue_max_attempts` is exhausted like a normal failure. Do not special-case it as always-terminal.
   - If `move_required`: call Storj `/move-to-nsfw` **before any DB write**. Failure aborts the attempt with zero writes — no partial state.
   - Single Postgres transaction: insert frame rows + final result row + storage-action row (if move happened) + mark job classified, atomically. Failure → rollback, no ClickHouse/KVRocks publish, queue retries the job.
   - After commit: push 3 JSON payloads to KVRocks list buffers (`nsfw:clickhouse_buffer:video_results`, `:legacy_nsfw_agg`, `:storage_actions`), write `offchain:video_nsfw:{video_id}` compatibility key (no TTL, matching source).
   - Error classification (`classify_processing_error`): `reqwest` status-code-based retryable set (408/429/5xx retryable), `AppError`-based terminal set (the 5 `video_*` error codes are always terminal regardless of attempts), unknown error types default retryable. `retryable = is_retryable && attempts < queue_max_attempts` (default 3, from `RuntimeConfig`). Error message truncated to 1000 chars before storage.
   - Temp dir cleanup on every path (success, every failure mode) — implemented via a drop guard or explicit cleanup-on-all-branches, which is strictly *more* robust than Python's `finally` against a raw process kill mid-cleanup.
3. **Ack/retry/DLQ**: same "ack original delivery, XADD a fresh message" retry shape as Python (not native Streams PEL/XCLAIM) — kept for compatibility with any existing downstream DLQ tooling. `move_video_job_message_to_dlq` on non-retryable failure or on a malformed/missing-job message.
4. **Shutdown**: real fix (§6, listed as a fix not a parity break since Python simply has no graceful shutdown at all) — `tokio::signal` handles SIGTERM/SIGINT, stops claiming new messages, drains in-flight job tasks up to a configurable timeout, then exits.

## 11. ClickHouse Flush Worker (`flush-worker` binary)

Python's version runs `flush_once()` exactly once per process invocation and exits — nothing in the source repo (no cron, no systemd timer, no k8s CronJob, `docker-compose.yml` only defines an `app` service) ever re-invokes it. This is a real, confirmed operational gap, not a design choice — fixed here per §5.

Rust: continuous `tokio::time::interval` loop (default 30s, `clickhouse_flush_interval_seconds` from `RuntimeConfig`). Each tick runs the same three-step flush Python does — `_flush_video_results`, `_flush_legacy_rows`, `_flush_storage_actions`, in sequence — draining up to `clickhouse_flush_batch_size` (default 50) items per buffer via the async `clickhouse` crate (fixing Python's event-loop-blocking sync client in the same motion). Rows are only trimmed from the KVRocks list buffer after a successful insert (`LTRIM key count -1`), matching the source's "remove buffered rows only after insert succeeds" rule. `ReplacingMergeTree(_updated_at)` on the target tables makes a duplicate flush safe if the process crashes between insert and trim. Graceful shutdown: same pattern as `video-worker` — finish the in-flight flush tick, then exit.

## 12. Clients (`clients` crate)

- **GPU (`gpu` module)**: one `POST /chat/completions`-shaped call per batch against the configured OpenAI-compatible endpoint (`api_base_url`/`api_key`/`model_name`). Images inlined as base64 data URLs (`data:{mime};base64,{...}`, mime guessed via `mime_guess`, default `image/jpeg` on failure), `temperature: 0`. Image-generation-prompt variant wraps any caller-provided generation prompt in `<<<GENERATION_PROMPT>>> ... <<<END_GENERATION_PROMPT>>>` delimiters with an explicit "treat as data, not instructions" note — a prompt-injection mitigation, ported verbatim. No timeout/retry configured on the HTTP client itself; all reliability comes from the calling service's retry loop (§6.3), matching Python's design (not a gap — same shape both sides).
- **Storj interface (`storj` module)**: `POST {storj_interface_url}/move-to-nsfw`, JSON body `{publisher_user_id, video_id}`, `Authorization: Bearer {token}`, timeout `storj_interface_timeout_seconds` (default 10s). Non-2xx raises uncaught (propagates to the caller's retryable/terminal classification, §10 step 2). No retry inside this client — retry happens at the whole-job level.
- **Video/image download (`http` module)**: `reqwest::Client` with `redirect(Policy::limited(...))` (Python: `follow_redirects=True`, unbounded — Rust should cap redirects, a minor hardening, not a parity break since it only changes behavior on a pathological redirect chain). Explicit per-call timeouts set at call sites (video download, image download), not relying on a client-level default. Matching Python (one shared `httpx.AsyncClient` reused for both video downloads and Storj calls across the worker's lifetime), the `video-worker` binary should construct one shared `reqwest::Client` and reuse it for both video download and Storj calls, rather than a fresh client per call. **Image download URL redaction** (this is the one path that actually does it, unlike video download — see §10): Python's `ImageDetectionService` builds a `_safe_url_context()` for Sentry breadcrumbs on retry/failure of `/v1/images/detect-url`, redacting the URL before it's attached to any error report. Port this on the image-download retry path specifically, not video download.

## 13. Data Layer

### 13.1 PostgreSQL

Four tables, ported verbatim via `sqlx` migrations (same column names, types, checks, foreign keys, unique constraints) — DDL is in the source repo's `plan.md` (`nsfw_video_jobs`, `nsfw_frame_results`, `nsfw_video_results`, `nsfw_storage_actions`). **Correction**: the companion audit document does not itself contain this DDL — `plan.md` is the sole DDL source; treat "ported verbatim" as "verbatim from `plan.md`," and confirm against the live schema (e.g. `\d` in `psql`) at the start of Phase 3, since `plan.md` predates the implementation and could itself have drifted the same way it drifted on the moderation-threshold logic (§2). Two behaviors to preserve exactly, not obvious from the DDL alone:
- `nsfw_video_jobs` rows are created **lazily** — only when a worker first calls `mark_processing`, not at enqueue time. A job that's queued but never picked up by a worker has a KVRocks entry but **no** Postgres row.
- `max_category_severities` on `nsfw_video_results` is not an independent column — it's reconstructed from the `final_response` JSONB column's nested key on read. Preserve this (don't add a redundant column) unless there's a reason to change it.

### 13.2 ClickHouse

Three existing tables ported verbatim: `yral.video_nsfw_detection`, `yral.video_nsfw_agg` (old-compatible schema, `gcs_video_id` nullable), `yral.video_nsfw_storage_actions`. DDL is in the source repo's `db/clickhouse/*.sql` and in `plan.md` (which states `yral.video_nsfw_detection` was confirmed via a live `DESCRIBE TABLE` at 45 columns) — port verbatim from those files, `ReplacingMergeTree` engine, same partition/order keys.

**Unresolved discrepancy, must be reconciled in Phase 3, not assumed**: the companion audit's read of `app/schemas/clickhouse.py::VideoNsfwDetectionRow` counts **39** explicit Pydantic fields "matching 1:1" against the table (plus the ClickHouse-only `_updated_at` rename, i.e. ~40 total) — 5 short of `plan.md`'s 45-column DDL. This spec does not know which source has drifted (the Pydantic schema might be missing fields that get NULL-defaulted by ClickHouse, or `plan.md`'s DDL might include columns the current code never populates). Phase 3 must run a live `DESCRIBE TABLE yral.video_nsfw_detection` against the actual production ClickHouse instance before finalizing the Rust row struct — do not silently trust either document's column list/count.

**New DDL** (closing a confirmed gap — no `excluded_videos` table exists anywhere in the source repo despite `ExcludedVideoRow` being used in code):

```sql
CREATE TABLE IF NOT EXISTS yral.excluded_videos
(
    video_id String,
    excluded_at DateTime64(3, 'UTC'),
    exclusion_reason String,
    _updated_at DateTime64(3, 'UTC')
)
ENGINE = ReplacingMergeTree(_updated_at)
ORDER BY video_id;
```
This must be confirmed against whatever ClickHouse admin/ops process provisions the other three tables before Phase 3 closes — same "confirm cluster/macros before implementing" guardrail the source `plan.md` applies to its other tables.

### 13.3 KVRocks / Redis

Key scheme ported 1:1: queue stream/group/DLQ names, the three `nsfw:clickhouse_buffer:*` list keys, `offchain:video_nsfw:{video_id}` (no TTL), and the idempotency keys (`nsfw:video_job:<job_id>` hash, `nsfw:video_job_unique:<video_id>:<source_object_version>:<policy_version>`, `nsfw:video_job_by_video_id:<video_id>` set with `NX`).

**Correction**: `plan.md`'s "KVRocks Usage" key list also names `nsfw:gpu:inflight`, intended as a Redis-backed GPU-concurrency coordination key. It was never implemented — no code anywhere in `app/` reads or writes it, and GPU concurrency is controlled purely by an in-process `asyncio.Semaphore(gpu_max_concurrency)` (§6.2 ports this as a single in-process `Arc<Semaphore>`, not a Redis key). This is the same plan-vs-actual-code drift pattern flagged in §2/§5 elsewhere — do not port this key; there is no Redis-based GPU concurrency mechanism to replicate.

**Open decision, not resolved by this spec**: cluster-mode support. Python's enqueue path is atomic (single Redis transaction/pipeline) only in non-cluster mode; in `RedisCluster` mode it issues 4 separate non-atomic calls (no multi-key transactions across hash slots), a real narrow consistency gap. Python also hand-rolls raw `XREADGROUP` protocol calls against cluster nodes because `redis-py`'s high-level cluster client doesn't support blocking reads well. Rust's `redis` crate has similar cluster-mode gaps; whether to (a) mirror Python's manual per-node approach, or (b) switch to the `fred` crate (more complete native cluster+streams support), is a concrete implementation decision for Phase 3/6 — not resolved here since it depends on which KVRocks deployment mode (single-node vs cluster) is actually in use, which should be confirmed before picking.

**Preserved quirk**: `nsfw:video_job_by_video_id:<video_id>` is set with `NX`, so only the *first* job for a given `video_id` claims it — if the same video is resubmitted later with a different `source_object_version`/`policy_version`, `GET /v1/videos/{video_id}/status` may return a stale/earlier job. Preserved per §5, not fixed.

### 13.4 Repository traits (`repositories` crate)

`async-trait`-based traits per the companion audit document's §11 method signatures (`VideoQueueRepository`, `ClickHouseBufferRepository`, `RuntimeNsfwRepository`, `VideoJobRepository`, `VideoJobStateRepository`, `FinalResultUnitOfWork`, `VideoResultRepository`, `FrameResultRepository`, `StorageActionRepository`, plus the four write-only ClickHouse row repositories) — object-safe for `Box<dyn Trait>` swapping between real and in-memory-fake implementations in tests, mirroring Python's `Protocol` + concrete-impl pattern. Method signatures should be transcribed directly from that document rather than re-derived, since several (e.g. `VideoJobStateRepository::mark_processing`'s upsert-then-update snapshot logic) encode non-obvious behavior.

## 14. Testing Strategy

- **Unit** (`#[cfg(test)]` modules, in-crate): all of `core`'s pure logic — `compute_is_nsfw`/`compute_overall_severity` against the exact threshold table (§7.1, especially the `nudity=4` vs `nudity=5` case), legacy mapping (`nsfw_ec`/`nsfw_gore` tables), aggregation tie-break (`RISK_ORDER`), model-response JSON parsing/validation (malformed JSON, wrong array length, out-of-range severities, envelope-unwrapping). `rstest` for the parameterized threshold/mapping cases. `mockall` for repository/client trait mocks in service-level unit tests.
- **Integration** (`tests/` per crate):
  - API-level: axum + `tower::ServiceExt::oneshot` against in-memory fake repositories — parity with Python's `TestClient` + fakes approach, no real DB needed. Covers HMAC validation (valid/invalid/missing/malformed/expired/future-skew), the full route table, idempotent re-enqueue behavior, 404/503 mapping.
  - Repository/worker-level: `testcontainers-rs` spinning up real ephemeral Postgres/ClickHouse/Redis — stronger than Python's actual test suite (which leans entirely on fakes here), matching the source `plan.md`'s own "Phase Testing Standard" (unit + phase integration test + regression + negative-path tests per phase) more literally than Python's current tests do.
- **Coverage target**: 80%+ via `cargo-llvm-cov`, per project convention.

## 15. Deployment & Ops

- `api`: keeps the existing bare-metal + HAProxy + Docker Compose + GHCR + GitHub Actions rolling-deploy shape. New Rust Dockerfile: `cargo build --release` (or a cargo-chef-cached multi-stage build for faster CI), slim runtime image (`debian:bookworm-slim` or similar) with `ffmpeg` installed, same `HEALTHCHECK` pattern (`curl -sf http://127.0.0.1:8080/health`).
- **Worker process supervision — explicitly open, not resolved by this spec** (per your instruction to decide this separately): `video-worker` and `flush-worker` need *some* deployment definition, since Python's never had one. Options to choose between later: additional `docker-compose.yml` services (simplest, consistent with the `api` service already there), systemd units on the same bare-metal hosts, or something else. This is a pre-cutover blocker, called out here so it isn't missed, not answered here.
- `fly.toml`: dropped. Confirmed stale (port mismatch vs. the actual app — a pre-rewrite leftover).
- CI: `build-check.yml` equivalent — `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`. Deploy workflow ports the existing rolling one-server-at-a-time SSH+rsync+compose-pull pattern for the `api` image at minimum; worker images once §15's open item is resolved.

## 16. Cutover Strategy

No in-service shadow-mode subsystem (Python's `plan.md` Phase 7 shadow-mode design leans on a feature flag that lives in the off-chain caller, a separate repo — out of scope here either way). Plan: blue-green at the HAProxy layer — stand up the Rust service on a separate backend, smoke-test against real dependencies (real Postgres/ClickHouse/KVRocks/GPU endpoint, not fakes), flip the HAProxy backend map, keep the Python backend warm for fast rollback. Simpler than replicating Phase 7's shadow-comparison machinery for what is, at its core, a straight port with no new business logic.

## 17. Open Items (explicitly deferred, not resolved by this spec)

1. **Worker process supervision** (§15) — compose service vs. systemd vs. other, left for a separate decision.
2. **KVRocks cluster-mode read path** (§13.3) — `redis` crate manual per-node approach vs. `fred` crate, depends on confirming actual KVRocks deployment mode (single-node vs. cluster) in use.
3. **`VideoMetadata` persistence** — Python computes `width`/`height`/`fps`/`codec_name`/`has_video_stream` via ffprobe but never persists any of it beyond `duration_seconds`/`frames_extracted` into the ClickHouse row. The Rust struct exists (§7.2); whether to actually persist the rest is a product decision, not answered here.
4. **`excluded_videos` DDL** (§13.2) needs sign-off from whoever manages the ClickHouse cluster before Phase 3, same as the other tables required in the source `plan.md`.
5. **Manual ban's discarded fields** (§9.3) — flagged as a likely latent product gap, intentionally not fixed in this port per the parity decision; worth a follow-up product conversation.
6. **Audit-log table for runtime-config changes** (§8.2) — currently spec'd as structured logs + Sentry breadcrumbs only; a dedicated Postgres audit table is a possible follow-up, not included here.

## 18. Phased Implementation Order

Matches the source `plan.md`'s phase discipline (unit tests + a phase integration test + negative-path tests + a short completion note per phase), adapted to the workspace structure:

1. **Workspace skeleton** — `core` crate (domain models, error types, moderation policy, legacy mapping, model-output parsing — all pure, no I/O), static `Settings` loader, CI (`fmt`/`clippy`/`build`/`test`).
2. **API skeleton** — axum app, `/health`/`/ready` against fakes, HMAC middleware, OpenAPI scaffold, error envelope wired through `IntoResponse`.
3. **Data layer** — Postgres migrations (§13.1, reconciled against live schema), ClickHouse DDL incl. new `excluded_videos` and the 39-vs-45-column reconciliation (§13.2), KVRocks key scheme, repository traits + real impls + in-memory fakes (§13.4). **Gate**: confirm actual KVRocks deployment mode (single-node vs. cluster) before this phase closes — it decides the §13.3 `redis` vs. `fred` crate choice, which the queue repository implementation depends on.
4. **Stateless endpoints** — GPU client (§12), `/v1/images/detect-url`, `/v1/images/detect-base64`, `/v1/text/detect`, no persistence.
5. **Video enqueue/status** — `QueueService`, idempotency rules, `POST /v1/videos/detect`, `GET /v1/videos/{video_id}/status`.
6. **Video worker pipeline** — full `video-worker` binary (§10), including the worker-concurrency and concurrent-frame-batch-dispatch improvements (§6.1, §6.2).
7. **Flush worker + manual ban** — `flush-worker` binary as a continuous loop (§11), `POST /v1/videos/{video_id}/ban` (§9.3).
8. **Runtime config admin API** — `RuntimeConfig` store + poll refresh in all three binaries, `GET`/`PATCH /v1/admin/config` (§8.2).
9. **Deployment** — per-binary Dockerfiles, CI, and resolution of the worker-supervision open item (§17.1) before this phase can close.
10. **Cutover** — blue-green HAProxy switch (§16), rollback plan verified.

Each phase should re-run the equivalent of `cargo fmt --check && cargo clippy -- -D warnings && cargo test --workspace` before being marked complete, matching the source repo's `make lint && make test` discipline per phase.
