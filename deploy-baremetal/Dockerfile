# Multi-stage build for the nsfw-api binary.
# The three moderation prompts are embedded at compile time via include_str!,
# so the runtime image needs no extra files -- just the static-ish binary +
# TLS certs (for outbound HTTPS to the GPU/LLM endpoint and image downloads).
#
# NOTE: no ffmpeg here yet -- the current binary is the stateless moderation
# API only. Add ffmpeg/ffprobe to the runtime stage when the video worker lands.

FROM rust:1-slim-bookworm AS builder
WORKDIR /build

# Build deps:
#   pkg-config/libssl-dev -> reqwest's native-tls backend
#   curl                  -> utoipa-swagger-ui's build script downloads the
#                            Swagger UI archive at compile time and shells out to
#                            `curl` (its `reqwest` feature is off). Without curl the
#                            build panics: "failed to download Swagger UI".
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev curl \
    && rm -rf /var/lib/apt/lists/*

# Copy the whole workspace and build just the API binary in release mode.
COPY . .
RUN cargo build --release -p nsfw-api

FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/nsfw-api /usr/local/bin/nsfw-api

EXPOSE 8080

# Liveness: /health needs no dependencies and always returns 200 when up.
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD curl -sf http://127.0.0.1:8080/health || exit 1

CMD ["/usr/local/bin/nsfw-api"]
