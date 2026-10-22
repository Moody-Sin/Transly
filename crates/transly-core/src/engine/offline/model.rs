//! 本地外部离线模型探测与状态管理
//!
//! 支持自动发现用户部署在 `<app_data_dir>/models/` 下的离线翻译模型文件（.onnx / .bin / .gguf）。
//! 当检测到外部模型时激活增强模型模式；当未检测到时，优雅降级为内置词库与短语表引擎，绝不报错或崩溃。

use std::path::Path;

/// 离线引擎运行状态与模型配置 DTO
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct OfflineEngineStatusDto {
    /// 内置词库词条总数
    pub lexicon_entries_count: u32,
    /// 是否加载/检测到了外部本地模型
    pub has_local_model: bool,
    /// 外部模型文件名（若检测到）
    pub local_model_name: Option<String>,
    /// 本地模型扫描目录路径
    pub model_dir: String,
}

/// 支持的外部模型后缀白名单
const SUPPORTED_MODEL_EXTENSIONS: &[&str] = &["onnx", "bin", "gguf", "model"];

/// 扫描指定目录探测是否存在可用的外部离线模型文件
pub fn inspect_local_models(models_dir: &Path) -> (bool, Option<String>) {
    if !models_dir.exists() || !models_dir.is_dir() {
        return (false, None);
    }

    let entries = match std::fs::read_dir(models_dir) {
        Ok(e) => e,
        Err(_) => return (false, None),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                if SUPPORTED_MODEL_EXTENSIONS
                    .iter()
                    .any(|&e| e.eq_ignore_ascii_case(ext))
                {
                    let filename = path
                        .file_name()
                        .and_then(|f| f.to_str())
                        .map(|s| s.to_string());
                    return (true, filename);
                }
            }
        }
    }

    (false, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inspect_local_models_non_existent_dir() {
        let (has_model, name) = inspect_local_models(Path::new("/path/does/not/exist"));
        assert!(!has_model);
        assert!(name.is_none());
    }

    #[test]
    fn test_inspect_local_models_finds_valid_model() {
        let temp_dir =
            std::env::temp_dir().join(format!("transly_model_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let model_file = temp_dir.join("opus-mt-en-zh.onnx");
        let _ = std::fs::write(&model_file, b"dummy onnx content");

        let (has_model, name) = inspect_local_models(&temp_dir);
        assert!(has_model);
        assert_eq!(name.as_deref(), Some("opus-mt-en-zh.onnx"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
