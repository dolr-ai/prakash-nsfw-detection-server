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
