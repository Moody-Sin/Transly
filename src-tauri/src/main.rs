// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use enigo::{Direction, Enigo, Key, Keyboard, Settings as EnigoSettings};
#[cfg(debug_assertions)]
use specta_typescript::Typescript;
use src_tauri::{commands, engine_pool::EnginePool, secrets, server};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WindowEvent,
};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tokio::sync::RwLock;
use transly_core::{AppConfig, Glossary, TranslationCache};

fn main() {
    let config = AppConfig::default();

    // 初始化持久化翻译缓存
    let cache = TranslationCache::default_shared();

    // 启动时自动做轻量缓存维护（清理超过 90 天条目，并保留最多 100,000 条）
    let _ = cache.cleanup_older_than_days(90);
    let _ = cache.trim_to_max_items(100_000);

    // 初始化全局术语库并从 SQLite 加载持久化规则
    let mut initial_glossary = Glossary::new();
    if let Ok(terms) = cache.get_all_glossary_terms() {
        for (src, tgt) in terms {
            initial_glossary.insert(src, tgt);
        }
    }
    let glossary = Arc::new(RwLock::new(initial_glossary));

    // 初始化全局共享引擎池（长连接复用）
    let pool = Arc::new(EnginePool::new());
    // 获取或生成用于本地扩展通信的 Bearer Token（失败则不启动服务，杜绝固定 Token 降级）
    let token = match secrets::get_or_create_server_token() {
        Ok(t) => Some(Arc::new(RwLock::new(t))),
        Err(e) => {
            eprintln!("系统安全凭据管理器不可用，已禁用本地扩展服务: {e}");
            None
        }
    };

    let builder = src_tauri::app_command_builder!();

    #[cfg(debug_assertions)]
    builder
        .export(Typescript::default(), "../src/bindings.ts")
        .expect("Failed to export typescript bindings");

    // 剪贴板监听开关（从持久化凭据库加载状态，默认关闭）
    let initial_monitor = secrets::get_secret("clipboard_monitor_enabled")
        .map(|v| v == "true")
        .unwrap_or(false);
    let clipboard_monitor_enabled = Arc::new(AtomicBool::new(initial_monitor));

    let monitor_flag = clipboard_monitor_enabled.clone();

    // 划词自触发抑制标志与跨入口请求 ID 计数器
    // 注意：selection_flag 不可 .manage()，Tauri 按 TypeId 存 state，
    // 与 clipboard_monitor_enabled 同为 Arc<AtomicBool> 会 panic
    let selection_flag = Arc::new(AtomicBool::new(false));
    let request_counter = Arc::new(AtomicU64::new(0));

    // 缓存状态协调器（从系统凭据读取初始开关，纳入 generation 版本控制）
    let initial_cache_enabled = secrets::get_secret("cache_enabled")
        .map(|v| v != "false")
        .unwrap_or(true);
    let cache_controller = Arc::new(src_tauri::cache_controller::CacheController::new(
        initial_cache_enabled,
    ));

    // 克隆传递给 setup 内部异步启动
    let server_token = token.clone();
    let server_cache = cache.clone();
    let server_pool = pool.clone();
    let server_glossary = glossary.clone();
    let server_port = config.local_server_port;
    let server_cache_controller = cache_controller.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(cache)
        .manage(pool)
        .manage(glossary)
        .manage(clipboard_monitor_enabled)
        .manage(cache_controller)
        .manage(Arc::new(commands::FloatSticky::default()))
        .manage(Arc::new(commands::PendingFloatRequest::default()))
        .manage(token.unwrap_or_else(|| Arc::new(RwLock::new(String::new()))))
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            let pending_slot = app
                .state::<Arc<commands::PendingFloatRequest>>()
                .inner()
                .clone();
            // 在正确的 Tauri async_runtime 上下文中启动本地 HTTP 服务（仅在成功生成安全 Token 时启动）
            if let Some(token_val) = server_token {
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = server::start_server(
                        server_port,
                        token_val,
                        server_cache,
                        server_pool,
                        server_glossary,
                        server_cache_controller,
                    )
                    .await
                    {
                        eprintln!("本地 HTTP 服务启动失败: {e}");
                    }
                });
            }

            // 注册全局快捷键 (Alt+Shift+D)，自动模拟复制当前屏幕选区文本并唤起轻量悬浮窗
            let app_handle = app.handle().clone();
            let sel_flag = selection_flag.clone();
            let counter = request_counter.clone();
            let _ =
                app.global_shortcut()
                    .on_shortcut("Alt+Shift+D", move |_app, _shortcut, event| {
                        if event.state() == ShortcutState::Pressed {
                            let handle = app_handle.clone();
                            let sel_flag = sel_flag.clone();
                            let counter = counter.clone();
                            let pending_slot = pending_slot.clone();
                            tauri::async_runtime::spawn(async move {
                                // 0. 备份旧剪贴板内容
                                let old_clipboard = handle.clipboard().read_text().ok();

                                // 模拟复制期间置抑制标志，防止剪贴板监听对同一次复制自触发翻译
                                sel_flag.store(true, Ordering::SeqCst);

                                // 1. 模拟按键发送 Ctrl+C (Windows/Linux) 或 Cmd+C (macOS)
                                if let Ok(mut enigo) = Enigo::new(&EnigoSettings::default()) {
                                    #[cfg(target_os = "macos")]
                                    let modifier = Key::Meta;
                                    #[cfg(not(target_os = "macos"))]
                                    let modifier = Key::Control;

                                    let _ = enigo.key(modifier, Direction::Press);
                                    let _ = enigo.key(Key::Unicode('c'), Direction::Click);
                                    let _ = enigo.key(modifier, Direction::Release);
                                }

                                // 2. 稍微等待系统剪贴板更新
                                tokio::time::sleep(std::time::Duration::from_millis(80)).await;

                                // 3. 提取剪贴板文本并唤起轻量悬浮窗
                                let text = handle.clipboard().read_text().unwrap_or_default();
                                if !text.trim().is_empty() {
                                    let rid = counter.fetch_add(1, Ordering::SeqCst);
                                    let req = commands::FloatRequest {
                                        text,
                                        source: "selection",
                                        request_id: rid,
                                    };
                                    // 先暂存再广播：浮窗未就绪时靠主动拉取兜底
                                    if let Ok(mut slot) = pending_slot.0.lock() {
                                        *slot = Some(req.clone());
                                    }
                                    if let Some(float_win) = handle.get_webview_window("float") {
                                        let _ = float_win.show();
                                        let _ = float_win.set_focus();
                                        let _ = float_win.emit("float-translate", req);
                                    }
                                }

                                // 4. 恢复原剪贴板内容，避免污染用户复制历史
                                if let Some(old_clipboard) = old_clipboard {
                                    let _ = handle.clipboard().write_text(old_clipboard);
                                }

                                // 无论成败均释放抑制标志，避免剪贴板监听被永久抑制
                                sel_flag.store(false, Ordering::SeqCst);
                            });
                        }
                    });

            // 启动后台剪贴板变动监听任务
            let clip_handle = app.handle().clone();
            let flag = monitor_flag.clone();
            let clip_sel_flag = selection_flag.clone();
            let clip_counter = request_counter.clone();
            let clip_pending = app
                .state::<Arc<commands::PendingFloatRequest>>()
                .inner()
                .clone();
            tauri::async_runtime::spawn(async move {
                let last_clipboard = std::sync::Mutex::new(String::new());
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    if !flag.load(Ordering::Relaxed) {
                        continue;
                    }
                    // 划词模拟复制期间不自触发，防止同一段文本翻译两次、浮窗弹两次
                    if src_tauri::clipboard_gate::is_selection_in_progress(&clip_sel_flag) {
                        continue;
                    }
                    let Ok(curr) = clip_handle.clipboard().read_text() else {
                        continue;
                    };
                    // 去重 + 最短长度门槛：相同内容或过短内容不重复打扰
                    if !src_tauri::clipboard_gate::should_emit(&last_clipboard, &curr) {
                        continue;
                    }
                    let rid = clip_counter.fetch_add(1, Ordering::SeqCst);
                    let req = commands::FloatRequest {
                        text: curr.trim().to_string(),
                        source: "clipboard",
                        request_id: rid,
                    };
                    // 先暂存再广播：浮窗未就绪时靠主动拉取兜底
                    if let Ok(mut slot) = clip_pending.0.lock() {
                        *slot = Some(req.clone());
                    }
                    if let Some(float_win) = clip_handle.get_webview_window("float") {
                        let _ = float_win.show();
                        let _ = float_win.set_focus();
                        let _ = float_win.emit("float-translate", req);
                    }
                }
            });

            // 构建系统托盘菜单
            let show_i = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "退出 Transly", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

            let _tray = TrayIconBuilder::new()
                .tooltip("Transly 桌面翻译")
                .icon(
                    app.default_window_icon()
                        .expect("Tauri 上下文必须内置默认窗口图标")
                        .clone(),
                )
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
            } else if let WindowEvent::Focused(false) = event {
                // 悬浮迷你窗在失去焦点时自动隐藏收起；但翻译中/错误态下粘滞，
                // 否则错误提示与重试按钮来不及点击
                let is_sticky = window
                    .state::<Arc<commands::FloatSticky>>()
                    .0
                    .load(Ordering::SeqCst);
                if window.label() == "float" && !is_sticky {
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
