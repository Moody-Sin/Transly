//! 快捷键注册状态的聚合视图
//!
//! 由 `commands/mod.rs` 拆分而来：命令函数保持在 crate 内的原路径不变
//! （`mod.rs` 统一 `pub use` 重导出），因此 `app_command_builder!` 与
//! `src/bindings.ts` 的生成结果不受影响。

use std::sync::Arc;

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
