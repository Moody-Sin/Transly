//! 截图翻译的 IPC 命令薄封装。
//!
//! 业务逻辑全部在 `crate::overlay`（窗口生命周期）与 `transly-core`（纯逻辑、可单测）；
//! 本模块只负责读取用户偏好、转发到 overlay、转换错误类型。

use std::sync::Arc;

use tauri::AppHandle;
use transly_core::{NormRect, OcrBlockDto, OcrErrorDto, ScreenshotSessionDto};

use crate::commands::TranslationDependencies;

/// 默认截图翻译快捷键。
///
/// 与另两个默认键（`Alt+Shift+D` 划词浮窗、`Alt+Shift+R` 原地替换）保持同一风格，
/// 且 `S` = Screenshot。
pub const DEFAULT_SCREENSHOT_SHORTCUT: &str = "Alt+Shift+S";

/// 截图翻译的可用性探测结果。
///
/// 供 UI 在**真正尝试之前**就告知用户环境是否支持，避免用户按了快捷键
/// 却只得到一个失败提示。探测只回答「理论上能不能」，**不真的去捕获屏幕**
/// （那会闪一下且开销大，且权限类问题只能在捕获时才能确定）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ScreenshotCapabilityDto {
    /// 当前环境是否支持框选截图
    pub supported: bool,
    /// 不支持时的**可执行**原因（直接展示给用户，UI 不要自己再拼文案）
    pub reason: Option<String>,
    /// OCR 后端是否可用（Linux 依赖外部 tesseract，Windows/macOS 为系统原生）
    pub ocr_available: bool,
}

/// 探测当前环境的截图翻译能力。
#[tauri::command]
#[specta::specta]
pub async fn get_screenshot_capability() -> ScreenshotCapabilityDto {
    tauri::async_runtime::spawn_blocking(detect_capability)
        .await
        .unwrap_or_else(|_| ScreenshotCapabilityDto {
            supported: false,
            reason: Some("能力探测任务异常终止".to_string()),
            ocr_available: false,
        })
}

/// 同步探测实现（供命令与测试共用）。
fn detect_capability() -> ScreenshotCapabilityDto {
    // Wayland 判定是纯逻辑，已在 core 中实现并单测；这里只负责读取环境变量。
    let wayland = transly_core::detect_wayland_session(
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
    );

    #[cfg(target_os = "linux")]
    {
        let ocr_available = tesseract_available();
        // 复用既有错误文案，不在 UI 侧另写一份（避免两处漂移）。
        if wayland {
            ScreenshotCapabilityDto {
                supported: false,
                reason: Some(OcrErrorDto::wayland_unsupported().message),
                ocr_available,
            }
        } else {
            ScreenshotCapabilityDto {
                supported: true,
                reason: None,
                ocr_available,
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        // Windows/macOS：系统原生 OCR 恒可用；屏幕权限问题只能在捕获时确定，
        // 不在此拦（否则未授权用户会看到按钮被禁用，而实际上点一下才会弹授权）。
        let _ = wayland;
        ScreenshotCapabilityDto {
            supported: true,
            reason: None,
            ocr_available: true,
        }
    }
}

/// 探测 `tesseract` 是否可执行。
///
/// 用 `--version` 而非 `which`：不引入新依赖，且能同时发现「装了但不可执行」。
#[cfg(target_os = "linux")]
fn tesseract_available() -> bool {
    std::process::Command::new("tesseract")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// 读取截图翻译快捷键（优先 SQLite，旧版回退 Keyring）。
pub async fn load_screenshot_shortcut(deps: &Arc<TranslationDependencies>) -> String {
    let cache = deps.cache.clone();
    tokio::task::spawn_blocking(move || {
        cache
            .get_setting("custom_screenshot_shortcut")
            .ok()
            .flatten()
            .or_else(|| crate::secrets::get_secret("custom_screenshot_shortcut").ok())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_SCREENSHOT_SHORTCUT.to_string())
    })
    .await
    .unwrap_or_else(|_| DEFAULT_SCREENSHOT_SHORTCUT.to_string())
}

/// 由快捷键 / 托盘菜单触发的截图入口。
///
/// 与 `start_screenshot_translation` 命令共用同一条实现路径（取偏好 → `overlay::start`），
/// 不另写一份，避免两处偏好读取逻辑漂移。
///
/// 失败只能留痕：这两个入口没有 UI 承载错误（设置页的能力探测已提前告知
/// 环境是否支持），静默失败至少比 panic 好，但要能在日志里查到原因。
pub fn trigger_screenshot_translation(app: AppHandle, deps: Arc<TranslationDependencies>) {
    tauri::async_runtime::spawn(async move {
        let (target_lang, engine) = load_prefs(deps).await;
        if let Err(e) = crate::overlay::start(&app, target_lang, engine).await {
            eprintln!(
                "[Screenshot] 触发截图翻译失败: {} ({:?})",
                e.message, e.kind
            );
        }
    });
}

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

/// 提交框选区域，返回归一化到整张冻结帧的 OCR 块。
#[tauri::command]
#[specta::specta]
pub async fn recognize_screenshot_selection(
    app: AppHandle,
    rect: NormRect,
    generation: u32,
    target_lang: Option<String>,
) -> Result<Vec<OcrBlockDto>, OcrErrorDto> {
    // OCR 是秒级阻塞操作（解码 PNG + 识别），必须派发到阻塞线程池。
    tauri::async_runtime::spawn_blocking(move || {
        crate::overlay::recognize(&app, rect, generation, target_lang)
    })
    .await
    .map_err(|e| OcrErrorDto::internal(format!("OCR 任务调度异常: {e}")))?
}

/// 读取截图翻译快捷键。
#[tauri::command]
#[specta::specta]
pub async fn get_screenshot_shortcut(
    deps: tauri::State<'_, Arc<TranslationDependencies>>,
) -> Result<String, String> {
    Ok(load_screenshot_shortcut(deps.inner()).await)
}

/// 设置截图翻译快捷键。
///
/// 与既有 `set_shortcut` / `set_replace_shortcut` 同构的四段式：
/// 注销旧键 → 注册新键 → 失败则回滚重注旧键 → 持久化（持久化失败也回滚）。
/// 注册失败必须原子回滚，否则快捷键状态会永久丢失（用户既用不了新键也丢了旧键）。
#[tauri::command]
#[specta::specta]
pub async fn set_screenshot_shortcut(
    shortcut: String,
    app: AppHandle,
    deps: tauri::State<'_, Arc<TranslationDependencies>>,
    shortcut_slot: tauri::State<'_, Arc<crate::commands::ShortcutStatusSlot>>,
) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    let clean = shortcut.trim();
    if clean.is_empty() {
        return Err("快捷键不能为空".to_string());
    }

    let deps_arc = deps.inner().clone();
    let old_shortcut = load_screenshot_shortcut(&deps_arc).await;

    // ⚠️ 只注销**本功能自己上次注册的键**，绝不能注销 `clean`。
    //
    // `unregister(clean)` 看似能避开「未改动快捷键而重复保存」的 AlreadyRegistered，
    // 但它会无条件注销该键——若这个键正被**别的功能**占用（例如用户把截图键设成
    // 划词用的 `Alt+Shift+D`），注销后再注册会**静默成功**，于是划词的快捷键被
    // 悄悄抢走，而 UI 因为 `translate_error` 仍为 null 而毫无告警。
    // 真机实测确证过这个静默抢占。
    //
    // 正确行为：不改动别人的键，让 `on_shortcut` 返回 AlreadyRegistered，
    // 把冲突如实报给用户（这正是设置卡片要展示的东西）。
    let _ = app.global_shortcut().unregister(old_shortcut.as_str());

    // 用闭包收敛「注册到 overlay::start」的重复代码（三处调用同一逻辑）。
    let register = |accel: &str| {
        let app_h = app.clone();
        let deps_h = deps_arc.clone();
        app.global_shortcut()
            .on_shortcut(accel, move |_app, _shortcut, event| {
                if event.state() == ShortcutState::Pressed {
                    trigger_screenshot_translation(app_h.clone(), deps_h.clone());
                }
            })
    };

    if let Err(e) = register(clean) {
        // 原子回滚：重新注册旧键
        if old_shortcut != clean {
            let _ = register(old_shortcut.as_str());
        }
        let err_msg = format!("快捷键注册失败，可能按键格式不合法或被系统占用: {e}");
        shortcut_slot.set_screenshot_failed(err_msg.clone());
        return Err(err_msg);
    }

    let to_save = clean.to_string();
    let cache = deps_arc.cache.clone();
    let persist = tokio::task::spawn_blocking(move || {
        let _ = crate::secrets::delete_secret("custom_screenshot_shortcut");
        cache.set_setting("custom_screenshot_shortcut", &to_save)
    })
    .await
    .map_err(|e| format!("快捷键调度异常: {e}"))?;

    if let Err(e) = persist {
        // 持久化失败也要回滚，否则重启后新旧键都不生效。
        let _ = app.global_shortcut().unregister(clean);
        if old_shortcut != clean {
            let _ = register(old_shortcut.as_str());
        }
        let err_msg = format!("保存快捷键失败（已回滚到原快捷键）: {e}");
        shortcut_slot.set_screenshot_failed(err_msg.clone());
        return Err(err_msg);
    }

    shortcut_slot.set_screenshot_ok();
    Ok(())
}

/// 取消并关闭覆盖窗。
#[tauri::command]
#[specta::specta]
pub async fn cancel_screenshot_translation(app: AppHandle) -> Result<(), String> {
    crate::overlay::cancel(&app);
    Ok(())
}

/// 单个块的翻译结果。
///
/// 字段刻意扁平（specta 禁嵌套 `Option`、禁 `u64`/`i64`）：
/// `failed` 与 `error_kind`/`error_message` 分开表达，前端无需解嵌套。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ScreenshotBlockResultDto {
    /// 与请求下标一一对应，前端据此回填（**不能按数组顺序硬贴**）
    pub index: u32,
    /// 成功时为译文；失败时为空串
    pub text: String,
    pub failed: bool,
    /// 失败时的错误种类，供前端映射 i18n 文案
    pub error_kind: Option<transly_core::ErrorKind>,
    pub error_message: Option<String>,
    /// 实际执行引擎（可能因降级与请求不同）
    pub engine: String,
}

/// 批量翻译截图块。
///
/// # 为什么走 `translate_batch_with_cache`
///
/// 「术语库源端 → 缓存 → 引擎 → 缓存 → 译文端」的顺序已在该模块单测覆盖；
/// 此处若另写一份就是第四份复制，任何顺序修正都会漂移（见该模块文档）。
///
/// # 逐项容错
///
/// 返回**整批级** `Err` 仅用于「引擎不可用」这类全局失败；单个块翻译失败
/// 走 `Vec` 里的 `failed: true`——用户需要能对失败块单独重试。
#[tauri::command]
#[specta::specta]
pub async fn translate_screenshot_blocks(
    texts: Vec<String>,
    target_lang: String,
    engine: Option<String>,
    generation: u32,
    app: AppHandle,
    deps: tauri::State<'_, Arc<TranslationDependencies>>,
) -> Result<Vec<ScreenshotBlockResultDto>, crate::commands::TranslateErrorDto> {
    use crate::commands::TranslateErrorDto;

    // 代际闸门：陈旧窗口（用户已重新截图）的结果不得落到新会话。
    let session_gen = crate::overlay::current_generation(&app).ok_or_else(|| {
        TranslateErrorDto::from_error(&transly_core::Error::Input("截图会话已结束".into()))
    })?;
    if session_gen != generation {
        return Err(TranslateErrorDto::from_error(&transly_core::Error::Input(
            "截图会话已过期，请重新截图".into(),
        )));
    }

    if texts.is_empty() {
        return Ok(Vec::new());
    }
    if let Some(ref eng) = engine {
        if !transly_core::validate_engine_id(eng) {
            return Err(TranslateErrorDto::from_error(&transly_core::Error::Input(
                format!("不支持或未知的翻译引擎: {eng}"),
            )));
        }
    }

    let deps_arc = deps.inner().clone();

    let active_engine = match engine {
        Some(ref name) => deps_arc.pool.get_engine(name).await,
        None => deps_arc.pool.default_engine(),
    };
    let engine_id = active_engine.id().to_string();

    let requests: Vec<transly_core::TranslationRequest> = texts
        .iter()
        .map(|t| transly_core::TranslationRequest {
            text: t.clone(),
            // 截图内容语言未知，交给引擎/离线检测
            source_lang: "auto".to_string(),
            target_lang: target_lang.clone(),
        })
        .collect();

    let (cache_enabled, _gen) = deps_arc.cache_controller.snapshot();
    let glossary = deps_arc.glossary.read().await.clone();

    let outcome = transly_core::translate_batch_with_cache(
        transly_core::BatchPipelineDeps {
            engine: active_engine.as_ref(),
            cache: &deps_arc.cache,
            glossary: &glossary,
            cache_enabled,
        },
        &requests,
    )
    .await;

    let results = outcome
        .results
        .into_iter()
        .enumerate()
        .map(|(idx, r)| match r {
            Ok(resp) => ScreenshotBlockResultDto {
                index: idx as u32,
                text: resp.text,
                failed: false,
                error_kind: None,
                error_message: None,
                engine: resp.engine,
            },
            Err(e) => {
                let dto = TranslateErrorDto::from_error(&e);
                ScreenshotBlockResultDto {
                    index: idx as u32,
                    text: String::new(),
                    failed: true,
                    error_kind: Some(dto.kind),
                    error_message: Some(dto.message),
                    engine: engine_id.clone(),
                }
            }
        })
        .collect::<Vec<_>>();

    Ok(results)
}

/// 把文本（通常是译文）送到主窗口编辑重译，并关闭覆盖窗。
///
/// 这是「OCR 结果可编辑」被砍掉后的替代路径（全局设计 §十二）。
#[tauri::command]
#[specta::specta]
pub async fn push_screenshot_text_to_main(app: AppHandle, text: String) -> Result<(), String> {
    use tauri::{Emitter, Manager};

    #[derive(Clone, serde::Serialize)]
    struct Payload {
        text: String,
    }

    app.emit_to("main", "transly://screenshot-to-main", Payload { text })
        .map_err(|e| format!("发送文本到主窗口失败: {e}"))?;

    if let Some(main) = app.get_webview_window("main") {
        let _ = main.show();
        let _ = main.set_focus();
    }
    crate::overlay::cancel(&app);
    Ok(())
}
