pub mod cache;
pub mod config;
pub mod detector;
pub mod engine;
pub mod error;
pub mod glossary;
pub mod ocr;
pub mod preserve;
pub mod task;
pub mod validation;

pub use cache::{CacheEntry, TranslationCache};
pub use config::AppConfig;
pub use detector::{detect_language, resolve_smart_target_lang};
pub use engine::{
    lookup_cache_with_fallback, plan_batch_ranges, BatchItemRequest, BatchItemResult, DeepLEngine,
    EngineCapabilities, FallbackEngine, GoogleWebEngine, OpenAIEngine, TranslateEngine,
    TranslationRequest, TranslationResponse, MAX_BATCH_ITEMS, MAX_BATCH_TOTAL_CHARS,
    MAX_ITEM_CHARS,
};
pub use error::{Error, ErrorKind, Result};
pub use glossary::Glossary;
pub use ocr::{
    build_candidate_languages, clamp_dimensions, is_within_ocr_pixel_budget, rgba_to_bgra_in_place,
    OcrErrorDto, OcrErrorKind,
};
pub use preserve::{restore_code_blocks, stash_code_blocks, PreservedText};
pub use task::{classify_error, TaskError};
pub use validation::{
    validate_engine_id, validate_language_code, validate_language_pair, validate_translation_input,
};
