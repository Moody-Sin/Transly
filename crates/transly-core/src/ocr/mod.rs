use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrRequest {
    /// 图像二进制数据（支持 PNG, JPEG, BMP）
    pub image_bytes: Vec<u8>,
    /// 期望识别的目标语言或区域（默认 "auto"）
    pub lang: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrTextLine {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrResponse {
    pub text: String,
    pub lines: Vec<OcrTextLine>,
    pub engine: String,
}

#[async_trait]
pub trait OcrEngine: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;

    async fn recognize(&self, req: &OcrRequest) -> Result<OcrResponse>;
}

/// 默认系统轻量 OCR 适配器（占位/原生入口）
pub struct SystemOcrEngine;

impl SystemOcrEngine {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SystemOcrEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl OcrEngine for SystemOcrEngine {
    fn id(&self) -> &'static str {
        "system_ocr"
    }

    fn name(&self) -> &'static str {
        "系统原生 OCR"
    }

    async fn recognize(&self, req: &OcrRequest) -> Result<OcrResponse> {
        if req.image_bytes.is_empty() {
            return Err(crate::Error::Internal("图像数据为空".to_string()));
        }
        // 当前版本未挂载原生 OCR 动态库时返回明确未支持错误，绝不伪装空成功
        Err(crate::Error::UnsupportedLanguage(
            "当前平台原生 OCR 引擎驱动尚未就绪或不可用".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_system_ocr_engine_info() {
        let engine = SystemOcrEngine::new();
        assert_eq!(engine.id(), "system_ocr");
        assert_eq!(engine.name(), "系统原生 OCR");

        let req = OcrRequest {
            image_bytes: vec![1, 2, 3],
            lang: Some("zh-CN".to_string()),
        };
        let res = engine.recognize(&req).await;
        assert!(res.is_err());
    }
}
