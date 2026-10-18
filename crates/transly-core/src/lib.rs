pub mod cache;
pub mod config;
pub mod detector;
pub mod document;
pub mod engine;
pub mod error;
pub mod glossary;
pub mod ocr;
pub mod preserve;
pub mod proxy;
pub mod task;
pub mod validation;

pub use cache::{CacheEntry, TranslationCache};
pub use config::AppConfig;
pub use detector::{detect_language, resolve_smart_target_lang};
pub use document::{translate_document_content, DocumentFormat, DocumentTranslateOutcome};
pub use engine::{
    lookup_batch_with_fallback, lookup_cache_with_fallback, plan_batch_ranges,
    translate_batch_with_cache, BatchItemRequest, BatchItemResult, BatchPipelineDeps,
    BatchPipelineOutcome, DeepLEngine, EngineCapabilities, GoogleWebEngine, OpenAIEngine,
    TranslateEngine, TranslationRequest, TranslationResponse, MAX_BATCH_ITEMS,
    MAX_BATCH_TOTAL_CHARS, MAX_ITEM_CHARS,
};
pub use error::{Error, ErrorKind, Result};
pub use glossary::Glossary;
pub use ocr::{
    bgrx_to_rgba_in_place, build_candidate_languages, clamp_dimensions, detect_wayland_session,
    filter_blocks, find_monitor_for_point, is_capture_size_consistent, is_within_ocr_pixel_budget,
    map_capture_error, merge_lines_into_paragraphs, merge_words_into_lines, parse_tesseract_tsv,
    plan_block_batches, rgba_to_bgra_in_place, tesseract_lang_arg, words_to_blocks, MonitorRect,
    NormRect, OcrBlockDto, OcrErrorDto, OcrErrorKind, OcrTsvError, PixelBox, ScreenshotSessionDto,
    CAPTURE_SIZE_TOLERANCE, MIN_BLOCK_CONFIDENCE,
};
pub use preserve::{restore_code_blocks, stash_code_blocks, PreservedText};
pub use proxy::{normalize_proxy_url, ProxyConfig, ProxyMode};
pub use task::{classify_error, TaskError};
pub use validation::{
    validate_engine_id, validate_language_code, validate_language_pair, validate_translation_input,
};
