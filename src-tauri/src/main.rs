// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use specta_typescript::Typescript;
use src_tauri::{commands, secrets, server};
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};
use tauri_specta::{collect_commands, Builder};
use transly_core::{GoogleWebEngine, TranslateEngine, TranslationCache};

fn main() {
    // 初始化持久化翻译缓存
    let cache = TranslationCache::default_shared();

    // 初始化全局共享翻译引擎（保持 HTTP Client 连接池长连接）
    let engine: Arc<dyn TranslateEngine> = Arc::new(GoogleWebEngine::new());

    // 获取或生成用于本地扩展通信的 Bearer Token
    let token = secrets::get_or_create_server_token()
        .unwrap_or_else(|_| "transly-default-token".to_string());

    // 异步启动本地插件通信服务
    let token_clone = token.clone();
    let server_cache = cache.clone();
    let server_engine = engine.clone();
    tokio::spawn(async move {
        if let Err(e) = server::start_server(39281, token_clone, server_cache, server_engine).await
        {
            eprintln!("本地 HTTP 服务启动失败: {e}");
        }
    });

    let builder = Builder::<tauri::Wry>::new().commands(collect_commands![
        commands::translate,
        commands::detect_lang,
        commands::get_server_token,
        commands::save_api_key,
        commands::get_api_key,
        commands::delete_api_key
    ]);

    #[cfg(debug_assertions)]
    builder
        .export(Typescript::default(), "../src/bindings.ts")
        .expect("Failed to export typescript bindings");

    tauri::Builder::default()
        .manage(cache)
        .manage(engine)
        .invoke_handler(builder.invoke_handler())
        .setup(|app| {
            // 构建系统托盘菜单
            let show_i = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "退出 Transly", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

            let _tray = TrayIconBuilder::new()
                .tooltip("Transly 桌面翻译")
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // 点击关闭按钮时隐藏窗口常驻托盘，不中断后台服务
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
