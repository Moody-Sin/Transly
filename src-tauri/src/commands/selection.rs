//! 划词与悬浮窗相关的平台能力。
//!
//! 从 `commands/mod.rs` 拆出：原地替换是一个含三阶段剪贴板竞态处理的完整业务
//! 流程（含 `enigo` 模拟按键），并非「薄封装」，与翻译命令混在一个文件里
//! 会让该模块的职责边界模糊（原文件已逾 2300 行）。

use crate::commands::{FloatRequest, PendingFloatRequests, TranslationDependencies};
use std::sync::Arc;
use transly_core::TranslationRequest;

pub const DEFAULT_SHORTCUT: &str = "Alt+Shift+D";
pub const DEFAULT_REPLACE_SHORTCUT: &str = "Alt+Shift+R";

/// 全局剪贴板自动化互斥锁，确保划词悬浮呼出与原地替换模拟流程不发生并发抢占
static CLIPBOARD_SIMULATION_MUTEX: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub struct SelectionContext {
    pub sel_flag: Arc<std::sync::atomic::AtomicBool>,
    pub counter: Arc<std::sync::atomic::AtomicU64>,
}

/// 划词原地替换失败事件载荷（通过 Tauri 事件广播至前端显示 Toast 告警）
#[derive(Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ReplacementFailedPayload {
    pub engine: String,
    pub message: String,
}

/// 触发划词原地翻译并替换工作流：
/// 1. 模拟复制 (Ctrl+C / Cmd+C) 获取当前选区文本
/// 2. 使用离线检测推断智能目标语言并执行翻译
/// 3. 将译文写入剪贴板并模拟粘贴 (Ctrl+V / Cmd+V) 覆盖原选区
/// 4. 延迟后恢复用户原有剪贴板，全程置 `sel_flag` 抑制剪贴板监听器自触发
pub fn trigger_selection_replacement(
    handle: tauri::AppHandle,
    sel_flag: Arc<std::sync::atomic::AtomicBool>,
    deps: Arc<TranslationDependencies>,
) {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings as EnigoSettings};
    use std::sync::atomic::Ordering;
    use tauri::{Emitter, Manager};
    use tauri_plugin_clipboard_manager::ClipboardExt;

    tauri::async_runtime::spawn(async move {
        // 抑制标志必须覆盖**整个**流程（含第二阶段数秒的网络翻译）。
        // 早期实现在一阶段结束（读到选区原文）就置回 false，而译文要到三阶段才写入，
        // 中间这段窗口里剪贴板监听器会把选区原文当成用户新复制的内容再触发一次
        // 浮窗翻译，同一段文本被翻译两次；更糟的是那次误触发会把去重缓存
        // `last_clipboard` 改写成选区原文，导致用户随后手动复制同一段文本时不再触发。
        //
        // 用 RAII guard 保证任何提前 return 的路径都能正确复位，
        // 否则翻译失败一次就会把剪贴板监听永久抑制。
        struct SelFlagGuard(Arc<std::sync::atomic::AtomicBool>);
        impl Drop for SelFlagGuard {
            fn drop(&mut self) {
                self.0.store(false, Ordering::SeqCst);
            }
        }
        let _sel_guard = SelFlagGuard(Arc::clone(&sel_flag));

        // 第一阶段：原子获取模拟复制，读取所选文本并立即释放全局剪贴板锁
        let (detected_text, old_clipboard) = {
            let _guard = CLIPBOARD_SIMULATION_MUTEX.lock().await;
            sel_flag.store(true, Ordering::SeqCst);

            // 0. 备份原剪贴板
            let old_clipboard = handle.clipboard().read_text().ok();

            // 1. 模拟 Ctrl+C
            if let Ok(mut enigo) = Enigo::new(&EnigoSettings::default()) {
                #[cfg(target_os = "macos")]
                let modifier = Key::Meta;
                #[cfg(not(target_os = "macos"))]
                let modifier = Key::Control;

                let _ = enigo.key(modifier, Direction::Press);
                let _ = enigo.key(Key::Unicode('c'), Direction::Click);
                let _ = enigo.key(modifier, Direction::Release);
            }

            // 2. 自适应轮询读取选中文本（单次 15ms，长文本自适应上限放宽至 300ms，内容变化立即放行）
            let clipboard_reader = {
                let h = handle.clone();
                move || h.clipboard().read_text().ok()
            };
            let detected = crate::clipboard_gate::poll_clipboard_change(
                old_clipboard.as_deref(),
                clipboard_reader,
                std::time::Duration::from_millis(15),
                std::time::Duration::from_millis(300),
                // 破坏性写场景必须 fail-closed：无法区分「没有选中文本」与
                // 「选中的内容恰好等于旧剪贴板」，回退返回旧剪贴板会把用户
                // 几分钟前复制的内容翻译后强行粘贴到光标处。
                crate::clipboard_gate::UnchangedFallback::Abort,
            )
            .await;
            // 离开本作用域自动释放 _guard 锁
            (detected, old_clipboard)
        };

        let Some(text) = detected_text else {
            return;
        };
        let clean = text.trim();
        if clean.is_empty() || clean.chars().count() > transly_core::MAX_ITEM_CHARS {
            return;
        }

        // 第二阶段：无锁执行网络翻译与术语库处理（网络耗时不阻塞其他快捷键与剪贴板监听）
        let (cache_enabled, start_gen) = deps.cache_controller.snapshot();

        let (preferred_engine, configured_target_lang) = {
            let cache_ref = deps.cache.clone();
            tokio::task::spawn_blocking(move || {
                let eng = cache_ref
                    .get_setting("preferred_replace_engine")
                    .ok()
                    .flatten()
                    .or_else(|| crate::secrets::get_secret("preferred_replace_engine").ok())
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| "google".to_string());
                let lang = cache_ref
                    .get_setting("preferred_target_lang")
                    .ok()
                    .flatten()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| "zh-CN".to_string());
                (eng, lang)
            })
            .await
            .unwrap_or_else(|_| ("google".to_string(), "zh-CN".to_string()))
        };
        let target_lang = transly_core::resolve_smart_target_lang(clean, &configured_target_lang);

        let active_engine = deps.pool.get_engine(&preferred_engine).await;

        // 0. 先提取代码块保护，再对非代码正文应用全局术语库预替换
        let preserved = transly_core::stash_code_blocks(clean);
        let glossary_applied_source = {
            let g = deps.glossary.read().await;
            if g.is_empty() {
                preserved.text.clone()
            } else {
                g.apply(&preserved.text)
            }
        };
        let effective_source =
            transly_core::restore_code_blocks(&glossary_applied_source, &preserved.stash);

        // 优先查缓存（仅查询首选引擎专属缓存，绝不跨引擎命中 Google 降级缓存）
        let mut final_text = None;
        if cache_enabled {
            let cache_ref = deps.cache.clone();
            let eng_id = active_engine.id().to_string();
            let t = effective_source.clone();
            let tl = target_lang.clone();
            let hit = tokio::task::spawn_blocking(move || {
                cache_ref.get(&t, "auto", &tl, &eng_id).unwrap_or(None)
            })
            .await
            .unwrap_or(None);

            if let Some(cached_text) = hit {
                let g = deps.glossary.read().await;
                final_text = Some(g.apply_to_translation(&cached_text));
            }
        }

        let final_text = match final_text {
            Some(cached) => cached,
            None => {
                let req = TranslationRequest {
                    text: glossary_applied_source,
                    source_lang: "auto".to_string(),
                    target_lang: target_lang.clone(),
                };

                match active_engine.translate(&req).await {
                    Ok(res) => {
                        let restored =
                            transly_core::restore_code_blocks(&res.text, &preserved.stash);
                        let cache_target_engine = res.engine.clone();
                        let owned_text = effective_source;
                        let owned_restored = restored.clone();
                        let owned_target = target_lang.clone();
                        let cache_arc = Arc::clone(&deps.cache);
                        let controller = Arc::clone(&deps.cache_controller);
                        crate::cache_controller::write_cache_offloaded(
                            controller,
                            start_gen,
                            move || {
                                cache_arc.set(
                                    &owned_text,
                                    &owned_restored,
                                    "auto",
                                    &owned_target,
                                    &cache_target_engine,
                                )
                            },
                        )
                        .await;
                        let g = deps.glossary.read().await;
                        g.apply_to_translation(&restored)
                    }
                    Err(e) => {
                        eprintln!("[Selection Replacement Warning] 划词原地替换翻译失败: {e}");
                        let _ = handle.emit(
                            "transly://replacement-failed",
                            ReplacementFailedPayload {
                                engine: active_engine.id().to_string(),
                                message: e.to_string(),
                            },
                        );
                        if let Some(float_win) = handle.get_webview_window("float") {
                            place_float_near_cursor(&float_win);
                            let _ = float_win.show();
                        }
                        // 校验当前剪贴板是否仍为第一阶段模拟复制的选区原文：
                        // 若网络请求耗时期间用户在其它应用复制了新内容，则放弃还原，杜绝将用户的新复制内容冲掉。
                        let _guard = CLIPBOARD_SIMULATION_MUTEX.lock().await;
                        let current = handle.clipboard().read_text().ok();
                        if current.as_deref() == Some(clean) {
                            if let Some(old) = old_clipboard {
                                let _ = handle.clipboard().write_text(old);
                            }
                        }
                        return;
                    }
                }
            }
        };

        // 第三阶段：重新获取剪贴板锁，写入译文并模拟粘贴覆盖原选区
        {
            let _guard = CLIPBOARD_SIMULATION_MUTEX.lock().await;
            sel_flag.store(true, Ordering::SeqCst);

            // 关键安全防线：在写回剪贴板前，检查当前剪贴板内容是否已被用户外部改写。
            // 第二阶段（网络翻译）长达数秒，若用户在其他应用复制了新内容（例如代码、密码），
            // 当前剪贴板既不等于选区原文 clean，也不等于 old_clipboard，说明外部产生了新复制。
            // 此时必须放弃写回剪贴板与模拟粘贴，防止摧毁用户的新数据。
            let pre_write_current = handle.clipboard().read_text().ok();
            let is_unmodified =
                pre_write_current.as_deref() == Some(clean) || pre_write_current == old_clipboard;

            if !is_unmodified {
                eprintln!("[Selection Replacement Warning] 翻译等待期间用户复制了新内容，放弃原地粘贴与剪贴板覆盖");
                sel_flag.store(false, Ordering::SeqCst);
                return;
            }

            if handle.clipboard().write_text(&final_text).is_ok() {
                tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                if let Ok(mut enigo) = Enigo::new(&EnigoSettings::default()) {
                    #[cfg(target_os = "macos")]
                    let modifier = Key::Meta;
                    #[cfg(not(target_os = "macos"))]
                    let modifier = Key::Control;

                    let _ = enigo.key(modifier, Direction::Press);
                    let _ = enigo.key(Key::Unicode('v'), Direction::Click);
                    let _ = enigo.key(modifier, Direction::Release);
                }
                // 留出应用接收粘贴事件的窗口时间（放宽至 350ms，确保异步应用如 VSCode/Slack 成功消费）
                tokio::time::sleep(std::time::Duration::from_millis(350)).await;
            }

            // 恢复用户原有剪贴板内容。
            //
            // 判据是「当前剪贴板是否仍等于本流程写入的译文」：
            //   - 相等 → 期间无人干预，安全还原；
            //   - 不等 → 已被外部改写，必须放弃还原，保留用户的新内容。
            let current = handle.clipboard().read_text().ok();
            if current.as_deref() == Some(final_text.as_str()) {
                if let Some(old) = old_clipboard {
                    if let Err(e) = handle.clipboard().write_text(old) {
                        eprintln!("[Clipboard Warning] 原地替换后恢复原剪贴板失败: {e}");
                    }
                }
            }

            sel_flag.store(false, Ordering::SeqCst);
        }
    });
}

/// 将浮窗移动到光标附近，并夹取到光标所在显示器的可视范围内。
///
/// 划词与剪贴板翻译的交互预期是「译文出现在选区附近」，不能沿用系统默认坐标；
/// 光标或显示器信息不可用时保持系统默认位置，不阻断浮窗展示。
pub fn place_float_near_cursor(win: &tauri::WebviewWindow) {
    let (Ok(cursor), Ok(size)) = (win.cursor_position(), win.outer_size()) else {
        return;
    };
    let monitor = win
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| win.primary_monitor().ok().flatten());

    let (min_x, min_y, max_x, max_y) = match monitor {
        Some(m) => {
            let pos = m.position();
            let monitor_size = m.size();
            (
                pos.x as f64,
                pos.y as f64,
                pos.x as f64 + monitor_size.width as f64,
                pos.y as f64 + monitor_size.height as f64,
            )
        }
        None => (0.0, 0.0, f64::MAX, f64::MAX),
    };

    let width = size.width as f64;
    let height = size.height as f64;
    // 水平居中对齐光标，垂直放在光标下方，避开正在阅读的选区
    let x = (cursor.x - width / 2.0).clamp(min_x, (max_x - width).max(min_x));
    let y = (cursor.y + 20.0).clamp(min_y, (max_y - height).max(min_y));
    let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
}

pub fn trigger_selection_translation(
    handle: tauri::AppHandle,
    sel_flag: Arc<std::sync::atomic::AtomicBool>,
    counter: Arc<std::sync::atomic::AtomicU64>,
    pending_slot: Arc<PendingFloatRequests>,
) {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings as EnigoSettings};
    use std::sync::atomic::Ordering;
    use tauri::{Emitter, Manager};
    use tauri_plugin_clipboard_manager::ClipboardExt;

    tauri::async_runtime::spawn(async move {
        // 使用 RAII Guard 确保任何提前返回或 panic 都能可靠复位抑制标志，
        // 杜绝划词异常导致系统剪贴板监听被永久抑制
        struct SelFlagGuard(Arc<std::sync::atomic::AtomicBool>);
        impl Drop for SelFlagGuard {
            fn drop(&mut self) {
                self.0.store(false, Ordering::SeqCst);
            }
        }
        let _sel_guard = SelFlagGuard(Arc::clone(&sel_flag));

        // 获取互斥锁排队，避免划词悬浮与原地替换同时模拟读写剪贴板发生数据竞态
        let _guard = CLIPBOARD_SIMULATION_MUTEX.lock().await;

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

        // 2. 自适应短轮询检测系统剪贴板更新（单次 15ms，最多等 180ms，内容变化立即放行）
        let clipboard_reader = {
            let h = handle.clone();
            move || h.clipboard().read_text().ok()
        };
        let detected_text = crate::clipboard_gate::poll_clipboard_change(
            old_clipboard.as_deref(),
            clipboard_reader,
            std::time::Duration::from_millis(15),
            std::time::Duration::from_millis(180),
            // 划词浮窗只读取并展示，误判无副作用；保留兜底以兼容
            // 「所选内容恰好等于旧剪贴板」的场景，避免漏翻。
            crate::clipboard_gate::UnchangedFallback::ReturnLastSeen,
        )
        .await;

        // 3. 提取剪贴板文本并唤起轻量悬浮窗
        if let Some(text) = detected_text {
            if !text.trim().is_empty() {
                let rid = counter.fetch_add(1, Ordering::SeqCst);
                let req = FloatRequest {
                    text,
                    source: "selection",
                    request_id: rid,
                };
                // 先暂存再广播：浮窗未就绪时靠主动拉取兜底
                if pending_slot.push(req.clone()) {
                    eprintln!("浮窗请求队列已满，丢弃最旧请求");
                }
                if let Some(float_win) = handle.get_webview_window("float") {
                    place_float_near_cursor(&float_win);
                    let _ = float_win.show();
                    let _ = float_win.set_focus();
                    let _ = float_win.emit_to("float", "float-translate", req);
                }
            }
        }

        // 4. 恢复原剪贴板内容，避免污染用户复制历史
        if let Some(old_clipboard) = old_clipboard {
            if let Err(e) = handle.clipboard().write_text(old_clipboard) {
                eprintln!("[Clipboard Warning] 划词后恢复原剪贴板失败（可能被外部锁占用）: {e}");
            }
        }

        // 无论成败均释放抑制标志，避免剪贴板监听被永久抑制
        sel_flag.store(false, Ordering::SeqCst);
    });
}
