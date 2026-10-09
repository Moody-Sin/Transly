pub mod cache_controller;
pub mod clipboard_gate;
pub mod commands;
pub mod engine_pool;
pub mod ocr;
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
            $crate::commands::translate,
            $crate::commands::translate_stream,
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
            $crate::commands::get_translation_history,
            $crate::commands::delete_history_entry,
            $crate::commands::set_cache_enabled,
            $crate::commands::get_cache_enabled,
            $crate::commands::ocr_clipboard_image,
            $crate::commands::set_clipboard_monitor,
            $crate::commands::get_clipboard_monitor,
            $crate::commands::get_server_status
        ])
    };
}
