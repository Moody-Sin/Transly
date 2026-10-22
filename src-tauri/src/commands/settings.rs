//! 剪贴板监听 / OCR / 全局快捷键 / 语言检测命令
//!
//! 由 `commands/mod.rs` 拆分而来：命令函数保持在 crate 内的原路径不变
//! （`mod.rs` 统一 `pub use` 重导出），因此 `app_command_builder!` 与
//! `src/bindings.ts` 的生成结果不受影响。

use std::sync::Arc;

use transly_core::TranslationCache;

use super::{
    trigger_selection_replacement, trigger_selection_translation, PendingFloatRequests,
    SelectionContext, ShortcutStatusSlot, TranslationDependencies, DEFAULT_REPLACE_SHORTCUT,
    DEFAULT_SHORTCUT,
};

#[tauri::command]
#[specta::specta]
pub async fn set_clipboard_monitor(
    enabled: bool,
    flag: tauri::State<'_, Arc<std::sync::atomic::AtomicBool>>,
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<(), String> {
    let cache_ref = cache.inner().clone();
    let val = if enabled { "true" } else { "false" };
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        cache_ref
            .set_setting("clipboard_monitor_enabled", val)
            .map_err(|e| anyhow::anyhow!("配置保存失败: {e}"))?;
        let _ = crate::secrets::delete_secret("clipboard_monitor_enabled");
        Ok(())
    })
    .await
    .map_err(|e| format!("配置保存调度异常: {e}"))?
    .map_err(|e| e.to_string())?;
    flag.store(enabled, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_clipboard_monitor(app: tauri::AppHandle) -> bool {
    use tauri::Manager;
    let cache_ref = match app.try_state::<Arc<TranslationCache>>() {
        Some(c) => c.inner().clone(),
        None => return false,
    };
    tokio::task::spawn_blocking(move || {
        cache_ref
            .get_setting("clipboard_monitor_enabled")
            .ok()
            .flatten()
            .or_else(|| crate::secrets::get_secret("clipboard_monitor_enabled").ok())
    })
    .await
    .map(|res| res.map(|v| v == "true").unwrap_or(false))
    .unwrap_or(false)
}

#[tauri::command]
#[specta::specta]
pub async fn ocr_clipboard_image(
    target_lang: String,
    app: tauri::AppHandle,
) -> Result<String, transly_core::OcrErrorDto> {
    crate::ocr::recognize_clipboard_image(&app, &target_lang).await
}

#[tauri::command]
#[specta::specta]
pub async fn get_shortcut(
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<String, String> {
    let cache_ref = cache.inner().clone();
    Ok(tokio::task::spawn_blocking(move || {
        cache_ref
            .get_setting("custom_shortcut")
            .ok()
            .flatten()
            .or_else(|| crate::secrets::get_secret("custom_shortcut").ok())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_SHORTCUT.to_string())
    })
    .await
    .unwrap_or_else(|_| DEFAULT_SHORTCUT.to_string()))
}

#[tauri::command]
#[specta::specta]
pub async fn set_shortcut(
    shortcut: String,
    app: tauri::AppHandle,
    selection_ctx: tauri::State<'_, Arc<SelectionContext>>,
    pending_slot: tauri::State<'_, Arc<PendingFloatRequests>>,
    shortcut_slot: tauri::State<'_, Arc<ShortcutStatusSlot>>,
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    let clean = shortcut.trim();
    if clean.is_empty() {
        return Err("快捷键不能为空".to_string());
    }

    // 读取当前生效的旧快捷键（优先读取 SQLite，旧版回退 Keyring）
    let cache_read = cache.inner().clone();
    let old_shortcut = tokio::task::spawn_blocking(move || {
        cache_read
            .get_setting("custom_shortcut")
            .ok()
            .flatten()
            .or_else(|| crate::secrets::get_secret("custom_shortcut").ok())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_SHORTCUT.to_string())
    })
    .await
    .unwrap_or_else(|_| DEFAULT_SHORTCUT.to_string());

    // 尝试注销本功能先前注册的旧快捷键（绝不注销 clean，避免静默抢占其他功能的快捷键，见 LESSONS 第 24 条）
    let _ = app.global_shortcut().unregister(old_shortcut.as_str());

    // 立即注册新快捷键
    let app_h = app.clone();
    let sel_flag = selection_ctx.sel_flag.clone();
    let counter = selection_ctx.counter.clone();
    let slot = pending_slot.inner().clone();
    let register_res = app
        .global_shortcut()
        .on_shortcut(clean, move |_app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                trigger_selection_translation(
                    app_h.clone(),
                    sel_flag.clone(),
                    counter.clone(),
                    slot.clone(),
                );
            }
        });

    if let Err(e) = register_res {
        // 注册失败时原子回滚：重新注册旧快捷键，防止快捷键状态永久丢失
        if old_shortcut != clean {
            let rollback_h = app.clone();
            let rollback_sel_flag = selection_ctx.sel_flag.clone();
            let rollback_counter = selection_ctx.counter.clone();
            let rollback_slot = pending_slot.inner().clone();
            let _ = app.global_shortcut().on_shortcut(
                old_shortcut.as_str(),
                move |_app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        trigger_selection_translation(
                            rollback_h.clone(),
                            rollback_sel_flag.clone(),
                            rollback_counter.clone(),
                            rollback_slot.clone(),
                        );
                    }
                },
            );
        }
        let err_msg = format!("快捷键注册失败，可能按键格式不合法或被系统占用: {e}");
        shortcut_slot.set_translate_failed(err_msg.clone());
        return Err(err_msg);
    }

    // 持久化存储新快捷键到 SQLite 设置表
    let to_save = clean.to_string();
    let cache_write = cache.inner().clone();
    let persist_res = tokio::task::spawn_blocking(move || {
        let _ = crate::secrets::delete_secret("custom_shortcut");
        cache_write.set_setting("custom_shortcut", &to_save)
    })
    .await
    .map_err(|e| format!("快捷键调度异常: {e}"))?;

    if let Err(e) = persist_res {
        // 回滚：注销刚注册的新快捷键，恢复旧快捷键
        let _ = app.global_shortcut().unregister(clean);
        if old_shortcut != clean {
            let rollback_h = app.clone();
            let rollback_sel_flag = selection_ctx.sel_flag.clone();
            let rollback_counter = selection_ctx.counter.clone();
            let rollback_slot = pending_slot.inner().clone();
            let _ = app.global_shortcut().on_shortcut(
                old_shortcut.as_str(),
                move |_app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        trigger_selection_translation(
                            rollback_h.clone(),
                            rollback_sel_flag.clone(),
                            rollback_counter.clone(),
                            rollback_slot.clone(),
                        );
                    }
                },
            );
        }
        let err_msg = format!("保存快捷键失败（已回滚到原快捷键）: {e}");
        shortcut_slot.set_translate_failed(err_msg.clone());
        return Err(err_msg);
    }

    shortcut_slot.set_translate_ok();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_replace_shortcut(
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<String, String> {
    let cache_ref = cache.inner().clone();
    Ok(tokio::task::spawn_blocking(move || {
        cache_ref
            .get_setting("custom_replace_shortcut")
            .ok()
            .flatten()
            .or_else(|| crate::secrets::get_secret("custom_replace_shortcut").ok())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_REPLACE_SHORTCUT.to_string())
    })
    .await
    .unwrap_or_else(|_| DEFAULT_REPLACE_SHORTCUT.to_string()))
}

#[tauri::command]
#[specta::specta]
pub async fn set_replace_shortcut(
    shortcut: String,
    app: tauri::AppHandle,
    selection_ctx: tauri::State<'_, Arc<SelectionContext>>,
    deps: tauri::State<'_, Arc<TranslationDependencies>>,
    shortcut_slot: tauri::State<'_, Arc<ShortcutStatusSlot>>,
) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    let clean = shortcut.trim();
    if clean.is_empty() {
        return Err("快捷键不能为空".to_string());
    }

    let cache_ref = deps.cache.clone();
    let old_shortcut = tokio::task::spawn_blocking(move || {
        cache_ref
            .get_setting("custom_replace_shortcut")
            .ok()
            .flatten()
            .or_else(|| crate::secrets::get_secret("custom_replace_shortcut").ok())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_REPLACE_SHORTCUT.to_string())
    })
    .await
    .unwrap_or_else(|_| DEFAULT_REPLACE_SHORTCUT.to_string());

    // 尝试注销本功能先前注册的旧快捷键（绝不注销 clean，避免静默抢占）
    let _ = app.global_shortcut().unregister(old_shortcut.as_str());

    let app_h = app.clone();
    let sel_flag = selection_ctx.sel_flag.clone();
    let deps_ref = deps.inner().clone();

    let register_res = app
        .global_shortcut()
        .on_shortcut(clean, move |_app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                trigger_selection_replacement(app_h.clone(), sel_flag.clone(), deps_ref.clone());
            }
        });

    if let Err(e) = register_res {
        // 注册失败时原子回滚：重新注册旧快捷键，防止快捷键状态永久丢失
        if old_shortcut != clean {
            let rollback_h = app.clone();
            let rollback_sel_flag = selection_ctx.sel_flag.clone();
            let rollback_deps = deps.inner().clone();
            let _ = app.global_shortcut().on_shortcut(
                old_shortcut.as_str(),
                move |_app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        trigger_selection_replacement(
                            rollback_h.clone(),
                            rollback_sel_flag.clone(),
                            rollback_deps.clone(),
                        );
                    }
                },
            );
        }
        let err_msg = format!("划词替换快捷键注册失败，可能按键格式不合法或被系统占用: {e}");
        shortcut_slot.set_replace_failed(err_msg.clone());
        return Err(err_msg);
    }

    let to_save = clean.to_string();
    let cache_write = deps.cache.clone();
    let persist_res = tokio::task::spawn_blocking(move || {
        let _ = crate::secrets::delete_secret("custom_replace_shortcut");
        cache_write.set_setting("custom_replace_shortcut", &to_save)
    })
    .await
    .map_err(|e| format!("快捷键调度异常: {e}"))?;

    // 与 set_shortcut 同理：注册成功但持久化失败时必须对称回滚，
    // 否则进程行为与 keyring 配置不一致，重启后快捷键回退。
    if let Err(e) = persist_res {
        let _ = app.global_shortcut().unregister(clean);
        if old_shortcut != clean {
            let rollback_h = app.clone();
            let rollback_sel_flag = selection_ctx.sel_flag.clone();
            let rollback_deps = deps.inner().clone();
            let _ = app.global_shortcut().on_shortcut(
                old_shortcut.as_str(),
                move |_app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        trigger_selection_replacement(
                            rollback_h.clone(),
                            rollback_sel_flag.clone(),
                            rollback_deps.clone(),
                        );
                    }
                },
            );
        }
        let err_msg = format!("保存快捷键失败（已回滚到原快捷键）: {e}");
        shortcut_slot.set_replace_failed(err_msg.clone());
        return Err(err_msg);
    }

    shortcut_slot.set_replace_ok();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn detect_text_language(text: String) -> Result<Option<String>, String> {
    Ok(transly_core::detect_language(&text).map(|s| s.to_string()))
}
