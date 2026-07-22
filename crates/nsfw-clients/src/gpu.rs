use base64::Engine;
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum GpuClientError {
    #[error("gpu request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("gpu response missing message content")]
    MissingContent,
}

#[derive(Clone)]
pub struct ImageInput {
    pub bytes: Vec<u8>,
    pub mime_type: String,
}

impl ImageInput {
    fn to_data_url(&self) -> String {
        let encoded = base64::engine::general_purpose::STANDARD.encode(&self.bytes);
        format!("data:{};base64,{encoded}", self.mime_type)
    }
}

pub struct GpuOpenAiClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    model_name: String,
}

impl GpuOpenAiClient {
    pub fn new(
        http: reqwest::Client,
        base_url: String,
        api_key: String,
        model_name: String,
    ) -> Self {
        Self {
            http,
            base_url,
            api_key,
            model_name,
        }
    }

    pub async fn moderate_images(
        &self,
        prompt: &str,
        images: &[ImageInput],
    ) -> Result<String, GpuClientError> {
        let mut content = vec![json!({"type": "text", "text": prompt})];
        for image in images {
            content.push(json!({"type": "image_url", "image_url": {"url": image.to_data_url()}}));
        }
        let body = json!({
            "model": self.model_name,
            "messages": [{"role": "user", "content": content}],
            "temperature": 0,
        });
        self.call(body).await
    }

    pub async fn moderate_text(&self, prompt: &str, text: &str) -> Result<String, GpuClientError> {
        let body = json!({
            "model": self.model_name,
            "messages": [
                {"role": "system", "content": prompt},
                {"role": "user", "content": text},
            ],
            "temperature": 0,
        });
        self.call(body).await
    }

    async fn call(&self, body: serde_json::Value) -> Result<String, GpuClientError> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let response = self
            .http
            .post(url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?
            .error_for_status()?;
        let parsed: ChatCompletionResponse = response.json().await?;
        let content = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .ok_or(GpuClientError::MissingContent)?;
        Ok(extract_content_text(content))
    }
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    content: Option<serde_json::Value>,
}

/// Matches Python's content extraction: plain string, or a list of parts joined,
/// otherwise the value stringified.
fn extract_content_text(value: serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s,
        serde_json::Value::Array(arr) => arr
            .into_iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(""),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn moderate_images_sends_expected_request_and_parses_string_content() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{"message": {"content": "[{\"frame_index\":0}]"}}]
            })))
            .mount(&server)
            .await;

        let client = GpuOpenAiClient::new(
            reqwest::Client::new(),
            server.uri(),
            "test-key".into(),
            "test-model".into(),
        );
        let image = ImageInput {
            bytes: vec![1, 2, 3],
            mime_type: "image/jpeg".into(),
        };
        let result = client.moderate_images("prompt", &[image]).await.unwrap();
        assert_eq!(result, "[{\"frame_index\":0}]");
    }

    #[tokio::test]
    async fn moderate_text_sends_expected_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{"message": {"content": "{\"top_category\":\"safe\"}"}}]
            })))
            .mount(&server)
            .await;

        let client = GpuOpenAiClient::new(
            reqwest::Client::new(),
            server.uri(),
            "test-key".into(),
            "test-model".into(),
        );
        let result = client.moderate_text("prompt", "hello").await.unwrap();
        assert_eq!(result, "{\"top_category\":\"safe\"}");
    }

    #[tokio::test]
    async fn returns_error_on_non_2xx_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let client = GpuOpenAiClient::new(
            reqwest::Client::new(),
            server.uri(),
            "test-key".into(),
            "test-model".into(),
        );
        assert!(client.moderate_text("prompt", "hello").await.is_err());
    }
}
