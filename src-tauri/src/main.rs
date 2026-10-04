// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use specta_typescript::Typescript;
use src_tauri::{commands, engine_pool::EnginePool, secrets, server};
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WindowEvent,
};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_specta::{collect_commands, Builder};
use tokio::sync::RwLock;
use transly_core::{Glossary, TranslationCache};

fn main() {
    // 初始化持久化翻译缓存
    let cache = TranslationCache::default_shared();

    // 启动时自动做轻量缓存维护（清理超过 90 天条目，并保留最多 100,000 条）
    let _ = cache.cleanup_older_than_days(90);
    let _ = cache.trim_to_max_items(100_000);

    // 初始化全局术语库
    let glossary = Arc::new(RwLock::new(Glossary::new()));

    // 初始化全局共享引擎池（长连接复用）
    let pool = Arc::new(EnginePool::new());

    // 获取或生成用于本地扩展通信的 Bearer Token
    let token = secrets::get_or_create_server_token()
        .unwrap_or_else(|_| "transly-default-token".to_string());

    // 异步启动本地插件通信服务
    let token_clone = token.clone();
    let server_cache = cache.clone();
    let server_pool = pool.clone();
    let server_glossary = glossary.clone();
    tokio::spawn(async move {
        if let Err(e) = server::start_server(
            39281,
            token_clone,
            server_cache,
            server_pool,
            server_glossary,
        )
        .await
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
        commands::delete_api_key,
        commands::add_glossary_term
    ]);

    #[cfg(debug_assertions)]
    builder
        .export(Typescript::default(), "../src/bindings.ts")
        .expect("Failed to export typescript bindings");

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(cache)
        .manage(pool)
        .manage(glossary)
        .invoke_handler(builder.invoke_handler())
        .setup(|app| {
            // 注册全局快捷键 (Alt+Shift+D)，触发读取选中文本/剪贴板并激活主窗口翻译
            let app_handle = app.handle().clone();
            let _ =
                app.global_shortcut()
                    .on_shortcut("Alt+Shift+D", move |_app, _shortcut, event| {
                        if event.state() == ShortcutState::Pressed {
                            let handle = app_handle.clone();
                            // 尝试从剪贴板读取最新选中文本
                            let text = handle.clipboard().read_text().unwrap_or_default();
                            if let Some(window) = handle.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                                if !text.trim().is_empty() {
                                    let _ = window.emit("quick-translate", text);
                                }
                            }
                        }
                    });

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
