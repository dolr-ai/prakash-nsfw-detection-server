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
