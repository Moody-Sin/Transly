//! 本地服务 Token、配对与服务状态
//!
//! 由 `commands/mod.rs` 拆分而来：命令函数保持在 crate 内的原路径不变
//! （`mod.rs` 统一 `pub use` 重导出），因此 `app_command_builder!` 与
//! `src/bindings.ts` 的生成结果不受影响。

use std::sync::Arc;

use tokio::sync::RwLock;

#[tauri::command]
#[specta::specta]
pub async fn get_server_token() -> Result<String, String> {
    // keyring 读取（未命中时还会生成并写入）为同步阻塞 I/O，禁止占用主线程
    tokio::task::spawn_blocking(crate::secrets::get_or_create_server_token)
        .await
        .map_err(|e| format!("Token 读取任务调度异常: {e}"))?
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn rotate_server_token(
    current_token: tauri::State<'_, Arc<RwLock<String>>>,
) -> Result<String, String> {
    // keyring 写入为同步阻塞 I/O，禁止占用 async 运行时工作线程
    let new_token = tokio::task::spawn_blocking(crate::secrets::rotate_server_token)
        .await
        .map_err(|e| format!("Token 轮换任务调度异常: {e}"))?
        .map_err(|e| e.to_string())?;
    let mut lock = current_token.write().await;
    *lock = new_token.clone();
    Ok(new_token)
}

#[tauri::command]
#[specta::specta]
pub async fn approve_pairing(
    pairing_mgr: tauri::State<'_, Arc<crate::server::pairing::PairingManager>>,
    session_id: String,
) -> Result<(), String> {
    if pairing_mgr.approve(&session_id).await {
        Ok(())
    } else {
        Err("配对会话不存在或已过期".to_string())
    }
}

#[tauri::command]
#[specta::specta]
pub async fn reject_pairing(
    pairing_mgr: tauri::State<'_, Arc<crate::server::pairing::PairingManager>>,
    session_id: String,
) -> Result<(), String> {
    if pairing_mgr.reject(&session_id).await {
        Ok(())
    } else {
        Err("配对会话不存在或已过期".to_string())
    }
}

/// 本地 HTTP 服务（供浏览器插件调用）的运行状态。
///
/// 服务在后台异步启动，失败只进 stderr 的话用户完全无感：
/// 扩展端会显示“桌面端未运行”，而桌面端毫无提示。此处把状态暴露给前端。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ServerStatus {
    /// 尚未启动完成（setup 中的异步任务还在进行）
    Starting,
    /// 已就绪
    Running,
    /// 未能启动，附原因（如端口被占用、系统凭据不可用）
    Failed { reason: String },
}

/// 本地服务状态槽位。独立新类型：Tauri 按 TypeId 存 state。
#[derive(Debug, Default)]
pub struct ServerStatusSlot(pub std::sync::Mutex<Option<ServerStatus>>);

impl ServerStatusSlot {
    pub fn set(&self, status: ServerStatus) {
        if let Ok(mut slot) = self.0.lock() {
            *slot = Some(status);
        }
    }

    pub fn get(&self) -> ServerStatus {
        self.0
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
            .unwrap_or(ServerStatus::Starting)
    }
}

#[tauri::command]
#[specta::specta]
pub fn get_server_status(slot: tauri::State<'_, Arc<ServerStatusSlot>>) -> ServerStatus {
    slot.get()
}
