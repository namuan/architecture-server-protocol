use async_trait::async_trait;
use anyhow::Result;
use futures::Stream;
use std::pin::Pin;

pub mod client;
pub mod types;

pub use types::{CompletionRequest, Message, Role};
pub use client::OpenAiCompatClient;

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(&self, req: CompletionRequest) -> Result<String>;
    async fn stream(
        &self,
        req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>>;
}
