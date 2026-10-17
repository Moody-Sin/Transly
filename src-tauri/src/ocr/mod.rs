//! OCR 平台分发层。
//!
//! 三个平台各有一份 `recognize_words` 实现，统一返回**图内像素坐标**的词级框；
//! 词 → 行 → 段 → 归一化块的聚合全部下沉在 `transly-core`（纯逻辑、可单测）。
//!
//! **必须是三段式 `cfg`**（`windows` / `macos` / `not(any(...))`）：
//! 若只写 `#[cfg(not(target_os = "windows"))]`，Linux 专用的 tesseract 代码
//! 会被编进 macOS。平台专属 `use` 同样必须门控，否则非目标平台在
//! `-D warnings` 下会因 unused import 编译失败。

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::recognize_words;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::recognize_words;

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod linux;
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub use linux::recognize_words;

use transly_core::{words_to_blocks, OcrBlockDto, OcrErrorDto};

/// 对整张图做 OCR，直接返回送往前端的归一化块。
///
/// 这是三平台共用的收口点：平台层只产出词级框，聚合与归一化在 core。
/// **同步阻塞**（秒级），调用方必须用 `spawn_blocking`。
pub fn recognize_blocks(
    image: &image::RgbaImage,
    target_lang: &str,
) -> Result<Vec<OcrBlockDto>, OcrErrorDto> {
    let words = recognize_words(image, target_lang)?;
    if words.is_empty() {
        // 引擎可用但图里没有文字：必须报 `empty_text` 而不是返回空表，
        // 否则前端「什么也没发生」，用户无法区分「没文字」与「功能坏了」。
        return Err(OcrErrorDto::empty_text());
    }
    let blocks = words_to_blocks(&words, image.width(), image.height());
    if blocks.is_empty() {
        // 有词但全被过滤（低置信度噪点等）同样属于「没识别到有效文字」。
        return Err(OcrErrorDto::empty_text());
    }
    Ok(blocks)
}

/// 识别剪贴板图片，返回拼接后的纯文本（既有功能的入口，保持原行为）。
///
/// 改造前 Windows 走 WinRT、其余平台返回 `unsupported_platform`；
/// 现在三平台统一走 [`recognize_blocks`]，故 Linux/macOS 的剪贴板 OCR 也随之可用。
/// **这不是功能扩展而是修正**：`unsupported_platform` 在 Linux 的文案是
/// 「……请改用『识别剪贴板图片』」——若剪贴板路径本身不可用，这句指引就是错的。
pub fn blocks_to_text(blocks: &[OcrBlockDto]) -> String {
    blocks
        .iter()
        .map(|b| b.text.trim())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// 识别剪贴板图片，返回拼接后的纯文本。
///
/// 读取剪贴板位图必须在后台线程（平台剪贴板 API 在主线程可能死锁），
/// OCR 本身也是秒级阻塞操作，故整体 `spawn_blocking`。
pub async fn recognize_clipboard_image(
    app: &tauri::AppHandle,
    target_lang: &str,
) -> Result<String, OcrErrorDto> {
    use tauri_plugin_clipboard_manager::ClipboardExt;

    let app_handle = app.clone();
    let target_lang = target_lang.to_string();

    tauri::async_runtime::spawn_blocking(move || {
        let img = app_handle
            .clipboard()
            .read_image()
            .map_err(|_| OcrErrorDto::no_image())?;

        let width = img.width();
        let height = img.height();
        if width == 0 || height == 0 {
            return Err(OcrErrorDto::no_image());
        }

        // 尺寸上限校验必须在任何全量拷贝**之前**。
        // 剪贴板位图尺寸完全由放入它的程序决定，一张 20000×20000 的图
        // 在 `rgba().to_vec()` 处即申请 1.6 GB，足以打爆进程内存。
        if !transly_core::is_within_ocr_pixel_budget(width, height) {
            return Err(OcrErrorDto::too_large());
        }

        let rgba = image::RgbaImage::from_raw(width, height, img.rgba().to_vec())
            .ok_or_else(|| OcrErrorDto::internal("解析剪贴板图像像素矩阵失败"))?;

        let blocks = recognize_blocks(&rgba, &target_lang)?;
        Ok(blocks_to_text(&blocks))
    })
    .await
    .map_err(|e| OcrErrorDto::internal(format!("异步任务调度异常: {e}")))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(text: &str) -> OcrBlockDto {
        OcrBlockDto {
            index: 0,
            text: text.to_string(),
            x: 0.0,
            y: 0.0,
            w: 0.1,
            h: 0.1,
            line_count: 1,
            confidence: None,
        }
    }

    /// 关键回归：块拼文本必须按顺序用换行连接，且跳过空白块。
    ///
    /// 剪贴板 OCR 的对外行为依赖它；若改成无分隔拼接，多段文本会粘成一行。
    #[test]
    fn test_blocks_to_text_joins_in_order_and_skips_blank() {
        let blocks = vec![block("first"), block("   "), block("second")];
        assert_eq!(blocks_to_text(&blocks), "first\nsecond");
        assert_eq!(blocks_to_text(&[]), "");
    }
}
