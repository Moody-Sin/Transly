//! 翻译历史与缓存命令
//!
//! 由 `commands/mod.rs` 拆分而来：命令函数保持在 crate 内的原路径不变
//! （`mod.rs` 统一 `pub use` 重导出），因此 `app_command_builder!` 与
//! `src/bindings.ts` 的生成结果不受影响。

use std::sync::Arc;

use transly_core::{CacheEntry, TranslationCache};

#[tauri::command]
#[specta::specta]
pub async fn clear_cache(
    cache: tauri::State<'_, Arc<TranslationCache>>,
    cache_controller: tauri::State<'_, Arc<crate::cache_controller::CacheController>>,
) -> Result<u32, String> {
    // 必须走 CacheController 以递增 generation：否则在途请求会把刚清掉的记录写回。
    // DELETE 全表在 10 万条缓存下是长事务，必须移出主线程避免 UI 硬卡
    let cache_ref = cache.inner().clone();
    let controller = cache_controller.inner().clone();
    tokio::task::spawn_blocking(move || {
        controller
            .clear_only(&cache_ref)
            .map(|n| n as u32)
            .map_err(|e| e.to_string())
    })
    .await
    .unwrap_or_else(|e| Err(format!("缓存清理任务调度异常: {e}")))
}

/// 分页读取翻译历史（最近优先）。`limit` / `offset` 由前端控制。
#[tauri::command]
#[specta::specta]
pub async fn get_translation_history(
    limit: u32,
    offset: u32,
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<Vec<CacheEntry>, String> {
    // SQLite 同步 I/O 禁止占用 async 工作线程
    let cache_ref = cache.inner().clone();
    tokio::task::spawn_blocking(move || {
        cache_ref
            .list_recent(limit, offset)
            .map_err(|e| e.to_string())
    })
    .await
    .unwrap_or_else(|e| Err(format!("翻译历史读取任务调度异常: {e}")))
}

/// 根据关键词检索翻译历史（支持模糊匹配原文与译文）
#[tauri::command]
#[specta::specta]
pub async fn search_translation_history(
    keyword: String,
    limit: u32,
    offset: u32,
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<Vec<CacheEntry>, String> {
    let cache_ref = cache.inner().clone();
    tokio::task::spawn_blocking(move || {
        cache_ref
            .search(&keyword, limit, offset)
            .map_err(|e| e.to_string())
    })
    .await
    .unwrap_or_else(|e| Err(format!("翻译历史搜索任务调度异常: {e}")))
}

/// 按 `cache_key` 删除单条翻译历史。
#[tauri::command]
#[specta::specta]
pub async fn delete_history_entry(
    cache_key: String,
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<bool, String> {
    // SQLite 同步 I/O 禁止占用 async 工作线程
    let cache_ref = cache.inner().clone();
    tokio::task::spawn_blocking(move || {
        cache_ref
            .delete_entry(&cache_key)
            .map_err(|e| e.to_string())
    })
    .await
    .unwrap_or_else(|e| Err(format!("翻译历史删除任务调度异常: {e}")))
}

#[derive(serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ExportHistoryOutcome {
    pub count: u32,
    pub file_path: String,
}

fn resolve_downloads_dir() -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(user) = std::env::var_os("USERPROFILE") {
            let p = std::path::PathBuf::from(user).join("Downloads");
            if p.is_dir() {
                return p;
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Some(home) = std::env::var_os("HOME") {
            let p = std::path::PathBuf::from(home).join("Downloads");
            if p.is_dir() {
                return p;
            }
        }
    }
    std::env::temp_dir()
}

/// 流式将翻译历史记录导出为本地文件（写入用户 Downloads 目录），零大内存 IPC 传输与 UI 阻塞
#[tauri::command]
#[specta::specta]
pub async fn export_translation_history(
    format: String,
    keyword: Option<String>,
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<ExportHistoryOutcome, String> {
    let cache_ref = cache.inner().clone();
    tokio::task::spawn_blocking(move || -> Result<ExportHistoryOutcome, String> {
        let downloads_dir = resolve_downloads_dir();
        let ext = if format.eq_ignore_ascii_case("csv") {
            "csv"
        } else {
            "json"
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let filename = format!("transly-history-{now}.{ext}");
        let file_path = downloads_dir.join(&filename);
        let file = std::fs::File::create(&file_path)
            .map_err(|e| format!("创建导出文件失败 ({file_path:?}): {e}"))?;
        let mut writer = std::io::BufWriter::new(file);
        let count = cache_ref
            .export_history_to_writer(&mut writer, ext, keyword.as_deref())
            .map_err(|e| e.to_string())?;

        Ok(ExportHistoryOutcome {
            count: count as u32,
            file_path: file_path.to_string_lossy().to_string(),
        })
    })
    .await
    .map_err(|e| format!("导出任务调度异常: {e}"))?
}

#[tauri::command]
#[specta::specta]
pub async fn set_cache_enabled(
    enabled: bool,
    controller: tauri::State<'_, Arc<crate::cache_controller::CacheController>>,
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<(), String> {
    let cache_setting_ref = cache.inner().clone();
    let val = if enabled { "true" } else { "false" };
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        cache_setting_ref
            .set_setting("cache_enabled", val)
            .map_err(|e| anyhow::anyhow!("配置保存失败: {e}"))?;
        let _ = crate::secrets::delete_secret("cache_enabled");
        Ok(())
    })
    .await
    .map_err(|e| format!("配置写入任务调度异常: {e}"))?
    .map_err(|e| e.to_string())?;

    if enabled {
        controller.enable();
        Ok(())
    } else {
        // 在同一门禁锁内递增 generation 并清空旧磁盘缓存，彻底原子闭环；
        // 清空为 SQLite 长事务，同样移出主线程
        let controller_ref = controller.inner().clone();
        let cache_ref = cache.inner().clone();
        tokio::task::spawn_blocking(move || controller_ref.disable_and_clear(&cache_ref))
            .await
            .map_err(|e| format!("缓存清空任务调度异常: {e}"))?
            .map(|_| ())
            .map_err(|e| format!("已禁用缓存，但清空历史缓存记录失败: {e}"))
    }
}

#[tauri::command]
#[specta::specta]
pub fn get_cache_enabled(
    controller: tauri::State<'_, Arc<crate::cache_controller::CacheController>>,
) -> bool {
    controller.is_enabled()
}
