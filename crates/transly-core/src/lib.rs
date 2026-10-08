pub mod cache;
pub mod config;
pub mod engine;
pub mod error;
pub mod glossary;
pub mod ocr;
pub mod task;
pub mod validation;

pub use cache::TranslationCache;
pub use config::AppConfig;
pub use engine::{
    lookup_cache_with_fallback, BatchItemRequest, BatchItemResult, DeepLEngine, EngineCapabilities,
    FallbackEngine, GoogleWebEngine, OpenAIEngine, TranslateEngine, TranslationRequest,
    TranslationResponse,
};
pub use error::{Error, ErrorKind, Result};
pub use glossary::Glossary;
pub use ocr::{
    build_candidate_languages, clamp_dimensions, rgba_to_bgra, rgba_to_bgra_in_place, OcrErrorDto,
    OcrErrorKind,
};
pub use task::{classify_error, TaskError};
pub use validation::{validate_engine_id, validate_language_code, validate_translation_input};
