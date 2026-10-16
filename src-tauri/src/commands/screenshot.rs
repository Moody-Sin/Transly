//! 截图翻译的 IPC 命令薄封装。
//!
//! 业务逻辑全部在 `crate::overlay`（窗口生命周期）与 `transly-core`（纯逻辑、可单测）；
//! 本模块只负责读取用户偏好、转发到 overlay、转换错误类型。

use std::sync::Arc;

use tauri::AppHandle;
use transly_core::{NormRect, OcrBlockDto, OcrErrorDto, ScreenshotSessionDto};

use crate::commands::TranslationDependencies;

/// 读取截图会话的初始目标语言与引擎偏好。
async fn load_prefs(deps: Arc<TranslationDependencies>) -> (String, String) {
    tokio::task::spawn_blocking(move || {
        let engine = deps
            .cache
            .get_setting("preferred_replace_engine")
            .ok()
            .flatten()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "google".to_string());
        // 主窗口的目标语言只存在前端本地状态、未持久化，故截图覆盖窗默认中文；
        // 阶段 4 引入的覆盖窗语言下拉框可在会话内覆盖它。
        ("zh-CN".to_string(), engine)
    })
    .await
    .unwrap_or_else(|_| ("zh-CN".to_string(), "google".to_string()))
}

/// 触发一次截图翻译：选光标所在显示器、捕获冻结帧并弹出覆盖窗。
#[tauri::command]
#[specta::specta]
pub async fn start_screenshot_translation(
    app: AppHandle,
    deps: tauri::State<'_, Arc<TranslationDependencies>>,
) -> Result<ScreenshotSessionDto, OcrErrorDto> {
    let (target_lang, engine) = load_prefs(deps.inner().clone()).await;
    crate::overlay::start(&app, target_lang, engine).await
}

/// 读取当前截图会话。幂等读，供覆盖窗挂载（含 StrictMode 二次执行）使用。
#[tauri::command]
#[specta::specta]
pub async fn get_screenshot_session(app: AppHandle) -> Result<ScreenshotSessionDto, OcrErrorDto> {
    crate::overlay::read_session(&app)
}

/// 提交框选区域。阶段 2 返回空块（OCR 在阶段 3），仅完成选区钳制与代际校验。
#[tauri::command]
#[specta::specta]
pub async fn recognize_screenshot_selection(
    app: AppHandle,
    rect: NormRect,
    generation: u32,
) -> Result<Vec<OcrBlockDto>, OcrErrorDto> {
    crate::overlay::recognize(&app, rect, generation)
}

/// 取消并关闭覆盖窗。
#[tauri::command]
#[specta::specta]
pub async fn cancel_screenshot_translation(app: AppHandle) -> Result<(), String> {
    crate::overlay::cancel(&app);
    Ok(())
}
