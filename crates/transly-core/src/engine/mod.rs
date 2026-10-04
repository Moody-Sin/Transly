use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationRequest {
    pub text: String,
    pub source_lang: String,
    pub target_lang: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationResponse {
    pub text: String,
    pub detected_source_lang: Option<String>,
    pub engine: String,
}

#[async_trait]
pub trait TranslateEngine: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;

    async fn translate(&self, req: &TranslationRequest) -> Result<TranslationResponse>;

    async fn translate_batch(
        &self,
        requests: &[TranslationRequest],
    ) -> Result<Vec<TranslationResponse>> {
        let mut results = Vec::with_capacity(requests.len());
        for req in requests {
            results.push(self.translate(req).await?);
        }
        Ok(results)
    }
}
