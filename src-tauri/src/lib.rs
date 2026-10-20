pub mod cache_controller;
pub mod clipboard_gate;
pub mod commands;
pub mod engine_pool;
pub mod ocr;
pub mod overlay;
pub mod proxy;
pub mod screencap;
pub mod secrets;
pub mod server;
pub mod storage;

/// 生成 tauri-specta 强类型绑定并写入仓库内的 `src/bindings.ts`。
///
/// 路径基准必须是 `CARGO_MANIFEST_DIR`（即 `src-tauri/`），不能用相对路径：
/// 相对路径会被按**进程 cwd** 解析，从仓库根执行 `cargo test` / `cargo run` 时
/// 会在仓库外凭空生成文件。
///
/// 本函数是导出绑定的**唯一实现**，`main.rs` 与 `bin/export_bindings.rs` 共用，
/// 避免两处各写一遍路径而基准不一致。
pub fn export_bindings() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/bindings.ts");
    app_command_builder!().export(specta_typescript::Typescript::default(), &output)?;
    println!("TypeScript 绑定已生成至 {output:?}");
    Ok(())
}

#[macro_export]
macro_rules! app_command_builder {
    () => {
        ::tauri_specta::Builder::<::tauri::Wry>::new().commands(::tauri_specta::collect_commands![
            $crate::commands::translation::translate,
            $crate::commands::translation::translate_stream,
            $crate::commands::translation::cancel_stream_task,
            $crate::commands::translation::get_engine_capabilities,
            $crate::commands::runtime::set_float_sticky,
            $crate::commands::runtime::take_pending_float_request,
            $crate::commands::runtime::get_server_token,
            $crate::commands::runtime::rotate_server_token,
            $crate::commands::runtime::approve_pairing,
            $crate::commands::runtime::reject_pairing,
            $crate::commands::settings::save_api_key,
            $crate::commands::settings::is_api_key_configured,
            $crate::commands::settings::delete_api_key,
            $crate::commands::settings::save_llm_config,
            $crate::commands::settings::get_llm_config,
            $crate::commands::settings::save_llm_style,
            $crate::commands::settings::get_llm_style,
            $crate::commands::shortcuts::get_shortcut,
            $crate::commands::shortcuts::set_shortcut,
            $crate::commands::shortcuts::get_replace_shortcut,
            $crate::commands::shortcuts::set_replace_shortcut,
            $crate::commands::shortcuts::detect_text_language,
            $crate::commands::history::add_glossary_term,
            $crate::commands::history::import_glossary_terms,
            $crate::commands::history::get_glossary_terms,
            $crate::commands::history::delete_glossary_term,
            $crate::commands::history::clear_cache,
            $crate::commands::history::get_translation_history,
            $crate::commands::history::search_translation_history,
            $crate::commands::history::delete_history_entry,
            $crate::commands::history::export_translation_history,
            $crate::commands::history::set_cache_enabled,
            $crate::commands::history::get_cache_enabled,
            $crate::commands::history::ocr_clipboard_image,
            $crate::commands::history::set_clipboard_monitor,
            $crate::commands::history::get_clipboard_monitor,
            $crate::commands::runtime::get_server_status,
            $crate::commands::shortcuts::get_shortcut_status,
            $crate::commands::settings::get_preferred_replace_engine,
            $crate::commands::settings::save_preferred_replace_engine,
            $crate::commands::settings::get_preferred_target_lang,
            $crate::commands::settings::save_preferred_target_lang,
            $crate::commands::settings::get_proxy_config,
            $crate::commands::settings::save_proxy_config,
            $crate::commands::screenshot::start_screenshot_translation,
            $crate::commands::screenshot::get_screenshot_session,
            $crate::commands::screenshot::recognize_screenshot_selection,
            $crate::commands::screenshot::cancel_screenshot_translation,
            $crate::commands::screenshot::get_screenshot_capability,
            $crate::commands::screenshot::get_screenshot_shortcut,
            $crate::commands::screenshot::set_screenshot_shortcut,
            $crate::commands::screenshot::translate_screenshot_blocks,
            $crate::commands::screenshot::push_screenshot_text_to_main,
            $crate::commands::document::translate_document,
            $crate::commands::document::cancel_document_task,
            $crate::commands::history::get_offline_engine_status,
            $crate::commands::updater::check_for_update,
            $crate::commands::updater::install_update
        ])
    };
}
