// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod secrets;
mod server;

fn main() {
    // 获取或生成用于本地扩展通信的 Bearer Token
    let token = secrets::get_or_create_server_token()
        .unwrap_or_else(|_| "transly-default-token".to_string());

    // 异步启动本地插件通信服务
    let token_clone = token.clone();
    tokio::spawn(async move {
        if let Err(e) = server::start_server(39281, token_clone).await {
            eprintln!("本地 HTTP 服务启动失败: {e}");
        }
    });

    tauri::Builder::default()
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
