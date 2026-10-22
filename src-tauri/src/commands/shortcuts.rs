//! 全局快捷键注册与状态查询、源语言检测。

use super::*;

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

/// 全局快捷键的注册状态。
///
/// `Alt+Shift+D` / `Alt+Shift+R` / `Alt+Shift+S` 是极易被其他软件占用的组合，
/// 注册失败（被占用 / 格式非法 / 权限不足）属于高频场景。若把结果直接丢弃，
/// 用户只会看到「功能莫名其妙不工作」且没有任何线索。因此必须把结果暴露出来。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ShortcutStatusDto {
    /// 三个快捷键是否都注册成功
    pub ok: bool,
    /// 聚合失败原因（兼容旧字段，全部成功时为 None）
    pub reason: Option<String>,
    /// 划词翻译快捷键注册失败原因（注册成功时为 None）
    pub translate_error: Option<String>,
    /// 划词原地替换快捷键注册失败原因（注册成功时为 None）
    pub replace_error: Option<String>,
    /// 截图翻译快捷键注册失败原因（注册成功时为 None）
    pub screenshot_error: Option<String>,
}

/// 快捷键注册状态槽位。与 `ServerStatusSlot` 同构（Tauri 按 TypeId 存 state）。
/// 分离维护三个快捷键的错误状态，避免它们相互覆盖。
#[derive(Debug, Default)]
pub struct ShortcutStatusSlot {
    translate_error: std::sync::Mutex<Option<String>>,
    replace_error: std::sync::Mutex<Option<String>>,
    screenshot_error: std::sync::Mutex<Option<String>>,
}

impl ShortcutStatusSlot {
    pub fn set_translate_ok(&self) {
        if let Ok(mut slot) = self.translate_error.lock() {
            *slot = None;
        }
    }

    pub fn set_translate_failed(&self, reason: String) {
        if let Ok(mut slot) = self.translate_error.lock() {
            *slot = Some(reason);
        }
    }

    pub fn set_replace_ok(&self) {
        if let Ok(mut slot) = self.replace_error.lock() {
            *slot = None;
        }
    }

    pub fn set_replace_failed(&self, reason: String) {
        if let Ok(mut slot) = self.replace_error.lock() {
            *slot = Some(reason);
        }
    }

    pub fn set_screenshot_ok(&self) {
        if let Ok(mut slot) = self.screenshot_error.lock() {
            *slot = None;
        }
    }

    pub fn set_screenshot_failed(&self, reason: String) {
        if let Ok(mut slot) = self.screenshot_error.lock() {
            *slot = Some(reason);
        }
    }

    pub fn get(&self) -> ShortcutStatusDto {
        let t_err = self.translate_error.lock().ok().and_then(|g| g.clone());
        let r_err = self.replace_error.lock().ok().and_then(|g| g.clone());
        let s_err = self.screenshot_error.lock().ok().and_then(|g| g.clone());

        // 用数组聚合而非两两 match：槽位从 2 个加到 3 个时，两两 match 会漏掉
        // 新增的那个（这正是本阶段踩过的坑）。
        let errors: Vec<&String> = [&t_err, &r_err, &s_err].into_iter().flatten().collect();
        let reason = if errors.is_empty() {
            None
        } else {
            Some(
                errors
                    .iter()
                    .map(|e| e.as_str())
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        };

        ShortcutStatusDto {
            ok: reason.is_none(),
            reason,
            translate_error: t_err,
            replace_error: r_err,
            screenshot_error: s_err,
        }
    }
}

#[tauri::command]
#[specta::specta]
pub fn get_shortcut_status(slot: tauri::State<'_, Arc<ShortcutStatusSlot>>) -> ShortcutStatusDto {
    slot.get()
}
