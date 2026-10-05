pub mod cache;
pub mod config;
pub mod detect;
pub mod engine;
pub mod error;
pub mod glossary;
pub mod ocr;
pub mod task;
pub mod validation;

pub use cache::TranslationCache;
pub use config::AppConfig;
pub use detect::detect_language;
pub use engine::{
    BatchItemRequest, BatchItemResult, DeepLEngine, EngineCapabilities, FallbackEngine,
    GoogleWebEngine, OpenAIEngine, TranslateEngine, TranslationRequest, TranslationResponse,
};
pub use error::{Error, ErrorKind, Result};
pub use task::{classify_error, TaskError, TaskOutcome, TaskState};
pub use glossary::Glossary;
pub use ocr::{OcrEngine, OcrRequest, OcrResponse, SystemOcrEngine};
pub use validation::{validate_engine_id, validate_language_code, validate_translation_input};
