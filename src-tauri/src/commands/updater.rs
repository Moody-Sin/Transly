//! 应用内检查与安装更新。
//!
//! 前端不直接调用 `tauri-plugin-updater` 的 IPC，而是走这里的强类型命令：
//! 更新地址与签名公钥固定在 `tauri.conf.json`，前端只表达“检查”与“安装”意图。

use tauri_plugin_updater::UpdaterExt;

/// 可更新版本信息（仅暴露 UI 需要的字段）
#[derive(serde::Serialize, serde::Deserialize, specta::Type)]
pub struct UpdateInfoDto {
    pub version: String,
    /// 发布说明（来自 latest.json 的 notes 字段）
    pub notes: Option<String>,
}

fn updater_error(err: impl std::fmt::Display) -> String {
    format!("更新服务不可用：{err}")
}

/// 查询是否有新版本；无更新时返回 `None`。
#[tauri::command]
#[specta::specta]
pub async fn check_for_update(app: tauri::AppHandle) -> Result<Option<UpdateInfoDto>, String> {
    let updater = app.updater().map_err(updater_error)?;
    let update = updater.check().await.map_err(updater_error)?;
    Ok(update.map(|u| UpdateInfoDto {
        version: u.version,
        notes: u.body,
    }))
}

/// 下载并安装当前可用更新，安装完成后重启应用。
#[tauri::command]
#[specta::specta]
pub async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(updater_error)?;
    let Some(update) = updater.check().await.map_err(updater_error)? else {
        return Err("当前已是最新版本".to_string());
    };
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| format!("更新安装失败：{e}"))?;
    app.restart();
}
