// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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

use src_tauri::commands::place_float_near_cursor;

/// 解析应用数据目录，并一次性把旧版缓存迁移过来。
///
/// 目录用 Tauri 约定 `data_dir/<identifier>`，而非旧版手拼的 `%APPDATA%\Transly`：
/// 后者与沙箱、清理、多用户策略都对不上，macOS 上也不符合平台惯例。
/// 平台分支放在此层（平台适配层），`transly-core` 保持零平台假设。
///
/// 迁移是必须的：术语库与翻译缓存在同一个 SQLite 文件中，
/// 不迁移就等于直接丢掉用户自建的术语。
fn resolve_data_dir(identifier: &str) -> std::path::PathBuf {
    let (dir, is_portable) = src_tauri::storage::resolve_data_dir(identifier);
    let _ = std::fs::create_dir_all(&dir);
    if is_portable {
        println!("已启用单文件便携模式，数据隔离保存在: {dir:?}");
        return dir;
    }
    // 先迁移最老版本的目录（<父目录>/Transly），再迁移漫游目录（%APPDATA%）。
    // 顺序有讲究：两者都存在时以更早的历史数据为准，且迁移本身不覆盖已有新库。
    match src_tauri::storage::migrate_legacy_cache(&dir) {
        Ok(true) => println!("已将旧版缓存迁移至 {dir:?}"),
        Ok(false) => {}
        Err(e) => eprintln!("旧版缓存迁移失败（不影响启动）: {e}"),
    }
    match src_tauri::storage::migrate_roaming_cache(&dir) {
        Ok(true) => println!("已将漫游目录缓存迁移至本机目录 {dir:?}"),
        Ok(false) => {}
        Err(e) => eprintln!("漫游目录缓存迁移失败（不影响启动）: {e}"),
    }
    dir
}

fn main() {
    let context = tauri::generate_context!();

    // 端口可通过环境变量覆盖，便于端口被占用时自救；默认值来自 AppConfig
    let config = AppConfig::default();
    let server_port = std::env::var("TRANSLY_SERVER_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(config.local_server_port);

    // 初始化持久化翻译缓存（目录见 resolve_data_dir）
    let cache = TranslationCache::open_default(resolve_data_dir(&context.config().identifier));

    // 执行一次性从系统 Keyring 迁移非敏感配置到 SQLite app_settings
    // 全新安装或已完成迁移后，app_settings 包含 __migrated_from_keyring 标记，
    // 后续启动彻底为 0 次系统凭据管理器调用，彻底消除主线程阻塞！
    src_tauri::storage::migrate_keyring_settings_to_db(&cache);

    // 启动时将轻量缓存维护（清理超过 90 天条目，并保留最多 100,000 条）投递至后台线程，避免阻塞主线程冷启动
    let startup_cache = cache.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _ = startup_cache.cleanup_older_than_days(90);
        let _ = startup_cache.trim_to_max_items(100_000);
    });

    // 初始化全局术语库并从 SQLite 加载持久化规则
    let mut initial_glossary = Glossary::new();
    if let Ok(terms) = cache.get_all_glossary_terms() {
        for (src, tgt, match_case) in terms {
            initial_glossary.insert_with_options(src, tgt, match_case);
        }
    }
    let glossary = Arc::new(RwLock::new(initial_glossary));

    // 初始化全局共享引擎池（关联本地 SQLite 缓存管理器以秒级快速加载非敏感设置）
    let pool = Arc::new(EnginePool::with_cache(cache.clone()));
    // 获取或生成用于本地扩展通信的 Bearer Token（失败则不启动服务，杜绝固定 Token 降级）
    let token = match secrets::get_or_create_server_token() {
        Ok(t) => Some(Arc::new(RwLock::new(t))),
        Err(e) => {
            eprintln!("系统安全凭据管理器不可用，已禁用本地扩展服务: {e}");
            #[cfg(target_os = "linux")]
            eprintln!(
                "💡 [Linux 提示] 若当前为 WSL 或无桌面会话，桌面端核心翻译仍可使用；若需启用扩展服务与凭据持久化，请安装并启动 Secret Service（如 gnome-keyring / kwallet）。"
            );
            None
        }
    };

    let builder = src_tauri::app_command_builder!();

    // 开发期自动刷新绑定。导出逻辑与路径基准统一在 `src_tauri::export_bindings`，
    // 不在此处再写一遍路径（相对路径会被按进程 cwd 解析，
    // 从仓库根执行 `cargo test` / `cargo run` 时会在仓库外凭空生成文件）。
    #[cfg(debug_assertions)]
    src_tauri::export_bindings().expect("Failed to export typescript bindings");
    // 剪贴板监听开关（从本地 SQLite 设置表加载，零系统凭据库查询开销）
    let initial_monitor = cache
        .get_setting("clipboard_monitor_enabled")
        .ok()
        .flatten()
        .map(|v| v == "true")
        .unwrap_or(false);
    let clipboard_monitor_enabled = Arc::new(AtomicBool::new(initial_monitor));

    let monitor_flag = clipboard_monitor_enabled.clone();

    // 划词自触发抑制标志与跨入口请求 ID 计数器
    // 注意：selection_flag 不可 .manage()，Tauri 按 TypeId 存 state，
    // 与 clipboard_monitor_enabled 同为 Arc<AtomicBool> 会 panic
    let selection_flag = Arc::new(AtomicBool::new(false));
    let request_counter = Arc::new(AtomicU64::new(0));

    // 缓存状态协调器（从本地 SQLite 设置表读取初始开关，纳入 generation 版本控制）
    let initial_cache_enabled = cache
        .get_setting("cache_enabled")
        .ok()
        .flatten()
        .map(|v| v != "false")
        .unwrap_or(true);
    let cache_controller = Arc::new(src_tauri::cache_controller::CacheController::new(
        initial_cache_enabled,
    ));

    let pairing_manager = Arc::new(src_tauri::server::pairing::PairingManager::new());

    // 截图翻译会话状态。必须 newtype（Tauri 按 TypeId 存 state，同类型重复 manage 会 panic），
    // 且以 Arc 共享给自定义 URI 协议的闭包。
    let screenshot_session = Arc::new(src_tauri::overlay::ScreenshotSession::default());

    // 克隆传递给 setup 内部异步启动
    let server_token = token.clone();
    let server_cache = cache.clone();
    let server_pool = pool.clone();
    let server_glossary = glossary.clone();
    let server_cache_controller = cache_controller.clone();
    let server_pairing_manager = pairing_manager.clone();

    let sel_flag_shared = selection_flag.clone();
    let counter_shared = request_counter.clone();

    let stream_task_registry = Arc::new(commands::StreamTaskRegistry::default());
    let shortcut_status_slot = Arc::new(commands::ShortcutStatusSlot::default());
    let translation_deps = Arc::new(commands::TranslationDependencies {
        cache: cache.clone(),
        pool: pool.clone(),
        glossary: glossary.clone(),
        cache_controller: cache_controller.clone(),
        stream_registry: stream_task_registry.clone(),
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(cache.clone())
        .manage(pool)
        .manage(glossary)
        .manage(clipboard_monitor_enabled)
        .manage(cache_controller)
        .manage(pairing_manager)
        .manage(translation_deps)
        .manage(Arc::new(commands::FloatSticky::default()))
        .manage(stream_task_registry)
        .manage(Arc::clone(&shortcut_status_slot))
        .manage(Arc::new(commands::PendingFloatRequests::default()))
        .manage(Arc::new(commands::ServerStatusSlot::default()))
        .manage(Arc::new(commands::SelectionContext {
            sel_flag: sel_flag_shared,
            counter: counter_shared,
        }))
        .manage(token.unwrap_or_else(|| Arc::new(RwLock::new(String::new()))))
        .manage(Arc::clone(&screenshot_session))
        // 冻结帧走自定义协议而非 IPC：4K 帧约 33MB，走 IPC 需 base64 再 JSON.parse，
        // JS 堆峰值可达图像 3 倍。session_id 不匹配即 404，防陈旧窗口读到新会话图。
        .register_uri_scheme_protocol("translyshot", {
            let shot_session = Arc::clone(&screenshot_session);
            move |_ctx, request| {
                // 路径形如 `/{session_id}/base.png`
                let session_id = request
                    .uri()
                    .path()
                    .trim_start_matches('/')
                    .split('/')
                    .next()
                    .unwrap_or_default();
                match src_tauri::overlay::frame_for_session(&shot_session, session_id) {
                    Some(png) => tauri::http::Response::builder()
                        .status(200)
                        .header("Content-Type", "image/png")
                        .header("Cache-Control", "no-store")
                        .body(png)
                        .unwrap_or_else(|_| tauri::http::Response::new(Vec::new())),
                    None => tauri::http::Response::builder()
                        .status(404)
                        .body(Vec::new())
                        .unwrap_or_else(|_| tauri::http::Response::new(Vec::new())),
                }
            }
        })
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            let pending_slot = app
                .state::<Arc<commands::PendingFloatRequests>>()
                .inner()
                .clone();
            let status_slot = app
                .state::<Arc<commands::ServerStatusSlot>>()
                .inner()
                .clone();
            let app_handle_for_server = app.handle().clone();
            let pairing_notify_hook: src_tauri::server::PairingNotifyHook = Arc::new(
                move |session_id: &str, expires_in: u64, browser_hint: Option<&str>| {
                    #[derive(serde::Serialize, Clone)]
                    struct PairingRequestedPayload {
                        session_id: String,
                        expires_in: u64,
                        browser_hint: Option<String>,
                    }
                    let _ = app_handle_for_server.emit(
                        "transly://pairing-requested",
                        PairingRequestedPayload {
                            session_id: session_id.to_string(),
                            expires_in,
                            browser_hint: browser_hint.map(ToString::to_string),
                        },
                    );
                },
            );

            // 在正确的 Tauri async_runtime 上下文中启动本地 HTTP 服务（仅在成功生成安全 Token 时启动）
            match server_token {
                Some(token_val) => {
                    tauri::async_runtime::spawn(async move {
                        // bind 成功即上报就绪：serve 正常只在出错时返回
                        let listening_slot = status_slot.clone();
                        let result = server::start_server(
                            server_port,
                            token_val,
                            server_cache,
                            server_pool,
                            server_glossary,
                            server_cache_controller,
                            server_pairing_manager,
                            Some(pairing_notify_hook),
                            move || listening_slot.set(commands::ServerStatus::Running),
                        )
                        .await;
                        if let Err(e) = result {
                            // 端口被占用是最常见的失败原因，用户必须看得到，
                            // 否则只能看到扩展端“桌面端未运行”而无法自查
                            eprintln!("本地 HTTP 服务启动失败: {e}");
                            status_slot.set(commands::ServerStatus::Failed {
                                reason: format!("本地扩展服务未能启动（端口 {server_port}）：{e}"),
                            });
                        }
                    });
                }
                None => {
                    status_slot.set(commands::ServerStatus::Failed {
                        reason: "系统安全凭据管理器不可用，已禁用本地扩展服务".to_string(),
                    });
                }
            }

            // 注册全局快捷键（直接从本地 SQLite 设置读取，零凭据库阻塞）
            let active_shortcut = cache
                .get_setting("custom_shortcut")
                .ok()
                .flatten()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| commands::DEFAULT_SHORTCUT.to_string());

            let active_replace_shortcut = cache
                .get_setting("custom_replace_shortcut")
                .ok()
                .flatten()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| commands::DEFAULT_REPLACE_SHORTCUT.to_string());

            let app_handle = app.handle().clone();
            let sel_flag = selection_flag.clone();
            let counter = request_counter.clone();
            let selection_slot = shortcut_status_slot.clone();
            let selection_accel = active_shortcut.clone();
            // 注册失败必须可见：`Alt+Shift+D` 极易被其他软件占用，
            // 静默丢弃会让用户看到「划词功能莫名其妙不工作」且毫无线索。
            let selection_reg = app.global_shortcut().on_shortcut(
                active_shortcut.as_str(),
                move |_app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        commands::trigger_selection_translation(
                            app_handle.clone(),
                            sel_flag.clone(),
                            counter.clone(),
                            pending_slot.clone(),
                        );
                    }
                },
            );
            match selection_reg {
                Ok(()) => {
                    selection_slot.set_translate_ok();
                }
                Err(err) => {
                    eprintln!(
                        "[Shortcut Warning] 划词浮窗快捷键 {selection_accel} 注册失败：{err}"
                    );
                    selection_slot.set_translate_failed(format!("{selection_accel}: {err}"));
                }
            }

            let replace_app_handle = app.handle().clone();
            let replace_sel_flag = selection_flag.clone();
            let replace_deps = app
                .state::<Arc<commands::TranslationDependencies>>()
                .inner()
                .clone();
            let replace_accel = active_replace_shortcut.clone();
            let replace_slot = shortcut_status_slot.clone();
            let replace_reg = app.global_shortcut().on_shortcut(
                active_replace_shortcut.as_str(),
                move |_app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        commands::trigger_selection_replacement(
                            replace_app_handle.clone(),
                            replace_sel_flag.clone(),
                            replace_deps.clone(),
                        );
                    }
                },
            );
            match replace_reg {
                Ok(()) => {
                    replace_slot.set_replace_ok();
                }
                Err(err) => {
                    eprintln!(
                        "[Shortcut Warning] 划词原地替换快捷键 {replace_accel} 注册失败：{err}"
                    );
                    replace_slot.set_replace_failed(format!("{replace_accel}: {err}"));
                }
            }

            // 第三个快捷键：截图翻译。
            // 既有两份**保持原样**（其回滚与竞态逻辑是历史踩坑的产物，动它们风险大于收益），
            // 本份只沿用同一个四段式骨架。
            let active_screenshot_shortcut = cache
                .get_setting("custom_screenshot_shortcut")
                .ok()
                .flatten()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| commands::screenshot::DEFAULT_SCREENSHOT_SHORTCUT.to_string());

            let shot_accel = active_screenshot_shortcut.clone();
            let shot_slot = shortcut_status_slot.clone();
            let shot_deps = app
                .state::<Arc<commands::TranslationDependencies>>()
                .inner()
                .clone();
            let shot_reg = app.global_shortcut().on_shortcut(
                active_screenshot_shortcut.as_str(),
                move |_app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        commands::screenshot::trigger_screenshot_translation(
                            _app.clone(),
                            shot_deps.clone(),
                        );
                    }
                },
            );
            match shot_reg {
                Ok(()) => {
                    shot_slot.set_screenshot_ok();
                }
                Err(err) => {
                    eprintln!("[Shortcut Warning] 截图翻译快捷键 {shot_accel} 注册失败：{err}");
                    shot_slot.set_screenshot_failed(format!("{shot_accel}: {err}"));
                }
            }

            // 启动后台剪贴板变动监听任务
            let clip_handle = app.handle().clone();
            let flag = monitor_flag.clone();
            let clip_sel_flag = selection_flag.clone();
            let clip_counter = request_counter.clone();
            let clip_pending = app
                .state::<Arc<commands::PendingFloatRequests>>()
                .inner()
                .clone();
            tauri::async_runtime::spawn(async move {
                let last_clipboard = std::sync::Mutex::new(String::new());
                loop {
                    if !flag.load(Ordering::Relaxed) {
                        // 禁用监听时降频休眠，避免持续产生无意义的空唤醒与 CPU 消耗
                        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                        continue;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
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
                    if clip_pending.push(req.clone()) {
                        eprintln!("浮窗请求队列已满，丢弃最旧请求");
                    }
                    if let Some(float_win) = clip_handle.get_webview_window("float") {
                        place_float_near_cursor(&float_win);
                        let _ = float_win.show();
                        let _ = float_win.set_focus();
                        // 同上：仅投递给浮窗，避免主窗口重复翻译
                        let _ = float_win.emit_to("float", "float-translate", req);
                    }
                }
            });

            // 构建系统托盘菜单（支持根据语言偏好国际化）
            let is_en = cache
                .get_setting("transly_locale")
                .ok()
                .flatten()
                .map(|v| v.starts_with("en"))
                .unwrap_or(false);

            let (show_text, shot_text, quit_text, tooltip_text) = if is_en {
                (
                    "Show Main Window",
                    "Screenshot Translation",
                    "Quit Transly",
                    "Transly Desktop Translator",
                )
            } else {
                ("显示主窗口", "截图翻译", "退出 Transly", "Transly 桌面翻译")
            };

            let show_i = MenuItem::with_id(app, "show", show_text, true, None::<&str>)?;
            let shot_i = MenuItem::with_id(app, "screenshot", shot_text, true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", quit_text, true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &shot_i, &quit_i])?;

            let _tray = match TrayIconBuilder::new()
                .tooltip(tooltip_text)
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
                    "screenshot" => {
                        // 必须延迟：此刻托盘菜单弹层仍在屏幕上，立即捕获会把菜单
                        // 连同高亮项一起截进冻结帧。200–250ms 足以等弹层收起。
                        let app_shot = app.clone();
                        let deps_shot = app
                            .state::<Arc<commands::TranslationDependencies>>()
                            .inner()
                            .clone();
                        tauri::async_runtime::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                            commands::screenshot::trigger_screenshot_translation(
                                app_shot, deps_shot,
                            );
                        });
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
                .build(app)
            {
                Ok(tray) => Some(tray),
                Err(err) => {
                    eprintln!("[Tray Warning] 系统托盘图标初始化失败: {err}");
                    #[cfg(target_os = "linux")]
                    eprintln!(
                        "💡 [Linux 提示] 若在 Linux 环境下缺少系统托盘库，主程序仍可使用主窗口；若需启用托盘图标，请安装 libayatana-appindicator3-1：sudo apt-get install -y libayatana-appindicator3-1"
                    );
                    None
                }
            };

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
        .run(context)
        .expect("error while running tauri application");
}
