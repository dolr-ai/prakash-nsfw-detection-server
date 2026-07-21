# NSFW Detection Service — Current Code vs `plan.md`, Full Spec Inputs for Rust Port

Reference document for [2026-07-21-rust-nsfw-detection-port-design.md](2026-07-21-rust-nsfw-detection-port-design.md). Produced by reading the actual Python source (not just its `plan.md`) to catch drift between the original design doc and what's actually implemented. Sections referenced by number from the design spec (e.g. "audit §11") correspond to the numbered headers below.

Repo root: `/Users/prk-jr/Desktop/work/dolr/ansuman-nsfw-detection-server`

---

## 1. Deltas from `plan.md` — Settings / Config

`app/core/config.py` is just a re-export shim (`from app.config.settings import Settings, get_settings`). All real config lives in `app/config/settings.py`, a single `pydantic-settings` `BaseSettings` class, `env_file=".env"`, `extra="ignore"`, `populate_by_name=True`.

Full field list (name / type / default / env alias / required):

| Field | Type | Default | Env alias | Required? |
|---|---|---|---|---|
| `app_name` | str | `"yral-nsfw-detector"` | (field name) | no |
| `environment` | str | `"local"` | (field name) | no |
| `internal_request_hmac_secret` | `SecretStr\|None` | `None` | `INTERNAL_REQUEST_HMAC_SECRET` | soft (auth fails closed if unset) |
| `internal_request_max_skew_sec` | int | `300` | `INTERNAL_REQUEST_MAX_SKEW_SEC` | no |
| `postgres_database_url` | `SecretStr\|None` | `None` | `POSTGRES_DATABASE_URL` | required for worker/status reads |
| `kvrocks_host` | `str\|None` | `None` | `KVROCKS_HOST` | required for durable queue |
| `kvrocks_port` | int | `6379` | `KVROCKS_PORT` | no |
| `kvrocks_password` | `SecretStr\|None` | `None` | `KVROCKS_PASSWORD` | no |
| `kvrocks_tls_enabled` | bool | `False` | `KVROCKS_TLS_ENABLED` | no |
| `kvrocks_cluster_enabled` | bool | `True` | `KVROCKS_CLUSTER_ENABLED` | no |
| `kvrocks_max_connections` | int | `500` | `KVROCKS_MAX_CONNECTIONS` | no |
| `kvrocks_pool_max_attempts` | int | `3` | `KVROCKS_POOL_MAX_ATTEMPTS` | no |
| `kvrocks_pool_retry_base_delay_seconds` | float | `0.05` | `KVROCKS_POOL_RETRY_BASE_DELAY_SECONDS` | no |
| `kvrocks_socket_timeout_seconds` | float | `5.0` | `KVROCKS_SOCKET_TIMEOUT_SECONDS` | no |
| `kvrocks_socket_connect_timeout_seconds` | float | `5.0` | `KVROCKS_SOCKET_CONNECT_TIMEOUT_SECONDS` | no |
| `kvrocks_health_check_interval_seconds` | int | `30` | `KVROCKS_HEALTH_CHECK_INTERVAL_SECONDS` | no |
| `kvrocks_ssl_ca_cert` | `str\|None` | `None` | `KVROCKS_SSL_CA_CERT` | no (PEM text or path, see §7) |
| `kvrocks_ssl_client_cert` | `str\|None` | `None` | `KVROCKS_SSL_CLIENT_CERT` | no |
| `kvrocks_ssl_client_key` | `str\|None` | `None` | `KVROCKS_SSL_CLIENT_KEY` | no |
| `clickhouse_primary_database_url` | `SecretStr\|None` | `None` | `CLICKHOUSE_PRIMARY_DATABASE_URL` | required for CH-backed features |
| `clickhouse_secondary_database_url` | `SecretStr\|None` | `None` | `CLICKHOUSE_SECONDARY_DATABASE_URL` | no (declared, **never read** anywhere else in code) |
| `clickhouse_secure` | bool | `True` | `CLICKHOUSE_SECURE` | no |
| `clickhouse_verify` | bool | `True` | `CLICKHOUSE_VERIFY` | no |
| `clickhouse_database` | str | `"yral"` | `CLICKHOUSE_DATABASE` | no |
| `clickhouse_user` | `SecretStr\|None` | `None` | `CLICKHOUSE_USER` | no (falls back to URL userinfo) |
| `clickhouse_password` | `SecretStr\|None` | `None` | `CLICKHOUSE_PASSWORD` | no (falls back to URL userinfo) |
| `clickhouse_nsfw_table` | str | `"video_nsfw_detection"` | `CLICKHOUSE_NSFW_TABLE` | no |
| `clickhouse_nsfw_agg_table` | str | `"video_nsfw_agg"` | `CLICKHOUSE_NSFW_AGG_TABLE` | no |
| `clickhouse_excluded_videos_table` | str | `"excluded_videos"` | `CLICKHOUSE_EXCLUDED_VIDEOS_TABLE` | no |
| `clickhouse_storage_actions_table` | str | `"video_nsfw_storage_actions"` | *(no alias — plain field name env var)* | no |
| `storj_interface_url` | `str\|None` | `None` | `STORJ_INTERFACE_URL` | required for move |
| `storj_interface_token` | `SecretStr\|None` | `None` | `STORJ_INTERFACE_TOKEN` | required for move |
| `storj_interface_timeout_seconds` | float | `10.0` | *(no alias)* | no |
| `api_base_url` | `str\|None` | `None` | `API_BASE_URL` (also accepts `"API_BASE_URL "` w/ trailing space — legacy typo compat) | required for GPU |
| `api_key` | `SecretStr\|None` | `None` | `API_KEY` / `"API_KEY "` | required for GPU |
| `model_name` | `str\|None` | `None` | `MODEL_NAME` / `"MODEL_NAME "` | required for GPU |
| `model_provider` | str | `"openai-compatible"` | *(field name)* | no |
| `model_version` | `str\|None` | `None` | *(field name)* | no |
| `sentry_dsn` | `SecretStr\|None` | `None` | `SENTRY_DSN` | no |
| `sentry_send_default_pii` | bool | `False` | `SENTRY_SEND_DEFAULT_PII` | no |
| `default_policy_version` | str | `"nsfw_policy_v1"` | *(field name; declared but never referenced elsewhere in code — dead)* | no |
| `visual_prompt_version` | str | `"visual_batch_moderation_v1"` | *(field name)* | no |
| `image_prompt_version` | str | `"image_generation_moderation_v1"` | *(field name)* | no |
| `image_text_prompt_version` | str | `"image_prompt_generation_moderation_v1"` | *(field name)* | no |
| `text_prompt_version` | str | `"text_moderation_v1"` | *(field name)* | no |
| `aggregation_version` | str | `"hard_any_frame_v1"` | *(field name)* | no |
| `frame_batch_size` | int | `5` | *(field name)* | no |
| `gpu_max_concurrency` | int | `5` | *(field name)* | no |
| `gpu_max_attempts` | int | `3` | *(field name)* | no |
| `gpu_retry_base_delay_seconds` | float | `0.25` | `GPU_RETRY_BASE_DELAY_SECONDS` | no |
| `image_max_bytes` | int | `10*1024*1024` (10 MiB) | *(field name)* | no |
| `image_download_timeout_seconds` | float | `30.0` | `IMAGE_DOWNLOAD_TIMEOUT_SECONDS` | no |
| `image_download_max_attempts` | int | `3` | `IMAGE_DOWNLOAD_MAX_ATTEMPTS` | no |
| `image_download_retry_base_delay_seconds` | float | `0.5` | `IMAGE_DOWNLOAD_RETRY_BASE_DELAY_SECONDS` | no |
| `video_download_timeout_seconds` | float | `120.0` | *(field name; no alias)* | no |
| `video_max_bytes` | int | `512*1024*1024` (512 MiB) | *(field name)* | no |
| `video_temp_root` | str | `"/tmp/nsfw"` | *(field name)* | no |
| `ffprobe_timeout_seconds` | float | `30.0` | *(field name)* | no |
| `ffmpeg_timeout_seconds` | float | `300.0` | *(field name)* | no |
| `move_threshold` | float | `0.8` | *(field name)* | no |
| `queue_stream_name` | str | `"nsfw:queue:video_detection"` | *(field name)* | no |
| `queue_group_name` | str | `"nsfw_video_workers"` | *(field name)* | no |
| `queue_consumer_name` | `str\|None` | `None` | `QUEUE_CONSUMER_NAME` | no (defaults to `hostname-pid`) |
| `queue_read_count` | int | `1` | `QUEUE_READ_COUNT` | no |
| `queue_block_ms` | int | `5000` | `QUEUE_BLOCK_MS` | no |
| `queue_max_attempts` | int | `3` | `QUEUE_MAX_ATTEMPTS` | no |
| `queue_dlq_stream_name` | str | `"nsfw:queue:video_detection:dlq"` | *(field name)* | no |
| `clickhouse_buffer_video_results_key` | str | `"nsfw:clickhouse_buffer:video_results"` | *(field name)* | no |
| `clickhouse_buffer_legacy_key` | str | `"nsfw:clickhouse_buffer:legacy_nsfw_agg"` | *(field name)* | no |
| `clickhouse_buffer_storage_actions_key` | str | `"nsfw:clickhouse_buffer:storage_actions"` | *(field name)* | no |
| `runtime_nsfw_key_prefix` | str | `"offchain:video_nsfw:"` | *(field name)* | no |

Helper methods on `Settings`: `internal_request_secret()`, `is_kvrocks_configured()`, `is_gpu_configured()` (`api_base_url and api_key and model_name` all set), `is_clickhouse_configured()`, `is_postgres_configured()`.

**Deltas vs plan.md:**
- Plan doesn't enumerate exact env var names/defaults at all — this is new information the Rust port needs verbatim (especially the trailing-space alias quirks on `API_BASE_URL `, `API_KEY `, `MODEL_NAME ` — these exist because of a historical `.env` typo and must be preserved or the migration will silently break prod config).
- `move_threshold` (0.8) matches plan's "Storj move threshold is `final_score >= 0.8`" locked decision — confirmed no drift.
- `clickhouse_secondary_database_url` is defined but **dead** — never read by any client/service. Not worth porting as a functioning failover unless new behavior is desired.
- `default_policy_version` is also dead config — the actual default `policy_version` comes from the `VideoDetectRequest` schema field default (`"nsfw_policy_v1"`), not from settings.
- Plan doesn't mention GPU/image download retry/backoff env vars, KVRocks pool retry env vars, or KVRocks mTLS PEM-vs-path handling — all of this is new information captured below in §6/§7.

---

## 2. API Surface, Exact

### Routing (`app/api/router.py`)
- `api_router` → includes `routes_health.router` unauthenticated at root.
- `v1_router = APIRouter(prefix="/v1", dependencies=[Depends(require_signed_request)])` — **every** `/v1/*` route requires HMAC auth via a router-level dependency (not per-route). Includes `routes_videos`, `routes_images`, `routes_text`.

### Auth dependency (`app/api/deps.py`)
`require_signed_request` declares `X-Internal-Timestamp` and `X-Internal-Signature` as required FastAPI `Header(...)` params (so **missing headers produce a `RequestValidationError`**, not the `AppError` from `AuthService`, unless both are present — see §9 for how that's special-cased). It then calls `AuthService.authenticate(method, path, headers, raw_body=await request.body())`.

Other deps: `get_settings`, `get_queue_service`, `get_readiness_service`, `get_image_detection_service`, `get_text_detection_service`, `get_video_status_service`, `get_manual_ban_service` (raises `AppError(SERVICE_UNAVAILABLE, ..., 503)` if `app.state.manual_ban_service` is `None`, e.g. ClickHouse not configured).

### `GET /health` (unauthenticated)
Returns `{"status": "ok"}`, always 200.

### `GET /ready` (unauthenticated)
Response model `ReadinessResponse { status: str, dependencies: list[ReadinessDependency] }`, `ReadinessDependency { name, ready: bool, detail: str|None }`. Checks (see §6 `ReadinessService`): `internal_auth`, `postgres`, `kvrocks`, `clickhouse`, `gpu`, `ffmpeg`, `ffprobe`. Returns HTTP 503 if any dependency not ready, else 200. `status` field is `"ready"`/`"not_ready"`.

### `POST /v1/videos/detect`
- Request `VideoDetectRequest`: `job_id: str (min_length=1)`, `video_id: str (min_length=1)`, `publisher_user_id: str (min_length=1)`, `source_video_uri: str (min_length=1)`, `post_id: str|None`, `canister_id: str|None`, `source_object_version: str = ""`, `upload_event_id: str|None`, `upload_created_at: datetime|None`, `policy_version: str = "nsfw_policy_v1" (min_length=1)`, `trace_id: str|None`.
- Response `VideoDetectResponse { job_id, video_id, status: VideoJobStatus, trace_id }`, HTTP `202 Accepted`.
- Errors: if `MaxConnectionsError | RedisConnectionError | RedisTimeoutError` raised by the repo → `AppError(QUEUE_UNAVAILABLE, "queue storage is temporarily unavailable", 503)`.
- Idempotency implemented in `RedisVideoQueueRepository`/`InMemoryVideoQueueRepository.enqueue_video_job`: same `job_id` → return existing job (no re-enqueue); else look up unique key `(video_id, source_object_version, policy_version)` — if an existing job is found in `TERMINAL_VIDEO_STATUSES ∪ {QUEUED, PROCESSING, CLASSIFIED}` return it unqueued (note: this set is effectively "all statuses except..." — actually all 6 statuses land in one of these two sets since `TERMINAL_VIDEO_STATUSES = {CLASSIFIED, FAILED_TERMINAL, SUPERSEDED}` and the explicit extra set adds `QUEUED, PROCESSING, CLASSIFIED}` — so practically **any** existing job for that unique key short-circuits re-enqueue, i.e., `FAILED_RETRYABLE` is the only status *not* explicitly listed but it's also not blocked from re-enqueue... actually since only found-vs-not-found matters and the code returns early whenever the branch condition is true, and `FAILED_RETRYABLE` isn't in the checked set, a `FAILED_RETRYABLE` unique-key hit falls through and creates a **new** job/job_id collision path). This edge case (re-enqueue behavior for `FAILED_RETRYABLE` under the same unique key) should be captured explicitly in the Rust spec since it's subtle.

### `GET /v1/videos/{video_id}/status`
- Response `VideoStatusResponse { job_id, video_id, status, trace_id, attempts: int=0, last_error_code, last_error_message, final_result: VideoFinalResultResponse|None }`.
- `VideoFinalResultResponse { policy_version, prompt_version, aggregation_version, final_is_nsfw, final_score, final_top_category, max_overall_severity, nsfw_frame_count, total_frame_count, move_required, move_threshold, max_category_severities: dict[str,int], legacy_nsfw_ec, legacy_nsfw_gore, final_response: dict }`.
- Logic in `VideoStatusService.get_status_by_video_id`: look up job in queue store first; if none found, fall back to Postgres `get_latest_by_video_id` on `nsfw_video_results` and synthesize a `CLASSIFIED` status response with no `attempts`/`trace_id`/error fields (defaults). If job found and `status == CLASSIFIED`, additionally fetch final result by `job_id` from Postgres.
- 404 (`AppError(NOT_FOUND, "video job not found", 404)`) only if **both** the queue lookup and the Postgres fallback return nothing.
- Same `QUEUE_UNAVAILABLE` 503 mapping as detect route for Redis connection errors.

### `POST /v1/videos/{video_id}/ban`
See §3 below — full detail.

### `POST /v1/images/detect-url`
- Request `ImageUrlDetectRequest { image_url: str (min_length=1), prompt: str|None }`.
- Response `ModerationDetectResponse` (see below).
- Stateless — no DB/queue/storage writes.

### `POST /v1/images/detect-base64`
- Request `ImageBase64DetectRequest { image_base64: str (min_length=1), prompt: str|None }`.
- Same response shape.

### `POST /v1/text/detect`
- Request `TextDetectRequest { text: str (min_length=1) }`.
- Response `ModerationDetectResponse`.

### `ModerationDetectResponse` (`app/schemas/moderation.py`)
```
top_category: Literal[safe, suggestive, nudity, porn, gore, violence, self_harm, hate_or_extremism, drugs, unknown, sexual_minor_content]
is_nsfw: bool
overall_severity: int (0..5)
categories: dict[str, int]   # exactly the 11 categories, each 0..5, non-bool int
reason: str
```
Model validator re-derives `overall_severity` from `compute_overall_severity(top_category, categories)` and `is_nsfw` from `compute_is_nsfw(categories)` and **raises `ValueError`** if the caller-supplied values don't match — this schema is used both for outbound API responses and, implicitly, doubles as a self-consistency check (Rust should replicate this validation exactly, not just trust upstream fields).

### `app/schemas/auth.py`
`SignedRequestContext { timestamp: int }` — the only return value of `require_signed_request`; not otherwise used downstream (no per-request auth context threading beyond the dependency check itself).

### `app/schemas/common.py`
`ErrorBody { code: str, message: str }`, `ErrorResponse { error: ErrorBody }` (this is literally the exact shape returned by all error handlers), `ReadinessDependency`, `ReadinessResponse` (above).

### `app/schemas/model_output.py`
- `ModerationModelOutput { top_category, categories: dict[str,int], reason: str, +computed overall_severity, +computed is_nsfw }` with validators: `validate_categories` (exactly the 11 keys, each int 0..5, no bools), `top_category_matches_scores` (if `top_category=="safe"` then all unsafe categories must be 0; else `categories[top_category]` must be >0 and must equal the max unsafe severity — ties are allowed, but `top_category`'s own severity can't be *lower* than another category's).
- `FrameModerationOutput(ModerationModelOutput)` adds `frame_index: int (ge=0)`.
- `TextModerationOutput(ModerationModelOutput)` — no extra fields.
- `parse_visual_batch_response(raw_response, expected_count)`: strips BOM/whitespace, tries direct `json.loads`; on failure, uses `_extract_single_json_document` to scan the string for the **first** balanced JSON `{`/`[` document via `JSONDecoder.raw_decode`, and requires that no *second* independent JSON document exists after it (if a second dict/list is found, treats it as ambiguous → raises `MODEL_RESPONSE_INVALID_JSON`). Then unwraps a single-key envelope if the top-level key is one of `("results","frames","result")`. If `expected_count==1` and payload is a dict containing `"frame_index"`, wraps it in a list. Requires resulting list length == `expected_count`, and each item's `frame_index` must equal its list position (else `MODEL_RESPONSE_INVALID_SCHEMA`, HTTP 502).
- `parse_text_moderation_response(raw_response)`: same JSON extraction, unwraps envelope keys `("result","moderation")`, must be a dict, validates against `TextModerationOutput`.

### `app/schemas/clickhouse.py`
- `VideoNsfwDetectionRow` — 39 explicit fields mirroring `yral.video_nsfw_detection` (see §12 ClickHouse DDL below for exact column list/types — Pydantic field set matches 1:1 except Pydantic omits the ClickHouse-only `_updated_at` column name, which `ClickHouseRepository.insert_model_rows` renames from `updated_at_replacing` at insert time).
- `ExcludedVideoRow { video_id: str, excluded_at: datetime, exclusion_reason: str, updated_at_replacing: datetime }` — **note:** no corresponding CREATE TABLE DDL exists anywhere in this repo (see §12 gap).

### `app/schemas/legacy.py`
`LegacyNsfwAggRow { video_id: str, gcs_video_id: str|None, nsfw_ec: str|None, nsfw_gore: str|None, is_nsfw: bool, probability: float }`.

### `app/schemas/storage_action.py`
`StorjMoveResponse { status_code: int, body: str }`, `StorageActionRow { action_id, video_id, job_id, publisher_user_id, action_type, threshold: float, final_score: float, status, request_url, request_body_json: str, response_status: int|None, response_body: str, created_at, completed_at: datetime|None, updated_at_replacing }`.

### `app/schemas/video.py`
Covered above (`VideoDetectRequest/Response`, `VideoStatusResponse`, `VideoFinalResultResponse`, `VideoBanRequest/Response`).

---

## 3. Manual Ban Endpoint — Exact Behavior

`POST /v1/videos/{video_id}/ban` → `app/api/v1/routes_videos.py::ban_video` → `ManualBanService.ban_video` (`app/services/manual_ban_service.py`).

Request `VideoBanRequest`:
```
publisher_user_id: str (min_length=1)
post_id: str (min_length=1)
canister_id: str (min_length=1)
reason: str = "user_report_approved" (min_length=1)
source: str = "google_chat" (min_length=1)
moderator_id: str|None = None
trace_id: str|None = None
```
**Important:** `publisher_user_id`, `post_id`, `canister_id`, `reason`, `source`, `moderator_id` are all accepted/validated but **none of them are persisted or used** by `ManualBanService.ban_video` — only `video_id` (path param) and `request.trace_id` (echoed back in response) are actually consumed. The exclusion reason written to ClickHouse is hardcoded to the literal string `"banned"`, not `request.reason`. This must be preserved (or deliberately fixed) in the Rust port — flag this to the port owner since it looks like a latent bug/incomplete feature (fields accepted by the API but silently dropped).

Response `VideoBanResponse { video_id, status: str, excluded_videos_written: bool, legacy_nsfw_agg_written: bool, trace_id: str|None }`.

Exact logic:
```python
now = datetime.now(UTC)
excluded_row = ExcludedVideoRow(video_id=video_id, excluded_at=now, exclusion_reason="banned", updated_at_replacing=now)
legacy_row = LegacyNsfwAggRow(video_id=video_id, gcs_video_id=None, nsfw_ec="explicit", nsfw_gore="VERY_UNLIKELY", is_nsfw=True, probability=1.0)

# writes legacy table FIRST, excluded_videos table SECOND — comment explains why:
# "Write the recsys exclusion last so a partial failure does not publish exclusion before compatibility data."
await to_thread.run_sync(legacy_repository.insert_rows, settings.clickhouse_nsfw_agg_table, [legacy_row])
await to_thread.run_sync(excluded_videos_repository.insert_rows, settings.clickhouse_excluded_videos_table, [excluded_row])

return ManualBanResult(video_id=video_id, status="banned", excluded_videos_written=True, legacy_nsfw_agg_written=True, trace_id=request.trace_id)
```
Both writes use `clickhouse_connect`'s synchronous `client.insert(...)` wrapped via `anyio.to_thread.run_sync` (blocking client run off the event loop thread pool), i.e. **no retries**, and this is fully **synchronous within the HTTP request** (unlike the async video detect flow — this is the defining architectural difference: ban is a direct, immediate, two-table ClickHouse write in the request path; detect is queue → async worker → buffered flush).

`nsfw_ec`/`nsfw_gore` values are **hardcoded**, not derived via `map_legacy_nsfw_ec`/`map_legacy_nsfw_gore` — a manual ban always writes `"explicit"` / `"VERY_UNLIKELY"` regardless of any actual content classification (there is none — no classifier is invoked for a ban).

`ManualBanService` is only constructed (`app/core/lifecycle.py::build_manual_ban_service`) when `settings.is_clickhouse_configured()` is true; if ClickHouse client init raises `ClickHouseError`, the service is disabled (`None`) and the dependency raises `503 service_unavailable`.

`app.state.manual_ban_service` can also be injected directly in `create_app(...)` for tests (bypasses the ClickHouse-configured check).

`ClickHouseExcludedVideosRepository.insert_rows` / `ClickHouseLegacyNsfwAggRepository.insert_rows` are trivial one-liners delegating to `ClickHouseRepository.insert_model_rows` (`app/repositories/clickhouse/base.py`), which: no-ops on empty list; converts each Pydantic row via `model_dump(mode="json")`; if the dumped payload has key `updated_at_replacing`, renames it to `_updated_at` (matching the ClickHouse `ReplacingMergeTree` version column convention); calls `client.insert(f"{database}.{table_name}", values, column_names=columns)` where `values` is a list-of-lists built from `columns = list(payload[0].keys())` (**column order is derived from the first row's dict key order** — since all rows come from the same Pydantic model this is stable, but it's worth noting for a Rust port that column order must be deterministic/model-derived, not row-derived).

---

## 4. Auth/Authz

### `app/core/security.py` (pure functions, framework-agnostic)
```python
SIGNATURE_HEX_LENGTH = 64
HEX_DIGITS = set(string.hexdigits)  # includes both cases

def body_sha256(body: bytes) -> str:
    return hashlib.sha256(body).hexdigest()

def build_signature_message(*, timestamp: str, method: str, path: str, body: bytes) -> bytes:
    return "\n".join((timestamp, method.upper(), path, body_sha256(body))).encode("utf-8")

def signature_has_valid_shape(signature: str) -> bool:
    return len(signature) == 64 and all(c in HEX_DIGITS for c in signature)

def sign_request(secret, *, timestamp, method, path, body) -> str:
    return hmac.new(secret.encode("utf-8"), build_signature_message(...), hashlib.sha256).hexdigest()

def verify_signature(secret, *, timestamp, method, path, body, signature) -> bool:
    if not signature_has_valid_shape(signature):
        return False
    expected = sign_request(secret, timestamp=timestamp, method=method, path=path, body=body)
    return hmac.compare_digest(expected, signature)
```
Signature message format is exactly `"{timestamp}\n{METHOD_UPPERCASED}\n{path}\n{sha256_hex(body)}"` UTF-8 bytes. **Note `method.upper()` is applied inside `build_signature_message`**, so callers may sign with lowercase method and it will still validate — this is confirmed by a unit test (`test_security.py::test_signature_message_uses_expected_shape` signs with `method="post"` lowercase). `path` is the raw request path only (no query string, e.g. `request.url.path`), not host/scheme.

Constant-time comparison via `hmac.compare_digest`. Malformed signatures (wrong length or non-hex chars) are rejected **before** reaching `compare_digest` — matches plan's "reject malformed signatures before constant-time comparison" rule exactly.

### `app/middleware/signed_request.py`
```python
TIMESTAMP_HEADER = "x-internal-timestamp"
SIGNATURE_HEADER = "x-internal-signature"
SIGNED_REQUEST_HEADERS = (TIMESTAMP_HEADER, SIGNATURE_HEADER)
```
(Despite the filename, this is not Starlette middleware — it's just header-name constants; the real "middleware" behavior is a FastAPI router dependency.)

### `app/services/auth_service.py::AuthService.authenticate`
```python
missing = [h for h in SIGNED_REQUEST_HEADERS if not headers.get(h)]
if missing: raise AppError(AUTH_MISSING_HEADERS, "missing internal auth headers", 401)

secret = settings.internal_request_secret()
if secret is None: raise AppError(AUTH_BAD_SIGNATURE, "invalid internal signature", 401)  # deliberately same code/message as bad-sig, not a distinct "not configured" error — avoids info leak

timestamp = int(timestamp_raw)  # ValueError -> AppError(AUTH_BAD_TIMESTAMP, "timestamp must be unix seconds", 401)

now = int(time.time())
if abs(now - timestamp) > settings.internal_request_max_skew_sec:
    raise AppError(AUTH_TIMESTAMP_OUT_OF_RANGE, "stale internal request timestamp", 401)

if not verify_signature(secret, timestamp=timestamp_raw, method=method, path=path, body=raw_body, signature=signature):
    raise AppError(AUTH_BAD_SIGNATURE, "invalid internal signature", 401)

return SignedRequestContext(timestamp=timestamp)
```
Clock skew constant: `internal_request_max_skew_sec` (default **300 seconds**, symmetric — both future and past timestamps beyond 300s are rejected, using `abs(now - timestamp)`).

All auth failures return **401**. Codes: `auth_missing_headers`, `auth_bad_timestamp`, `auth_timestamp_out_of_range`, `auth_bad_signature`.

Because FastAPI declares the two headers as required `Header(...)` params on `require_signed_request`, a genuinely missing header actually triggers Pydantic's `RequestValidationError` (422 by default) **before** `AuthService.authenticate` ever runs — the error handler `request_validation_error_handler` (see §9) special-cases this to still return `401 auth_missing_headers`, keeping behavior consistent with `AuthService`'s own missing-header check (which is otherwise dead code in this exact path but documents intent / is exercised directly in unit tests of `AuthService`).

### `app/services/authz.py`
Empty file — only a module docstring: `"""Reserved for future authorization rules distinct from request signing."""`. **There is no second authorization layer today** — HMAC validity is the only access control. No RBAC/scopes/roles exist in the current codebase; the Rust port needs no additional authz abstraction beyond what's specified here unless new requirements are introduced.

---

## 5. Moderation Policy & Legacy Mapping — Exact Current Logic

### `app/services/moderation_policy.py`
```python
CATEGORY_BLOCK_THRESHOLDS: dict[str, int] = {
    "porn": 4,
    "sexual_minor_content": 3,
    "gore": 4,
    "violence": 4,
    "self_harm": 4,
    "hate_or_extremism": 4,
    "drugs": 4,
    "unknown": 4,
    "suggestive": 5,
    "nudity": 5,
}

def compute_overall_severity(top_category, categories) -> int:
    if top_category not in MODERATION_CATEGORIES: raise ValueError(...)
    return int(categories[top_category])

def compute_is_nsfw(categories) -> bool:
    return any(int(categories.get(cat, 0)) >= threshold for cat, threshold in CATEGORY_BLOCK_THRESHOLDS.items())
```

**This is the single biggest delta vs `plan.md`.** Plan's "Frame To Video Policy" section states:
```
frame_is_nsfw = model_response.is_nsfw OR top_category in unsafe_categories OR overall_severity >= 3
```
The **actual implementation does not use that rule at all**. `is_nsfw` (both at the per-frame moderation-model-output level, via `ModerationModelOutput.is_nsfw` computed field, and at the video-response level) is derived purely from **per-category severity thresholds**, independent of `top_category` or `overall_severity`:

| Category | Block threshold (>=) |
|---|---|
| `sexual_minor_content` | 3 |
| `porn`, `gore`, `violence`, `self_harm`, `hate_or_extremism`, `drugs`, `unknown` | 4 |
| `suggestive`, `nudity` | 5 |
| `safe` | (not in table — never triggers) |

Confirmed by `tests/unit/services/test_moderation_policy.py`:
```python
compute_is_nsfw(categories(porn=4)) is True
compute_is_nsfw(categories(sexual_minor_content=3)) is True
compute_is_nsfw(categories(gore=4)) is True
compute_is_nsfw(categories(nudity=4, suggestive=4)) is False   # nudity/suggestive need severity 5, not 4
compute_is_nsfw(categories(nudity=5)) is True
compute_is_nsfw(categories(safe=5)) is False
```
So e.g. `nudity=4` (a "strong" severity!) does **not** flag a frame/video as NSFW under current code — only `nudity=5` ("explicit or severe") does. This is materially different from plan's `overall_severity >= 3` blanket rule, which would have flagged `nudity=4` as NSFW. **The Rust port must replicate the exact `CATEGORY_BLOCK_THRESHOLDS` table above, not the plan.md prose rule.**

`compute_overall_severity` = `categories[top_category]` — i.e., it's just the model's asserted severity for its own chosen top category (the schema-level validator `ModerationModelOutput.top_category_matches_scores`, §2, additionally constrains `top_category`'s severity must equal the max unsafe severity across all categories, so in practice this equals `max(unsafe category severities)`).

### `app/services/legacy_mapping_service.py`
```python
def map_legacy_nsfw_ec(final_top_category: str) -> str:
    mapping = {"porn": "explicit", "nudity": "nudity", "suggestive": "provocative", "sexual_minor_content": "explicit"}
    return mapping.get(final_top_category, "neutral")

def map_legacy_nsfw_gore(max_category_severities: dict[str, int]) -> str:
    severity = max(max_category_severities.get("gore", 0), max_category_severities.get("violence", 0))
    if severity >= 5: return "VERY_LIKELY"
    if severity >= 4: return "LIKELY"
    if severity >= 3: return "POSSIBLE"
    if severity >= 1: return "UNLIKELY"
    return "VERY_UNLIKELY"

def to_legacy_nsfw_agg(result: VideoModerationResult, historical_gcs_video_id: str | None = None) -> LegacyNsfwAggRow:
    return LegacyNsfwAggRow(
        video_id=result.video_id, gcs_video_id=historical_gcs_video_id,
        nsfw_ec=result.legacy_nsfw_ec, nsfw_gore=result.legacy_nsfw_gore,
        is_nsfw=result.final_is_nsfw, probability=result.final_score,
    )
```
This **matches plan.md's `nsfw_ec`/`nsfw_gore` mapping tables exactly** — no drift here. Note `to_legacy_nsfw_agg` reads `result.legacy_nsfw_ec`/`result.legacy_nsfw_gore` (pre-computed fields already stored on `VideoModerationResult` by `AggregationService`) rather than recomputing from `result` — the actual mapping call happens once, in `AggregationService.aggregate`, not in `to_legacy_nsfw_agg` itself (a harmless internal refactor vs. plan's pseudocode, not a behavior change).

### `app/services/aggregation_service.py` — Video-level aggregation (`AggregationService.aggregate`)
```python
if not frames: raise ValueError("cannot aggregate an empty frame list")

max_category_severities = {cat: max(f.categories.get(cat, 0) for f in frames) for cat in MODERATION_CATEGORIES}  # all 11 categories, always present
nsfw_frame_count = sum(1 for f in frames if f.is_nsfw)   # frame.is_nsfw was computed via compute_is_nsfw, i.e. the category-threshold rule
max_overall_severity = max(f.overall_severity for f in frames)
final_is_nsfw = nsfw_frame_count > 0                      # "any frame NSFW -> video NSFW", matches plan's "hard any-frame" rule
final_score = max_overall_severity / 5.0
final_top_category = self._select_top_category(frames)   # see below
move_required = final_score >= settings.move_threshold    # default 0.8
legacy_nsfw_ec = map_legacy_nsfw_ec(final_top_category)
legacy_nsfw_gore = map_legacy_nsfw_gore(max_category_severities)
```
`_select_top_category` (tie-break logic):
```python
risk_rank = {category: index for index, category in enumerate(RISK_ORDER)}
best = max(frames, key=lambda f: (f.overall_severity, -risk_rank.get(f.top_category, len(RISK_ORDER))))
highest_severity = best.overall_severity
candidates = [f.top_category for f in frames if f.overall_severity == highest_severity]
return min(candidates, key=lambda cat: risk_rank.get(cat, len(RISK_ORDER)))
```
`RISK_ORDER = (sexual_minor_content, porn, nudity, gore, violence, self_harm, hate_or_extremism, drugs, suggestive, unknown, safe)` — this **matches plan.md's tie-break order exactly**. Implementation detail: it first finds the max `overall_severity` across all frames (breaking internal max-ties by risk order to pick an initial candidate `best`), then collects **every** frame category at that severity level and picks the risk-highest (lowest `risk_rank` index) among them — effectively: highest severity wins; among frames tied at highest severity, the riskiest category wins. `aggregation_version` field is set from `settings.aggregation_version` (default `"hard_any_frame_v1"`), and `prompt_version` is set from `settings.visual_prompt_version` (not the frame/text prompt — always the visual batch prompt version, even though `AggregationService` is only invoked from the video pipeline so this is consistent).

`AggregationService.aggregate` requires non-empty `frames` (raises plain `ValueError`, not `AppError` — this propagates up through `VideoJobProcessor._process_video` and gets caught by the generic `except Exception` in `process()`, classified via `classify_processing_error`, which defaults unknown/non-`AppError`/non-httpx exceptions to **retryable=True** unless attempts are exhausted).

---

## 6. Video Pipeline Services

### `FrameExtractionService` (`app/services/frame_extraction_service.py`)
Public methods: `prepare_job_dir(job_id) -> Path` (calls `cleanup_dir` then creates `<job_dir>/frames`), `probe(*, job_id, video_id, source_path) -> VideoMetadata`, `extract_frames(source_path, frames_dir) -> list[ExtractedFrame]`.
- `download_video(source_url, output_path, settings, http_client=None)` — module-level function (not on the class): streams via `httpx.AsyncClient(timeout=httpx.Timeout(settings.video_download_timeout_seconds), follow_redirects=True)`, writes chunks to disk, raises `VideoTooLargeError(bytes_written)` mid-stream if `bytes_written > settings.video_max_bytes` (512 MiB default), raises `EmptyVideoDownloadError()` if 0 bytes total. **No retry** on download (single attempt; retries happen at the queue/job level, not inside this function).
- `probe`: runs `ffprobe -v error -print_format json -show_format -show_streams <path>` with `timeout_seconds=settings.ffprobe_timeout_seconds` (30s default). `TimeoutError`→`VideoProbeError("ffprobe timed out")`; `RuntimeError` (non-zero exit)→`VideoProbeError(str(exc))`; JSON decode failure→`VideoProbeError("ffprobe returned invalid JSON")`. `parse_ffprobe_metadata` picks the first stream with `codec_type=="video"` (raises `NoVideoStreamError()` if none); `duration` from stream duration or format duration, defaults to `0.0` if unparseable; `fps` parsed from `avg_frame_rate` or `r_frame_rate` as `"num/den"` fraction (returns `None` on `"0/0"` or empty or unparseable).
- `extract_frames`: runs `ffmpeg -loglevel error -i <source> -vf fps=1 -q:v 3 <frames_dir>/frame-%06d.jpg` with `timeout_seconds=settings.ffmpeg_timeout_seconds` (300s default). `TimeoutError`→`VideoExtractionError("ffmpeg timed out")`; non-zero exit→`VideoExtractionError(str(exc))`; zero frames produced→`VideoExtractionError("ffmpeg produced no frames")`.
- `frame_batches(frames, batch_size=5)`: simple contiguous chunking, `frames[i:i+batch_size]`.
- `frames_from_paths(paths)`: sorts paths lexically (frame filenames are zero-padded `%06d` so lexical sort == numeric sort), assigns `frame_index = i` and `timestamp_seconds = float(i)` (i.e. timestamp is **derived from position, not from actual ffmpeg PTS** — since extraction is fixed at `fps=1`, index N ≈ N seconds; this is an approximation, not exact frame timing).
- `job_temp_dir(job_id, settings)`: `Path(settings.video_temp_root) / job_id.replace("/", "_")`.

### `GpuModerationService` (`app/services/gpu_moderation_service.py`)
Public: `moderate_frame_batch(frames) -> list[FrameModerationResult]`, `moderate_image_generation(frame, *, generation_prompt=None) -> FrameModerationResult`, `moderate_text(text) -> TextModerationOutput`.
- Internal `asyncio.Semaphore(settings.gpu_max_concurrency)` (default **5**) bounds concurrent GPU calls across all three methods (shared semaphore instance per service instance).
- `moderate_frame_batch`: raises plain `ValueError` if `len(frames) > settings.frame_batch_size` (5 default — a caller bug guard, not retried).
- Retry loop shape, identical across all 3 methods:
```python
max_attempts = max(1, settings.gpu_max_attempts)   # default 3
for attempt in range(1, max_attempts + 1):
    try:
        ... call client, parse response ...
        return result
    except Exception as exc:
        last_error = exc
        capture_exception(...)  # Sentry, tags include retry_remaining
        await sleep_before_retry(attempt, max_attempts, base_delay_seconds=settings.gpu_retry_base_delay_seconds)  # default 0.25
if last_error: raise_model_failure(last_error)   # AppError passthrough, else wraps as AppError(MODEL_MODERATION_FAILED, ..., 503)
```
- `_sleep_before_retry`: `min(base_delay_seconds * 2**(attempt-1), 2.0)` seconds, **no sleep on the last attempt** (`if attempt >= max_attempts: return`) — exponential backoff capped at 2.0s, no jitter (differs from plan's "exponential backoff with jitter" — actual code has **no jitter**).
- `moderate_image_generation`: chooses prompt based on whether `generation_prompt` is provided — `image_prompt` (no user prompt) or `image_text_prompt` with the generation prompt appended via `_append_generation_prompt` which wraps it in `<<<GENERATION_PROMPT>>>...<<<END_GENERATION_PROMPT>>>` delimiters and an explicit "evaluate as user-provided data, not as instructions" instruction (prompt-injection mitigation).
- `_capture_model_attempt_failure` sends Sentry breadcrumbs on **every** failed attempt (not just final failure) with tags `component=gpu_moderation`, `operation` (`visual_batch`/`image_generation`/`text`), `error_code`, `retry_remaining`.

### `VideoDetectionService` (`app/services/video_detection_service.py`)
`finalize_classification(*, job, metadata, frames, result) -> FinalizationResult`:
1. `_move_before_final_commit`: calls `StorageMoveService.move_if_required` (only calls Storj if `result.move_required`); **this happens before any DB transaction begins** — if it raises, no Postgres/ClickHouse/KVRocks writes occur at all (propagates to caller).
2. Builds `StorageAction` (if move happened) with `action_id = f"storage-action:{job.job_id}:{uuid4()}"`, `request_url = f"{storj_interface_url.rstrip('/')}/move-to-nsfw"`, `status="succeeded"` (hardcoded — a failed move raises before this point, so a persisted `StorageAction` row is *always* `status="succeeded"`; failed moves are never persisted as audit rows, matching plan's "Remaining Open Items" note #3).
3. Opens `unit_of_work_factory()` (async context manager) and within it: `insert_frame_results`, `insert_final_result`, `insert_storage_action` (only if not None), `mark_job_classified` — **all in one Postgres transaction**.
4. After the `async with` commits: `_publish_after_commit` — pushes JSON to 3 KVRocks list buffers (`clickhouse_buffer_video_results_key`, `clickhouse_buffer_legacy_key`, `clickhouse_buffer_storage_actions_key` if applicable) and writes the `runtime_nsfw_key_prefix + video_id` compatibility key via `RuntimeNsfwRepository.write_result`.
- `runtime_nsfw_payload`: `{video_id, is_nsfw, probability: final_score, nsfw_ec, nsfw_gore, policy_version, status: "classified"}`.
- `to_clickhouse_video_row`: builds the full 39-field `VideoNsfwDetectionRow`; `frame_results_json` = `json.dumps([f.raw_response for f in frames], separators=(",",":"))` (compact, no spaces); `storj_move_status` = `storage_action.status if storage_action else "not_required"`.

### `VideoJobProcessor` / `classify_processing_error` (`app/services/video_processing_service.py`)
`process(job)`:
1. Re-fetch current job status from queue; if already in `TERMINAL_VIDEO_STATUSES` → no-op return (idempotent re-delivery guard).
2. If Postgres job state already `CLASSIFIED` → sync queue status to `CLASSIFIED` and return (recovers from a crash between Postgres commit and queue-status update / KVRocks buffer push).
3. `update_status(job_id, PROCESSING)` (increments `attempts` — see `RedisVideoQueueRepository.update_status`: `attempts = job.attempts + 1 if status == PROCESSING else job.attempts`).
4. `job_state_repository.mark_processing(processing_job)` — Postgres upsert-then-update (inserts a row with `on_conflict_do_nothing()` using `attempts = max(job.attempts-1, 0)` as a "just-queued" snapshot, i.e. re-derives the pre-processing attempts value, then calls `mark_processing(job_id)` which increments attempts by 1 in SQL — this dual-increment (once in Redis, once again logically in Postgres via `attempts + 1`) means **attempts as tracked by Redis and Postgres both increment independently but should stay in sync** since both start from the same base; still, worth flagging for a Rust port that Postgres and the queue store maintain separate `attempts` counters that must be kept consistent by convention, not by a single source of truth).
5. On success: full pipeline (`download_video` → `probe` → `extract_frames` → batched `moderate_frame_batch` → `aggregation_service.aggregate` → `detection_service.finalize_classification` → `update_status(CLASSIFIED)`), always followed by `cleanup_dir(job_temp_dir(...))` in `finally`.
6. On any exception: `classify_processing_error(exc, attempts=processing_job.attempts, max_attempts=settings.queue_max_attempts)` (default max_attempts=3), then `queue_service.update_status(job_id, failure.status, last_error_code, last_error_message)` and `job_state_repository.mark_failed(...)`, then re-raises as `VideoJobProcessingError(failure)`.

`classify_processing_error` / `_is_retryable`:
```python
TERMINAL_PROCESSING_ERROR_CODES = {VIDEO_DOWNLOAD_EMPTY, VIDEO_TOO_LARGE, VIDEO_NO_STREAM, VIDEO_PROBE_FAILED, VIDEO_EXTRACTION_FAILED}

def _is_retryable(exc):
    if httpx.HTTPStatusError: return status_code in {408, 429} or status_code >= 500
    if httpx.RequestError: return True
    if AppError: return exc.code not in TERMINAL_PROCESSING_ERROR_CODES and exc.status_code >= 500
    return True   # unknown exception types default to retryable
retryable = _is_retryable(exc) and attempts < max_attempts
status = FAILED_RETRYABLE if retryable else FAILED_TERMINAL
```
Error message is truncated to **1000 chars** (`message[:1000]`) before storage.

### `QueueService` (`app/services/queue_service.py`)
Thin façade over `VideoQueueRepository`. `_read_with_pool_retry` wraps `get_status_by_video_id`/`get_status_by_job_id` (read-only ops) with retry-on-`MaxConnectionsError`: `pool_max_attempts` (default 3, from `kvrocks_pool_max_attempts`), backoff `min(pool_retry_base_delay_seconds * 2**(attempt-1), 1.0)` (default base 0.05s, capped at 1.0s, **no jitter**). Write/enqueue/update paths do **not** get this pool-retry wrapper (only reads).

### `AggregationService` — see §5 above.

### `ClickHouseFlushService` (`app/services/clickhouse_flush_service.py`)
`flush_once()` runs 3 independent flush steps sequentially: `_flush_video_results`, `_flush_legacy_rows`, `_flush_storage_actions`. Each: `read_batch(key, batch_size)` (default **batch_size=50**, constructor param) from the KVRocks list buffer → parse each JSON item into the corresponding Pydantic row model → `repository.insert_rows(table, parsed)` (sync ClickHouse client call, no `to_thread` wrapping here unlike the manual ban path — **this call blocks the asyncio event loop** since `clickhouse_connect`'s `client.insert` is synchronous) → `trim_batch(key, len(rows))` which does `LTRIM key count -1` (i.e., removes only the **first `len(rows)`** items — rows are only removed after a successful insert, matching plan's "remove buffered rows only after insert succeeds"). **No batching loop / no re-poll if more than `batch_size` items remain** — `flush_once()` drains at most `batch_size` items per buffer per call; if more are queued, they wait for the next external invocation.

**Critical operational note**: `app/workers/clickhouse_flush_worker.py::run()` calls `flush_service.flush_once()` **exactly once and then the process exits** — it is not a loop (see §8). Nothing in this repo re-invokes it on an interval; `Makefile`'s `flush-worker` target and `deploy-baremetal/docker-compose.yml` (which defines **only an `app` service**, no `worker`/`flush-worker` service) leave this unscheduled. **This is a real deployment gap**: there is no cron/systemd-timer/supervisor config anywhere in the repo that periodically invokes `python -m app.workers.clickhouse_flush_worker`, nor is `python -m app.workers.video_worker` run anywhere in the baremetal compose file or fly.toml. The Rust port spec should either (a) explicitly design a continuous-loop flush worker (departing from this one-shot design) or (b) replicate the one-shot-plus-external-scheduler design and the scheduler needs to be specified as new infrastructure, since it doesn't exist in this repo.

### `StorageMoveService` (`app/services/storage_move_service.py`)
`move_if_required(*, result, publisher_user_id)`: returns `None` if `not result.move_required`; else calls `storj_client.move_to_nsfw(publisher_user_id, result.video_id)`. No retry logic at this layer (retries happen at the whole-job level via queue retry).

---

## 7. Clients

### `app/clients/gpu_openai.py`
`create_gpu_openai_client(settings)`: `openai.AsyncOpenAI(base_url=settings.api_base_url, api_key=settings.api_key.get_secret_value())` — raises plain `ValueError` if `api_base_url`/`api_key` missing. `GpuOpenAIClient.__init__` raises `ValueError("MODEL_NAME is required")` if `settings.model_name is None`.
- `moderate_images(*, prompt, image_paths)`: builds one `chat.completions.create` call with `messages=[{"role":"user","content":[{"type":"text","text":prompt}, *[{"type":"image_url","image_url":{"url": data_url}} for each path]]}]`, `temperature=0`. Images are inlined as base64 data URLs (`data:{mime};base64,{...}`), mime guessed via `mimetypes.guess_type` defaulting to `image/jpeg`. **No explicit HTTP timeout or retry configured on the OpenAI SDK client itself** — reliability comes entirely from `GpuModerationService`'s own retry loop (§6), not from the client or SDK config.
- `moderate_text(*, prompt, text)`: `messages=[{"role":"system","content":prompt},{"role":"user","content":text}]`, `temperature=0`.
- Response content extraction handles `str`, `list` (joins `str(part)` for each), and otherwise `str(message_content)`.

### `app/clients/kvrocks.py`
`create_kvrocks_client(settings)`: picks `RedisCluster` if `kvrocks_cluster_enabled` (default **True**) else plain `Redis`. Params: `host, port, password, ssl=kvrocks_tls_enabled, ssl_ca_certs/ssl_ca_data, ssl_certfile, ssl_keyfile, decode_responses=True, max_connections=kvrocks_max_connections (500), socket_timeout (5.0s), socket_connect_timeout (5.0s), health_check_interval (30s)`.
- PEM handling: if the env value for CA cert contains `"-----BEGIN "` it's treated as inline PEM text (`ssl_ca_data`); for client cert/key, if inline PEM, the value is written to a file under `/tmp/nsfw-kvrocks-certs/<sha256(value)>.pem` with mode `0600` (dir mode `0700`) — this is because `redis-py` requires file paths for `ssl_certfile`/`ssl_keyfile` but accepts raw data for `ssl_ca_data`. Otherwise (no `"-----BEGIN "` substring) the value is assumed to already be a filesystem path.

### `app/clients/postgres.py`
`create_postgres_engine(settings)`: `sqlalchemy.ext.asyncio.create_async_engine(url, pool_pre_ping=True)` — no explicit pool size/timeout/overflow configured (SQLAlchemy defaults apply: pool_size=5, max_overflow=10 by default for the underlying dialect). Raises `ValueError` if `postgres_database_url` unset. Uses `asyncpg` driver per the connection string scheme seen in tests (`postgresql+asyncpg://...`), though `psycopg[binary]` is also a listed dependency (likely for Alembic sync migrations).

### `app/clients/clickhouse.py`
`create_clickhouse_client(settings)`: uses `clickhouse_connect.get_client(...)`, parses `clickhouse_primary_database_url` via `urlparse` for host/port/database (falls back to `settings.clickhouse_database` if URL has no path), `username`/`password` prefer explicit `clickhouse_user`/`clickhouse_password` settings over URL userinfo, `secure = clickhouse_secure or (parsed.scheme == "https")`, `verify = clickhouse_verify`. **Synchronous client** (no async ClickHouse driver used) — all call sites either run it in a thread pool (`manual_ban_service` via `anyio.to_thread.run_sync`) or call it directly and block the event loop (`ClickHouseFlushService`, worker context — acceptable since the flush worker is a separate single-purpose process, not the API server).

### `app/clients/storj_interface.py`
`StorjInterfaceClient.move_to_nsfw(publisher_user_id, video_id)`: raises `AppError("storj_not_configured", ..., 503)` if URL/token missing. `POST {storj_interface_url.rstrip('/')}/move-to-nsfw` with JSON body `{"publisher_user_id", "video_id"}`, header `Authorization: Bearer <token>`, `timeout=settings.storj_interface_timeout_seconds` (default **10.0s**). `response.raise_for_status()` — any non-2xx raises `httpx.HTTPStatusError` uncaught at this layer (propagates to caller; the video pipeline treats 5xx as retryable per `_is_retryable`, 4xx as non-retryable). No retry inside this client itself.

### `app/clients/http.py`
`create_http_client()`: `httpx.AsyncClient(follow_redirects=True)` — no explicit timeout set here (relies on httpx default of 5s connect/read/write/pool... actually httpx default total timeout is 5.0s unless overridden per-call, and call sites like `image_detection_service._download_image_with_retries` and `frame_extraction_service.download_video` pass their own explicit `timeout=` kwargs per-request, so the client-level default rarely matters in practice — but this is worth flagging since the shared `http_client` created here is reused across the video worker's lifetime for both video downloads and Storj calls).

---

## 8. Workers

### `app/workers/video_worker.py`
`VideoQueueWorker`:
```python
async def run_forever(self):
    await self._queue_service.ensure_consumer_group()
    while True:
        await self.run_once()

async def run_once(self) -> int:
    messages = await self._queue_service.read_video_job_messages(consumer_name=self._consumer_name, count=settings.queue_read_count, block_ms=settings.queue_block_ms)
    for message in messages:
        await self._handle_message(message)
    return len(messages)
```
- No explicit polling sleep — concurrency/pacing comes from Redis Streams `XREADGROUP ... BLOCK <queue_block_ms>` (default **5000ms**), `COUNT queue_read_count` (default **1**, i.e., processes **one job at a time, sequentially**, not concurrently — no `asyncio.gather`/worker pool inside a single process). Horizontal scaling is achieved by running multiple worker **processes** (each with a distinct consumer name), not by internal concurrency.
- `_handle_message`: if `message.job_id` empty → `move_video_job_message_to_dlq(error_code="queue_message_missing_job_id")`. If job lookup (`get_status_by_job_id`) returns `None` → DLQ with `error_code="queue_job_not_found"`. Otherwise `processor.process(job)`; on `VideoJobProcessingError`: if `retryable` → `requeue_video_job` (re-`XADD`s a fresh message) + `ack_video_job_message` (acks/removes the *original* delivery — i.e., retry is implemented as "ack old, publish new", not Redis Streams' native PEL-retry/claim mechanism); if not retryable → `move_video_job_message_to_dlq`. On success → `ack_video_job_message`.
- `main()`/`run()`: builds all dependencies manually (no DI container), requires `is_kvrocks_configured()` and `is_postgres_configured()` else raises `RuntimeError` at startup; requires `build_gpu_moderation_service(settings)` to succeed (raises `RuntimeError("GPU moderation settings are required for the video worker")` if `None`). `_consumer_name(settings)`: uses `settings.queue_consumer_name` if set, else `f"{socket.gethostname()}-{os.getpid()}"`. `finally` block closes `http_client`, disposes `postgres_engine`, closes `redis_client`.
- **No graceful shutdown/signal handling** — `run_forever` has no SIGTERM handler; a `docker stop` would hard-kill the process mid-message-processing (no drain logic). Worth flagging as a requirement decision point for the Rust port (Rust async runtimes typically want explicit shutdown signal wiring, e.g. via `tokio::signal`).

### `app/workers/clickhouse_flush_worker.py`
```python
async def run():
    settings = Settings()
    redis_client = create_kvrocks_client(settings)
    clickhouse_client = create_clickhouse_client(settings)
    flush_service = ClickHouseFlushService(...)
    await flush_service.flush_once()

def main(): asyncio.run(run())
```
**Runs exactly once per process invocation and exits** — not a loop, no polling interval, no signal handling, no client cleanup (`redis_client`/`clickhouse_client` are never explicitly closed — relies on process exit). This confirms §6's flagged gap: this must be invoked repeatedly by an external scheduler (cron, k8s CronJob, etc.) which is **not defined anywhere in this repo**.

---

## 9. Error Handling

### `app/errors/base.py`
```python
class AppError(Exception):
    def __init__(self, code: str, message: str, status_code: int = 400):
        self.code, self.message, self.status_code = code, message, status_code
```
Simple typed exception carrying its own HTTP status — there is **no central code→status registry**; each raise site decides its own status inline. Below is the complete map reconstructed from every raise site found in the codebase:

| Code (from `app/errors/codes.py` unless noted) | HTTP status | Raised by |
|---|---|---|
| `auth_missing_headers` | 401 | `AuthService.authenticate`; also synthesized by `request_validation_error_handler` |
| `auth_bad_timestamp` | 401 | `AuthService.authenticate` |
| `auth_timestamp_out_of_range` | 401 | `AuthService.authenticate` |
| `auth_bad_signature` | 401 | `AuthService.authenticate` (both "secret not configured" and "signature mismatch" cases) |
| `not_found` | 404 | `routes_videos.video_status` (job/result not found) |
| `not_implemented` | *(declared, unused anywhere)* | — |
| `service_unavailable` | 503 | `deps.get_manual_ban_service` (ban service not configured); also the generic `unhandled_error_handler` catch-all (500, reusing this code string oddly — see below) |
| `queue_error` | *(declared, unused anywhere)* | — |
| `queue_unavailable` | 503 | `routes_videos._queue_unavailable_error()` (Redis connection/timeout/max-connections errors on detect & status routes) |
| `validation_error` | 422 | `validation_error_handler` (pydantic `ValidationError`) and `request_validation_error_handler` fallback (FastAPI `RequestValidationError` not related to the two HMAC headers) |
| `model_moderation_failed` | 503 | `GpuModerationService._raise_model_failure` (GPU retries exhausted, non-`AppError` cause) |
| `model_response_invalid_json` | 502 | `schemas/model_output.py::_load_model_json` |
| `model_response_invalid_schema` | 502 | `parse_visual_batch_response`, `parse_text_moderation_response` (multiple validation branches) |
| `image_download_failed` | 400 *(default — no explicit status_code passed)* | `ImageDetectionService._download_image_with_retries` |
| `image_download_timeout` | 504 | same, on `httpx.TimeoutException` after retries exhausted |
| `image_download_upstream_error` | 502 | same, on `httpx.HTTPStatusError` with `status>=500` after retries exhausted |
| `video_download_empty` | 400 *(default)* | `EmptyVideoDownloadError` |
| `video_too_large` | 400 *(default)* | `VideoTooLargeError` |
| `video_no_stream` | 400 *(default)* | `NoVideoStreamError` |
| `video_probe_failed` | 400 *(default)* | `VideoProbeError` |
| `video_extraction_failed` | 400 *(default)* | `VideoExtractionError` |

**Additional ad-hoc string codes not declared in `codes.py`** (used directly as raw string literals — a minor inconsistency the Rust port should normalize into one enum, but must preserve wire-format values exactly):
| Code (literal string) | HTTP status | Raised by |
|---|---|---|
| `"gpu_not_configured"` | 503 | `ImageDetectionService.detect_url`/`_detect_image_bytes`, `TextDetectionService.detect` |
| `"invalid_image_base64"` | 400 *(default)* | `ImageDetectionService.detect_base64` |
| `"empty_image"` | 400 *(default)* | `ImageDetectionService._detect_image_bytes` |
| `"image_too_large"` | 400 *(default)* | `ImageDetectionService._detect_image_bytes` |
| `"storj_not_configured"` | 503 | `StorjInterfaceClient.move_to_nsfw` |

### `app/middleware/error_handler.py` (registered in `app/main.py::create_app`)
- `app_error_handler(AppError)` → `JSONResponse(status_code=exc.status_code, content={"error":{"code":exc.code,"message":exc.message}})`; additionally, if `exc.status_code >= 500`, reports to Sentry with tags `component=api, error_code, http_status`.
- `http_error_handler(StarletteHTTPException)` → `{"error":{"code": str(exc.status_code), "message": exc.detail}}` (code is literally the **status code as a string**, e.g. `"404"`, not a semantic code — this only fires for framework-level HTTP exceptions like an unmatched route → 404 via Starlette, not app routes).
- `validation_error_handler(pydantic.ValidationError)` → 422, `{"error":{"code":"validation_error","message":str(exc)}}`.
- `request_validation_error_handler(fastapi.RequestValidationError)`: inspects `exc.errors()` for any error with `loc[0]=="header"` and `type=="missing"` where `loc[1].lower()` is `x-internal-timestamp` or `x-internal-signature` → returns `401 auth_missing_headers`; otherwise falls through to generic `422 validation_error`.
- `unhandled_error_handler(Exception)` → always `500`, `{"error":{"code":"service_unavailable","message":"internal server error"}}` (note: reuses the `service_unavailable` code for a **generic unhandled exception**, which is semantically inconsistent — a Rust port might want a distinct `internal_error` code, but must match this **exact wire response** for compatibility with existing callers unless the API contract is explicitly allowed to change). Always reports to Sentry (`component=api, operation=unhandled_exception`), regardless of exception type.
- All 5 handlers are registered in `app/main.py::create_app`: `AppError`, `StarletteHTTPException`, `ValidationError`, `RequestValidationError`, `Exception` (catch-all, registered last).

### `app/errors/video.py` — typed `AppError` subclasses
`EmptyVideoDownloadError()`, `VideoTooLargeError(bytes_written)` (message includes the byte count), `NoVideoStreamError()`, `VideoProbeError(message)`, `VideoExtractionError(message)` — all default to `status_code=400` (no override passed).

### `app/middleware/request_id.py`
`RequestIdMiddleware(BaseHTTPMiddleware)`: reads `x-request-id` header if present else generates `uuid4()`; sets `request.state.request_id`; echoes it back as the `x-request-id` response header on every request (including errors, since it's outer middleware). **Not used for log correlation anywhere else in the code** (no logger adapter injects it into log records) — it's purely a response header today.

---

## 10. Models (`app/models/*.py`)

### `app/models/enums.py`
Just re-exports `VideoJobStatus` from `app.core.constants`.

### `app/core/constants.py` (the real source of enums)
```python
class VideoJobStatus(StrEnum):
    QUEUED = "queued"
    PROCESSING = "processing"
    CLASSIFIED = "classified"
    FAILED_RETRYABLE = "failed_retryable"
    FAILED_TERMINAL = "failed_terminal"
    SUPERSEDED = "superseded"   # declared but never assigned anywhere in current code — no code path sets SUPERSEDED

TERMINAL_VIDEO_STATUSES = {CLASSIFIED, FAILED_TERMINAL, SUPERSEDED}

MODERATION_CATEGORIES = ("safe", "suggestive", "nudity", "porn", "gore", "violence", "self_harm", "hate_or_extremism", "drugs", "unknown", "sexual_minor_content")  # 11 total

UNSAFE_CATEGORIES = {all of the above except "safe"}   # declared, and matches plan's "unsafe categories" list, but note: this set is NOT used by compute_is_nsfw (see §5) — it's imported by model_output.py only as UNSAFE_MODEL_CATEGORIES (a tuple, via list comprehension) to validate top_category consistency, not to compute is_nsfw. compute_is_nsfw uses CATEGORY_BLOCK_THRESHOLDS instead.

RISK_ORDER = (sexual_minor_content, porn, nudity, gore, violence, self_harm, hate_or_extremism, drugs, suggestive, unknown, safe)
```
`SUPERSEDED` being unused today means the Rust port doesn't need to implement any state-transition logic for it yet, but should keep the variant reserved (it appears in the public `VideoJobStatus` enum returned in API responses per the schema examples in `video.py`).

### `app/models/frame_result.py`
```python
@dataclass(frozen=True)
class FrameModerationResult:
    frame_index: int
    frame_timestamp_seconds: float
    top_category: str
    is_nsfw: bool
    overall_severity: int
    categories: dict[str, int]
    reason: str
    raw_response: dict[str, object]   # the full model_dump(mode="json") of the parsed FrameModerationOutput, including its own frame_index/overall_severity/is_nsfw computed fields
```

### `app/models/storage_action.py`
```python
@dataclass(frozen=True)
class StorageAction:
    action_id: str; job_id: str; video_id: str; publisher_user_id: str
    action_type: str; threshold: float; final_score: float
    request_url: str; request_body: dict[str, object]
    response_status: int | None; response_body: str | None
    status: str; created_at: datetime; completed_at: datetime | None
```

### `app/models/video_job.py`
```python
@dataclass(frozen=True)
class VideoJob:
    job_id: str; video_id: str; source_object_version: str; policy_version: str
    status: VideoJobStatus; publisher_user_id: str; post_id: str | None; canister_id: str | None
    source_video_uri: str; upload_event_id: str | None; trace_id: str | None
    attempts: int = 0; last_error_code: str | None = None; last_error_message: str | None = None
    created_at: datetime | None = None; updated_at: datetime | None = None
    started_at: datetime | None = None; finished_at: datetime | None = None
```

### `app/models/video_metadata.py`
```python
@dataclass(frozen=True)
class VideoMetadata:
    job_id: str; video_id: str; duration_seconds: float
    width: int | None; height: int | None; fps: float | None
    codec_name: str | None; has_video_stream: bool; frames_extracted: int
```
**Note**: `width`, `height`, `fps`, `codec_name`, `has_video_stream` are computed by `parse_ffprobe_metadata` but are **never persisted anywhere** (no Postgres table, not written to ClickHouse row beyond `duration_seconds`/`frames_extracted`) — they exist purely transiently in-process. The Rust port should decide whether to actually persist this metadata or intentionally continue discarding it.

### `app/models/video_result.py`
```python
@dataclass(frozen=True)
class VideoModerationResult:
    job_id: str; video_id: str; policy_version: str; prompt_version: str; aggregation_version: str
    final_is_nsfw: bool; final_score: float; final_top_category: str; max_overall_severity: int
    nsfw_frame_count: int; total_frame_count: int
    move_required: bool; move_threshold: float
    max_category_severities: dict[str, int]
    legacy_nsfw_ec: str; legacy_nsfw_gore: str
    final_response: dict[str, object]
    created_at: datetime; updated_at: datetime
```

---

## 11. Repositories — Public Interfaces (what Rust traits must replicate)

### `app/repositories/base.py` / `postgres/base.py` / `clickhouse/base.py`
- `BaseRepository`: just holds a `logging.Logger`.
- `PostgresRepository(session)`: `async def execute(statement)`.
- `ClickHouseRepository(client, database)`: `table(table_name) -> "{database}.{table_name}"`; `insert_model_rows(table_name, rows: list[PydanticModel])` (no-ops on empty, renames `updated_at_replacing`→`_updated_at`, calls sync `client.insert`).
- `UnitOfWork(session_factory)` (`app/repositories/unit_of_work.py`): `async with transaction() as session` — a generic reusable transactional context manager; **note**: this generic `UnitOfWork` class is actually **unused** by the real finalize-classification flow, which instead uses the more specific `PostgresFinalResultUnitOfWork` (below) — dead/legacy scaffolding worth pruning in the port, not a functional requirement.

### KVRocks (`app/repositories/kvrocks/*.py`)

**`VideoQueueRepository` (Protocol)** — implementations: `InMemoryVideoQueueRepository` (test double), `RedisVideoQueueRepository` (real):
```python
async def enqueue_video_job(request: VideoDetectRequest) -> EnqueueResult
async def get_job_by_video_id(video_id: str) -> VideoJob | None
async def get_job_by_id(job_id: str) -> VideoJob | None
async def update_status(job_id, status, *, last_error_code=None, last_error_message=None) -> VideoJob | None
async def ensure_consumer_group() -> None
async def read_video_job_messages(*, consumer_name, count, block_ms) -> list[QueuedVideoJobMessage]
async def ack_video_job_message(message_id) -> None
async def requeue_video_job(job_id) -> None
async def move_video_job_message_to_dlq(message, *, error_code, error_message) -> None
async def aclose() -> None
```
Redis key scheme: `nsfw:video_job:<job_id>` (hash, full job fields), `nsfw:video_job_unique:<video_id>:<source_object_version>:<policy_version>` (string→job_id, for idempotency), `nsfw:video_job_by_video_id:<video_id>` (string→job_id, **set with `NX`** so only the first job for a video_id claims this lookup key — later jobs for the same video_id under a different unique key won't overwrite it, meaning `GET /status?video_id=` may return a **stale/earlier** job if a video is resubmitted with a new `source_object_version`/`policy_version` after the first one already set this key — a subtlety the Rust port needs to explicitly decide whether to preserve). Enqueue uses a Redis pipeline transaction (`hset`+`set`+`set`+`xadd`) for non-cluster mode; for `RedisCluster` these are issued as 4 separate non-atomic calls (cluster mode has no multi-key transactions across different hash slots, so **atomicity is only guaranteed in single-node Redis, not in cluster mode** — this is a real consistency gap worth flagging explicitly).
`read_video_job_messages` uses raw `XREADGROUP` via manual connection acquisition for `RedisCluster` (since `redis-py`'s high-level `RedisCluster.xreadgroup` doesn't support blocking reads the same way) — this is intricate custom protocol-level code (`_xreadgroup`) that a Rust client library may or may not need to replicate depending on the chosen Redis crate's cluster support.
Job (de)serialization to/from Redis hash: `_job_to_mapping`/`_job_from_mapping` — all values coerced to strings (`datetime.isoformat()`, `None`→`""`, enums via `.value`), empty string maps back to `None` for optional fields.

**`ClickHouseBufferRepository` (Protocol)** — `InMemoryClickHouseBufferRepository`, `RedisClickHouseBufferRepository`:
```python
async def push_json(key, payload: dict) -> None      # RPUSH key json.dumps(payload, separators=(",",":"))
async def read_batch(key, limit) -> list[dict]         # LRANGE key 0 limit-1, json.loads each
async def trim_batch(key, count) -> None                # LTRIM key count -1
```

**`RuntimeNsfwRepository` (Protocol)** — `InMemoryRuntimeNsfwRepository`, `RedisRuntimeNsfwRepository`:
```python
async def write_result(video_id, payload: dict) -> None   # SET {runtime_nsfw_key_prefix}{video_id} json.dumps(payload)
```
No TTL/expiry set on this key.

### PostgreSQL (`app/repositories/postgres/*.py`)

**`VideoJobRepository(session)`**:
```python
async def insert_video_job(job: VideoJob) -> None
async def get_by_job_id(job_id) -> VideoJob | None
async def get_latest_by_video_id(video_id) -> VideoJob | None   # ORDER BY updated_at DESC LIMIT 1
async def mark_processing(job_id) -> None    # status=PROCESSING, attempts=attempts+1, started_at=now(), updated_at=now()
async def mark_classified(job_id) -> None    # status=CLASSIFIED, finished_at=now(), updated_at=now()
async def mark_failed(job_id, *, status, error_code, error_message) -> None   # raises ValueError if status not in {FAILED_RETRYABLE, FAILED_TERMINAL}; finished_at set only for FAILED_TERMINAL
```

**`VideoJobStateRepository` (Protocol)** used by the processor — implemented by `PostgresVideoJobStateRepository(session_factory)`:
```python
async def get_by_job_id(job_id) -> VideoJob | None
async def mark_processing(job: VideoJob) -> None   # upserts a pre-processing snapshot row (on_conflict_do_nothing) THEN calls VideoJobRepository.mark_processing
async def mark_failed(job_id, *, status, error_code, error_message) -> None
```
Note: `mark_processing`'s upsert-then-update is needed because the initial `insert_video_job` (persisting the row created at enqueue time) is **never actually called anywhere in the traced code path** — enqueue only writes to KVRocks, not Postgres. The **first** Postgres row for a job is created lazily inside `mark_processing`'s `on_conflict_do_nothing()` insert, using a reconstructed snapshot (`attempts = max(job.attempts-1, 0)`, `status=QUEUED`, `started_at=None`) derived from the in-memory `VideoJob` passed by the processor. This is an important asymmetry: **Postgres `nsfw_video_jobs` rows only start existing once a worker first attempts the job**, not at enqueue time — a job that's queued but never picked up by a worker has no Postgres row at all, only a KVRocks entry.

**`PostgresFinalResultUnitOfWork(session_factory)`** implements the `FinalResultUnitOfWork` protocol from `video_detection_service.py`:
```python
async def __aenter__(self)/__aexit__(...)   # opens session + begins transaction; on exit, exits transaction ctx then session ctx (transaction errors properly propagate/rollback via SQLAlchemy's begin() context manager semantics)
async def insert_frame_results(*, job, result, frames, settings) -> None
async def insert_final_result(result) -> None
async def insert_storage_action(action) -> None
async def mark_job_classified(job_id) -> None
```

**`VideoResultRepository(session)`**:
```python
async def insert_final_result(result: VideoModerationResult) -> None
async def get_by_job_id(job_id) -> VideoModerationResult | None
async def get_latest_by_video_id(video_id) -> VideoModerationResult | None   # ORDER BY updated_at DESC LIMIT 1
```
`row_to_video_result` reconstructs `max_category_severities` **from the JSONB `final_response` column's `max_category_severities` key**, not from a dedicated column — i.e., that field is not independently persisted/queryable in SQL, only nested inside the JSON blob.

**`FrameResultRepository(session)`**:
```python
async def insert_frame_results(*, job_id, video_id, prompt_version, model_provider, model_name, model_version, frames: list[FrameModerationResult]) -> None
```
Bulk single `INSERT ... VALUES (...), (...)` via SQLAlchemy Core; no-ops if `frames` empty. `frame_id = f"{job_id}:{frame.frame_index}"` (primary key), `frame_hash` always `None` (declared column, never populated — no perceptual hashing implemented anywhere in this codebase despite the column existing).

**`StorageActionRepository(session)`**: `async def insert_storage_action(action: StorageAction) -> None`.

**`VideoMetadataRepository(session)`**: empty stub class (`pass`) — confirms §10's note that video metadata is never persisted to Postgres despite a repository file existing as a placeholder.

### ClickHouse (`app/repositories/clickhouse/*.py`)
All four are one-liner wrappers around `ClickHouseRepository.insert_model_rows`:
- `ClickHouseVideoResultRepository.insert_rows(table_name, rows: list[VideoNsfwDetectionRow])`
- `ClickHouseLegacyNsfwAggRepository.insert_rows(table_name, rows: list[LegacyNsfwAggRow])`
- `ClickHouseStorageActionRepository.insert_rows(table_name, rows: list[StorageActionRow])`
- `ClickHouseExcludedVideosRepository.insert_rows(table_name, rows: list[ExcludedVideoRow])`

None of these have read methods — ClickHouse is write-only from this service's perspective (all reads for `/status` and idempotency come from Postgres/KVRocks only).

---

## 12. Deployment

### `Dockerfile` (repo root, used by `make`/local/fly-style builds — **not** the one CI actually builds)
`python:3.12-slim` base, installs `ffmpeg` via apt, `pip install -r requirements.txt`, copies whole repo, `EXPOSE 8080`, `CMD uvicorn app.main:app --host 0.0.0.0 --port 8080`. No `alembic`/`db`/`scripts` explicitly copied separately (whole `. /app` copy), no healthcheck defined here.

### `deploy-baremetal/Dockerfile` (the one CI actually builds — see workflow below)
Same base image, additionally installs `ca-certificates`, `curl`; copies `alembic.ini`, `alembic/`, `app/`, `db/`, `scripts/` explicitly (not `tests/`); adds a `HEALTHCHECK` (`curl -sf http://127.0.0.1:8080/health`, interval 30s, timeout 5s, start-period 30s, retries 3); same `CMD`.

### `fly.toml`
App `prod-yral-nsfw-classification`, region `ams`, `PORT=50051` env var (note: **mismatches** the Dockerfile's `EXPOSE 8080`/hardcoded uvicorn port — fly.toml's `internal_port = 50051` targets a port the container doesn't actually listen on per the current `CMD`; this looks like a **stale leftover from the old gRPC service** (which likely listened on 50051) and fly.io deployment is probably not actually functional/current for this FastAPI app). `kill_signal=SIGINT`, `kill_timeout=5s`, `swap_size_mb=32768`, concurrency `hard_limit=2500/soft_limit=250`, VM `8gb` memory, `performance` CPU kind, 1 CPU. **This strongly suggests fly.io is a legacy/unused deployment target and bare-metal+HAProxy (below) is the actual current production path** — worth confirming with the team before porting fly.toml as-is.

### `Makefile` targets
`install` (`uv sync`), `run` (`uv run uvicorn app.main:app --host 0.0.0.0 --port ${PORT:-8080} --reload`), `worker` (`python -m app.workers.video_worker`), `flush-worker` (`python -m app.workers.clickhouse_flush_worker`), `lint`/`format` (`ruff`), `check` (`lint`+`test`), `test`/`test-unit`/`test-integration` (`pytest`), `migrate`/`db-upgrade`/`db-downgrade` (`alembic upgrade head` / `downgrade -1`), `ch-ddl` (`scripts/create_clickhouse_tables.py` — applies every `db/clickhouse/*.sql` file in sorted order via `client.command(...)`), `smoke-video`/`smoke-image`/`smoke-text` (ad-hoc scripts in `scripts/`, not part of CI).

### `deploy-baremetal/docker-compose.yml`
Defines **only one service, `app`** — no `worker` or `flush-worker` service. Image `ghcr.io/ansuman-yral/ansuman-nsfw-detetction-server:${IMAGE_TAG:-latest}` (note the `detetction` typo baked into the image repo name — must be preserved verbatim if referencing the actual GHCR path, though the workflow file actually computes the image name dynamically from `github.repository` so this static default in compose is likely stale/unused). `restart: unless-stopped`, journald logging (`tag: nsfw-app`), full env-var passthrough for every setting in §1, host port binding `127.0.0.1:${HOST_APP_PORT:-8001}:8080` (loopback-only — HAProxy fronts it), Docker healthcheck matching the Dockerfile's, `extra_hosts: host.docker.internal:host-gateway`, custom bridge network `nsfw-net`. **Confirms**: video worker and flush worker processes have no defined deployment target in this repo at all — a genuine gap for the Rust port to resolve (either add compose services or design a scheduler).

### `deploy-baremetal/haproxy-nsfw-snippets.cfg` + `host2backend.map.append` + `install-haproxy-nsfw.sh` + `README.md`
Two-tier HAProxy: (1) a local bridge frontend `fe_nsfw_bridge` on `127.0.0.1:18082` + Tailscale IP that forwards to the local app on `127.0.0.1:8001`, health-checked via `GET /health`; (2) a public-facing backend `be_nsfw_app` with `roundrobin` balancing across `ansuman1 (100.78.17.101:18082)` and `ansuman2 (100.79.99.107:18082)` (Tailscale IPs), sets `X-Real-IP`/`X-Forwarded-For` from Cloudflare's `CF-Connecting-IP` header when present, adds standard security response headers (HSTS, X-Frame-Options DENY, X-Content-Type-Options nosniff, Referrer-Policy). Public host: `nsfw.ansuman.yral.com` (routed via a host→backend map file). GitHub Actions SSHes to **public IPs** (`88.99.192.144`, `88.99.61.221`) for deployment, while HAProxy backend config itself routes over **Tailscale IPs** — two distinct IP addressing schemes for the same two physical servers.

### `.github/workflows/build-check.yml`
Reusable workflow: `uv sync --dev`, `make lint`, `pytest tests/unit tests/integration -q`, `python -m compileall app scripts alembic` (Python 3.12 by default, parameterized).

### `.github/workflows/deploy-baremetal.yml`
On push to `main` (or manual dispatch): runs `build-check.yml` first → builds/pushes `deploy-baremetal/Dockerfile` to GHCR tagged `latest` + full commit SHA (via `docker/metadata-action`) → parses `BAREMETAL_SERVER_IPS` (comma-separated repo var) into a matrix → for each server (max-parallel: 1, i.e. **rolling one-at-a-time deploy**, `fail-fast: true`): SSH connectivity check, `rsync -avz --delete` the `deploy-baremetal/` directory to `/home/ansuman/nsfw` on the server, `docker login` to GHCR on the remote host, then `docker compose pull app && docker compose up -d --no-deps app` with a large inline env-var block sourced from GitHub secrets/vars (all settings from §1, prefixed `PROD_*` for secrets), `sleep 20`, `docker compose ps`, then a verify step polling `curl -sf http://127.0.0.1:8001/health` up to 10× every 3s (dumps `docker compose logs --tail=50 app` and fails the job if health never passes).

### `.github/workflows/rollback-production.yml`
Manual-dispatch only, takes an `image_tag` input; same rolling-per-server flow as deploy but pulls/deploys the specified tag directly.

### `pyproject.toml` — exact dependency versions (managed via `uv`, `hatchling` build backend)
```
python >=3.11 (Dockerfiles use 3.12; CI matrix defaults to 3.12)
alembic>=1.13.3
asyncpg>=0.29.0
clickhouse-connect>=0.8.0
fastapi>=0.115.0
httpx>=0.27.2
openai>=1.50.0
pydantic>=2.9.2
pydantic-settings>=2.5.2
psycopg[binary]>=3.2.3
redis>=5.1.0
sentry-sdk>=2.14.0
sqlalchemy[asyncio]>=2.0.35
tenacity>=9.0.0            # NOTE: declared as a dependency but never imported/used anywhere in app/ — all retry loops in this codebase are hand-rolled (see §6), not tenacity-based; dead dependency
uvicorn[standard]>=0.30.6
```
`[project.optional-dependencies].legacy`: a long list of pinned legacy gRPC/ML deps (cachetools, google-cloud-*, grpcio*, opencv-python, pandas, pillow, protobuf, pyjwt, requests, scikit-learn, torch, torchvision, transformers, upstash-vector) — these back `app/legacy/` which is explicitly out of scope per your instructions; not needed for the Rust port.
Dev deps: `pytest>=8.3.3`, `pytest-asyncio>=0.24.0`, `ruff>=0.6.8`. Ruff config: `line-length=120`, `target-version=py311`, `extend-exclude=["app/legacy"]`, lint rules `["E","F","I","UP","B"]`. Pytest: `asyncio_mode="auto"`, `testpaths=["tests"]`.

### Deployment summary
**Bare-metal + Docker Compose + HAProxy is the real, actively-deployed production path** (two Hetzner servers `ansuman-1`/`ansuman-2`, GHCR image, systemd/journald logging, GitHub Actions rolling deploy). `fly.toml` appears stale/legacy (port mismatch strongly suggests it predates the FastAPI rewrite). Only the FastAPI `app` process is deployed anywhere — **the video worker and ClickHouse flush worker have no operational deployment definition in this repo** and must be designed from scratch (or discovered as existing outside this repo) for the Rust port's ops plan.

---

## 13. Tests — Structure Overview

```
tests/conftest.py                                    # test_settings fixture (SecretStr overrides, _env_file=None), signed_headers() helper for building valid HMAC test requests
tests/integration/api/test_health.py
tests/integration/api/test_image_detect.py
tests/integration/api/test_text_detect.py
tests/integration/api/test_video_ban.py
tests/integration/api/test_video_detect.py
tests/integration/workers/test_frame_extraction.py
tests/unit/api/test_legacy_isolation.py               # likely asserts app/legacy/ isn't imported by production code
tests/unit/clients/test_storj_interface.py
tests/unit/core/test_lifecycle.py
tests/unit/core/test_security.py                       # HMAC message shape, signature round-trip, malformed-signature rejection (detailed in §4)
tests/unit/core/test_settings.py                       # secret redaction in repr(), KVRocks pool option env var overrides
tests/unit/repositories/test_clickhouse_repositories.py
tests/unit/repositories/test_kvrocks_queue_repository.py
tests/unit/repositories/test_postgres_serialization.py
tests/unit/repositories/test_postgres_tables.py
tests/unit/services/test_aggregation.py
tests/unit/services/test_auth_service.py
tests/unit/services/test_clickhouse_flush_service.py
tests/unit/services/test_frame_extraction_service.py
tests/unit/services/test_gpu_moderation_service.py
tests/unit/services/test_image_detection_service.py
tests/unit/services/test_legacy_mapping.py              # confirms exact legacy_ec/legacy_gore threshold tables (detailed in §5)
tests/unit/services/test_manual_ban_service.py           # confirms hardcoded "explicit"/"VERY_UNLIKELY"/"banned" values, request-body fields other than trace_id are unused (detailed in §3)
tests/unit/services/test_model_output.py
tests/unit/services/test_moderation_policy.py            # confirms per-category threshold table, esp. nudity/suggestive needing severity 5 not 4 (the major plan.md delta in §5)
tests/unit/services/test_queue_service.py
tests/unit/services/test_text_detection_service.py
tests/unit/services/test_video_detection_service.py
tests/unit/services/test_video_processing_service.py
tests/unit/services/test_video_status_service.py
tests/unit/workers/test_video_worker.py
```
No test files touch `app/legacy/` directly (consistent with the out-of-scope instruction). The most load-bearing edge-case tests for the Rust port's correctness are `test_moderation_policy.py` (§5's threshold table) and `test_manual_ban_service.py` (§3's hardcoded-values/unused-fields behavior) — both already fully captured above with their exact assertions.

---

## Summary of Highest-Priority Items for the Rust Spec

1. **`compute_is_nsfw` uses per-category thresholds** (`CATEGORY_BLOCK_THRESHOLDS` in §5), **not** plan.md's `top_category in unsafe_categories OR overall_severity >= 3` rule. This is the single most consequential behavioral delta — get this table exactly right.
2. **Manual ban endpoint accepts but discards** `publisher_user_id`, `post_id`, `canister_id`, `reason`, `source`, `moderator_id` — only `video_id` (path) and `trace_id` are used; `nsfw_ec`/`nsfw_gore`/`exclusion_reason` are hardcoded literals, not derived (§3).
3. **No deployed video worker / flush worker** exists in this repo's ops config — `docker-compose.yml` only runs the API; the flush worker is a one-shot process requiring an external scheduler that doesn't exist here (§6, §8, §12).
4. **`excluded_videos` ClickHouse table has no DDL anywhere in the repo** — schema must be reverse-engineered from `ExcludedVideoRow` and confirmed externally before the Rust port can define its own DDL/migration (§2, §12).
5. **KVRocks writes are only atomic in single-node mode**; `RedisCluster` enqueue path issues 4 non-atomic calls, so cluster-mode deployments have a real (if narrow) window for partial-enqueue inconsistency (§11).
6. **Retry/backoff constants** (all exponential, capped, no jitter despite plan.md saying "with jitter"): GPU calls `base=0.25s, attempts=3, cap=2.0s`; image download `base=0.5s, attempts=3, cap=2.0s`; KVRocks pool-exhaustion read retry `base=0.05s, attempts=3, cap=1.0s`; queue/job-level retry budget `queue_max_attempts=3`.
7. **fly.toml is very likely stale/unused** (port mismatch vs. actual app) — confirm with the team whether Fly.io needs to be part of the Rust port's deployment target at all, or whether bare-metal+HAProxy is the only real target.
