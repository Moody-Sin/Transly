pub mod cache_controller;
pub mod clipboard_gate;
pub mod commands;
pub mod engine_pool;
pub mod ocr;
pub mod secrets;
pub mod server;

#[macro_export]
macro_rules! app_command_builder {
    () => {
        ::tauri_specta::Builder::<::tauri::Wry>::new().commands(::tauri_specta::collect_commands![
            $crate::commands::translate,
            $crate::commands::get_engine_capabilities,
            $crate::commands::set_float_sticky,
            $crate::commands::take_pending_float_request,
            $crate::commands::get_server_token,
            $crate::commands::rotate_server_token,
            $crate::commands::save_api_key,
            $crate::commands::is_api_key_configured,
            $crate::commands::delete_api_key,
            $crate::commands::add_glossary_term,
            $crate::commands::get_glossary_terms,
            $crate::commands::delete_glossary_term,
            $crate::commands::clear_cache,
            $crate::commands::set_cache_enabled,
            $crate::commands::get_cache_enabled,
            $crate::commands::ocr_clipboard_image,
            $crate::commands::set_clipboard_monitor,
            $crate::commands::get_clipboard_monitor
        ])
    };
}
