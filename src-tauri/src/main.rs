// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod secrets;
mod server;

use transly_core::TranslationCache;

fn main() {
    // 初始化持久化翻译缓存
    let cache = TranslationCache::default_shared();

    // 获取或生成用于本地扩展通信的 Bearer Token
    let token = secrets::get_or_create_server_token()
        .unwrap_or_else(|_| "transly-default-token".to_string());

    // 异步启动本地插件通信服务
    let token_clone = token.clone();
    let server_cache = cache.clone();
    tokio::spawn(async move {
        if let Err(e) = server::start_server(39281, token_clone, server_cache).await {
            eprintln!("本地 HTTP 服务启动失败: {e}");
        }
    });

    tauri::Builder::default()
        .manage(cache)
        .invoke_handler(tauri::generate_handler![
            commands::translate,
            commands::detect_lang,
            commands::get_server_token,
            commands::save_api_key,
            commands::get_api_key,
            commands::delete_api_key
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
