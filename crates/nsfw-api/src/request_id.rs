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
