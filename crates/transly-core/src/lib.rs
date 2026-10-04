pub mod cache;
pub mod config;
pub mod detect;
pub mod engine;
pub mod error;
pub mod glossary;
pub mod ocr;

pub use cache::TranslationCache;
pub use config::AppConfig;
pub use detect::detect_language;
pub use engine::{
    DeepLEngine, FallbackEngine, GoogleWebEngine, OpenAIEngine, TranslateEngine,
    TranslationRequest, TranslationResponse,
};
pub use error::{Error, Result};
pub use glossary::Glossary;
pub use ocr::{OcrEngine, OcrRequest, OcrResponse, SystemOcrEngine};
