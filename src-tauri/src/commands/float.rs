//! 浮窗请求队列与粘滞状态
//!
//! 由 `commands/mod.rs` 拆分而来：命令函数保持在 crate 内的原路径不变
//! （`mod.rs` 统一 `pub use` 重导出），因此 `app_command_builder!` 与
//! `src/bindings.ts` 的生成结果不受影响。

use std::sync::Arc;

/// 浮窗翻译请求负载：携带来源区分与请求 ID，供前端去重与提示
#[derive(Clone, serde::Serialize)]
pub struct FloatRequest {
    pub text: String,
    /// "selection" = 全局快捷键划词；"clipboard" = 系统剪贴板监听
    pub source: &'static str,
    /// 宿主单调递增；同一 ID 只翻译一次
    pub request_id: u64,
}

/// 暂存待处理的浮窗请求队列。
///
/// 浮窗 `visible: false` 时 Tauri 仍创建 webview，但页面 JS 尚未加载完成；
/// 此时 `emit` 的事件无人接收且不会重放。因此宿主先暂存，浮窗就绪后主动拉取，
/// 从根上消除 emit 时序竞态。
///
/// 必须是**队列**而不是单槽：划词（快捷键）与剪贴板监听是两条独立链路，
/// 共用单槽时先到的请求会被后到的覆盖，用户表现为“第一次划词没反应”。
/// 容量设上限并在超出时丢弃最旧项，防止浮窗长期不可用时无界增长。
#[derive(Default)]
pub struct PendingFloatRequests(pub std::sync::Mutex<std::collections::VecDeque<FloatRequest>>);

/// 待处理队列容量上限。正常交互下浮窗会在毫秒级就绪，超出此值说明浮窗已不可用。
pub(crate) const PENDING_FLOAT_CAPACITY: usize = 16;

impl PendingFloatRequests {
    /// 入队一条请求；超出容量时丢弃最旧项并返回 true 表示发生了丢弃。
    pub fn push(&self, req: FloatRequest) -> bool {
        let Ok(mut queue) = self.0.lock() else {
            return false;
        };
        let mut dropped = false;
        while queue.len() >= PENDING_FLOAT_CAPACITY {
            queue.pop_front();
            dropped = true;
        }
        queue.push_back(req);
        dropped
    }
}

/// 浮窗粘滞标志。必须用独立新类型：Tauri 按 TypeId 存 state，
/// 若与 `clipboard_monitor_enabled` 同为 `Arc<AtomicBool>` 会 panic（重复 manage）
#[derive(Debug, Default)]
pub struct FloatSticky(pub std::sync::atomic::AtomicBool);

/// 设置浮窗粘滞：翻译中/错误态时为 true，浮窗不失焦自动隐藏，
/// 保证错误提示与重试按钮来得及被用户看到并点击
#[tauri::command]
#[specta::specta]
pub fn set_float_sticky(
    enabled: bool,
    sticky: tauri::State<'_, Arc<FloatSticky>>,
) -> Result<(), String> {
    sticky.0.store(enabled, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

/// 浮窗拉取待处理请求。`has_request: false` 表示无待处理请求。
/// 用扁平字段而非 `Option<FloatRequest>`：specta 不支持嵌套 Option 作为 IPC 返回类型。
#[tauri::command]
#[specta::specta]
pub fn take_pending_float_request(
    pending: tauri::State<'_, Arc<PendingFloatRequests>>,
) -> Result<PendingFloatResponse, String> {
    let mut queue = pending
        .0
        .lock()
        .map_err(|e| format!("读取待处理浮窗请求失败: {e}"))?;
    Ok(match queue.pop_front() {
        Some(req) => PendingFloatResponse {
            has_request: true,
            text: req.text,
            source: req.source.to_string(),
            request_id: req.request_id as u32,
        },
        None => PendingFloatResponse {
            has_request: false,
            text: String::new(),
            source: String::new(),
            request_id: 0,
        },
    })
}

#[derive(serde::Serialize, serde::Deserialize, specta::Type)]
pub struct PendingFloatResponse {
    pub has_request: bool,
    pub text: String,
    pub source: String,
    /// u32 而非 u64：specta 禁止导出 BigInt 类型以避免 JS 精度丢失
    pub request_id: u32,
}
