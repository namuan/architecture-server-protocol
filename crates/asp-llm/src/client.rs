use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use std::pin::Pin;
use crate::{CompletionRequest, LlmClient};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct OpenAiCompatClient {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    client: reqwest::Client,
}

impl OpenAiCompatClient {
    pub fn new(base_url: String, model: String, api_key: String) -> Self {
        Self {
            base_url,
            model,
            api_key,
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Serialize)]
struct ApiRequest<'a> {
    model: &'a str,
    messages: &'a [crate::types::Message],
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    stream: bool,
}

#[derive(Deserialize)]
struct ApiResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Option<MessageContent>,
    #[allow(dead_code)]
    delta: Option<MessageContent>,
}

#[derive(Deserialize)]
struct MessageContent {
    content: Option<String>,
}

#[async_trait]
impl LlmClient for OpenAiCompatClient {
    async fn complete(&self, req: CompletionRequest) -> Result<String> {
        let api_req = ApiRequest {
            model: &self.model,
            messages: &req.messages,
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            stream: false,
        };

        let mut builder = self.client
            .post(format!("{}/chat/completions", self.base_url))
            .json(&api_req);

        if !self.api_key.is_empty() {
            builder = builder.bearer_auth(&self.api_key);
        }

        let resp = builder.send().await?;
        let body: ApiResponse = resp.json().await?;

        body.choices
            .into_iter()
            .next()
            .and_then(|c| c.message)
            .and_then(|m| m.content)
            .ok_or_else(|| anyhow!("No content in response"))
    }

    async fn stream(
        &self,
        req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let api_req = ApiRequest {
            model: &self.model,
            messages: &req.messages,
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            stream: true,
        };

        let mut builder = self.client
            .post(format!("{}/chat/completions", self.base_url))
            .json(&api_req);

        if !self.api_key.is_empty() {
            builder = builder.bearer_auth(&self.api_key);
        }

        let resp = builder.send().await?;
        let byte_stream = resp.bytes_stream();

        let stream = byte_stream.flat_map(|chunk| {
            let items: Vec<Result<String>> = match chunk {
                Err(e) => vec![Err(anyhow!(e))],
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    text.lines()
                        .filter(|line| line.starts_with("data: "))
                        .filter(|line| *line != "data: [DONE]")
                        .filter_map(|line| {
                            let json_str = &line["data: ".len()..];
                            let v: Value = serde_json::from_str(json_str).ok()?;
                            let content = v["choices"][0]["delta"]["content"].as_str()?.to_string();
                            Some(Ok(content))
                        })
                        .collect()
                }
            };
            futures::stream::iter(items)
        });

        Ok(Box::pin(stream))
    }
}
