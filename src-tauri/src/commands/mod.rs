use crate::engine_pool::EnginePool;
use std::sync::Arc;
use tokio::sync::RwLock;
use transly_core::task::classify_error;
use transly_core::{
    CacheEntry, Error, ErrorKind, Glossary, ProxyConfig, ProxyMode, TranslationCache,
    TranslationRequest,
};

pub mod translation;

// 翻译命令迁至 `commands::translation`，此处重导出以保持既有引用路径与
// `app_command_builder!` 之外的调用点稳定。
pub use translation::{
    cancel_stream_task, translate, translate_stream, StreamChunkPayload, StreamTaskRegistry,
    TranslateErrorDto, TranslateOutcomeDto, TranslationDependencies,
};

pub mod history;

pub use history::{
    add_glossary_term, clear_cache, delete_glossary_term, delete_history_entry,
    export_translation_history, get_cache_enabled, get_clipboard_monitor, get_glossary_terms,
    get_offline_engine_status, get_translation_history, import_glossary_terms, ocr_clipboard_image,
    search_translation_history, set_cache_enabled, set_clipboard_monitor, ExportHistoryOutcome,
    GlossaryTermDto,
};

pub mod document;
pub mod screenshot;
pub mod selection;
pub mod updater;

pub use document::{translate_document, DocumentTranslateOutcomeDto};

// 划词/浮窗相关符号平移至 `commands::selection`，此处重导出：
// `app_command_builder!` 与 `main.rs` 的既有引用路径保持不变，避免连带改动。
pub use selection::{
    place_float_near_cursor, trigger_selection_replacement, trigger_selection_translation,
    ReplacementFailedPayload, SelectionContext, DEFAULT_REPLACE_SHORTCUT, DEFAULT_SHORTCUT,
};

pub mod runtime;
pub mod settings;
pub mod shortcuts;

pub use runtime::{
    approve_pairing, get_server_status, get_server_token, reject_pairing, rotate_server_token,
    set_float_sticky, take_pending_float_request, FloatRequest, FloatSticky, PendingFloatRequests,
    PendingFloatResponse, ServerStatus, ServerStatusSlot,
};
pub use settings::{
    delete_api_key, get_llm_config, get_preferred_replace_engine, get_preferred_target_lang,
    get_proxy_config, is_api_key_configured, save_api_key, save_llm_config, save_llm_style,
    save_preferred_replace_engine, save_preferred_target_lang, save_proxy_config, LlmConfigDto,
};
pub use shortcuts::{
    detect_text_language, get_replace_shortcut, get_shortcut, get_shortcut_status,
    set_replace_shortcut, set_shortcut, ShortcutStatusDto, ShortcutStatusSlot,
};

#[cfg(test)]
pub(crate) use runtime::PENDING_FLOAT_CAPACITY;
#[cfg(test)]
pub(crate) use settings::{
    get_proxy_config_internal, save_proxy_config_internal, validate_preferred_replace_engine,
    validate_proxy_config,
};

#[cfg(test)]
mod tests;
