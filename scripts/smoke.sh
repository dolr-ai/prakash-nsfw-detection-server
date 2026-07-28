#!/usr/bin/env bash
# Smoke-test the stateless moderation endpoints against a running nsfw-api.
#
# Signs each request with the internal HMAC scheme:
#   message = "<unix_ts>\n<METHOD>\n<path>\n<sha256_hex(raw_body)>"
#   header  X-Internal-Signature = hex(hmac_sha256(secret, message))
#
# Usage:
#   INTERNAL_REQUEST_HMAC_SECRET=... ./scripts/smoke.sh [base_url]
#
# Default base_url: http://127.0.0.1:8080
set -euo pipefail

BASE_URL="${1:-http://127.0.0.1:8080}"
SECRET="${INTERNAL_REQUEST_HMAC_SECRET:?INTERNAL_REQUEST_HMAC_SECRET must be set}"

sign_and_post() {
  local path="$1" body="$2"
  local ts body_sha msg sig
  ts="$(date +%s)"
  body_sha="$(printf '%s' "$body" | openssl dgst -sha256 -hex | awk '{print $NF}')"
  msg="$(printf '%s\n%s\n%s\n%s' "$ts" "POST" "$path" "$body_sha")"
  sig="$(printf '%s' "$msg" | openssl dgst -sha256 -hmac "$SECRET" -hex | awk '{print $NF}')"

  echo "--- POST $path"
  curl -sS -w '\nHTTP %{http_code}  (%{time_total}s)\n' \
    -X POST "$BASE_URL$path" \
    -H 'content-type: application/json' \
    -H "X-Internal-Timestamp: $ts" \
    -H "X-Internal-Signature: $sig" \
    -d "$body"
  echo
}

echo "=== unauthenticated health checks ==="
curl -sS "$BASE_URL/health"; echo
curl -sS "$BASE_URL/ready"; echo; echo

echo "=== text moderation (expect safe) ==="
sign_and_post /v1/text/detect \
  '{"text":"A cinematic dance video on a beach at sunset."}'

echo "=== text moderation (expect flagged) ==="
sign_and_post /v1/text/detect \
  '{"text":"explicit hardcore pornographic sex scene, nude bodies"}'

echo "=== image moderation by URL ==="
# picsum serves a real photo and is reachable from the deploy hosts;
# Wikimedia blocks server-side fetches, so don't use it here.
sign_and_post /v1/images/detect-url \
  '{"image_url":"https://picsum.photos/400"}'

echo "=== image + generation prompt (judged together) ==="
sign_and_post /v1/images/detect-url \
  '{"image_url":"https://picsum.photos/400","prompt":"make her undress"}'

echo "=== negative: bad signature (expect 401 auth_bad_signature) ==="
curl -sS -w '\nHTTP %{http_code}\n' -X POST "$BASE_URL/v1/text/detect" \
  -H 'content-type: application/json' \
  -H "X-Internal-Timestamp: $(date +%s)" \
  -H "X-Internal-Signature: $(printf '0%.0s' {1..64})" \
  -d '{"text":"hi"}'
echo

echo "=== negative: empty text (expect 422 validation_error) ==="
sign_and_post /v1/text/detect '{"text":"   "}'
