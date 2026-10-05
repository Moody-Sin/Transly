use tauri_plugin_clipboard_manager::ClipboardExt;
use transly_core::{OcrErrorDto, OcrErrorKind};

/// 构建去重候选识别语言列表
pub fn build_candidate_languages(target_lang: &str) -> Vec<&'static str> {
    let primary = if target_lang.trim() == "zh-CN" {
        "en-US"
    } else {
        "system"
    };

    let mut list = Vec::with_capacity(2);
    for cand in [primary, "system", "en-US"] {
        if !list.contains(&cand) {
            list.push(cand);
        }
    }
    list
}

/// 将 RGBA8 像素切片转换为 Windows SoftwareBitmap 所需的 BGRA8 字节序
pub fn rgba_to_bgra(rgba: &[u8]) -> Vec<u8> {
    let mut bgra = rgba.to_vec();
    for chunk in bgra.chunks_exact_mut(4) {
        chunk.swap(0, 2);
    }
    bgra
}

/// 等比下采样计算：保证宽高均不大于 max_dim
pub fn clamp_dimensions(w: u32, h: u32, max_dim: u32) -> (u32, u32) {
    let max_side = w.max(h);
    if max_side <= max_dim || max_side == 0 {
        return (w, h);
    }
    let scale = max_dim as f64 / max_side as f64;
    let nw = ((w as f64 * scale).round() as u32).max(1);
    let nh = ((h as f64 * scale).round() as u32).max(1);
    (nw, nh)
}

#[cfg(not(target_os = "windows"))]
pub async fn recognize_clipboard_image(
    _app: &tauri::AppHandle,
    _target_lang: &str,
) -> Result<String, OcrErrorDto> {
    Err(OcrErrorDto::unsupported_platform())
}

#[cfg(target_os = "windows")]
struct ComGuard;

#[cfg(target_os = "windows")]
impl ComGuard {
    fn new() -> Self {
        unsafe {
            let _ = windows::Win32::System::WinRT::RoInitialize(
                windows::Win32::System::WinRT::RO_INIT_MULTITHREADED,
            );
        }
        Self
    }
}

#[cfg(target_os = "windows")]
impl Drop for ComGuard {
    fn drop(&mut self) {
        unsafe {
            windows::Win32::System::WinRT::RoUninitialize();
        }
    }
}

#[cfg(target_os = "windows")]
pub async fn recognize_clipboard_image(
    app: &tauri::AppHandle,
    target_lang: &str,
) -> Result<String, OcrErrorDto> {
    let app_handle = app.clone();
    let target_lang = target_lang.to_string();

    tauri::async_runtime::spawn_blocking(move || {
        let _guard = ComGuard::new();

        // 1. 在后台线程读取剪贴板图片（避免平台死锁）
        let img = match app_handle.clipboard().read_image() {
            Ok(im) => im,
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("no image") || msg.contains("empty") {
                    return Err(OcrErrorDto::no_image());
                }
                return Err(OcrErrorDto::new(OcrErrorKind::NoImage, msg));
            }
        };

        let width = img.width();
        let height = img.height();
        if width == 0 || height == 0 {
            return Err(OcrErrorDto::no_image());
        }

        // 2. 尺寸安全下采样（规避 Windows OCR MaxImageDimension 上限抛错）
        let max_dim = windows::Media::Ocr::OcrEngine::MaxImageDimension().unwrap_or(2600);
        let (nw, nh) = clamp_dimensions(width, height, max_dim);

        let rgba_bytes = if nw != width || nh != height {
            let dyn_img = image::RgbaImage::from_raw(width, height, img.rgba().to_vec())
                .ok_or_else(|| OcrErrorDto::internal("解析图像像素矩阵失败"))?;
            let resized = image::imageops::resize(
                &dyn_img,
                nw,
                nh,
                image::imageops::FilterType::Triangle,
            );
            resized.into_raw()
        } else {
            img.rgba().to_vec()
        };

        // 3. RGBA 转 BGRA8 并装载进入 SoftwareBitmap
        let bgra_bytes = rgba_to_bgra(&rgba_bytes);

        use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap};
        use windows::Storage::Streams::DataWriter;

        let writer = DataWriter::new()
            .map_err(|e| OcrErrorDto::internal(format!("初始化 DataWriter 失败: {e}")))?;
        writer
            .WriteBytes(&bgra_bytes)
            .map_err(|e| OcrErrorDto::internal(format!("写入图像像素失败: {e}")))?;
        let buffer = writer
            .DetachBuffer()
            .map_err(|e| OcrErrorDto::internal(format!("分离 Buffer 失败: {e}")))?;

        let bitmap = SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
            &buffer,
            BitmapPixelFormat::Bgra8,
            nw as i32,
            nh as i32,
            BitmapAlphaMode::Premultiplied,
        )
        .map_err(|e| OcrErrorDto::internal(format!("创建 SoftwareBitmap 失败: {e}")))?;

        // 4. 构建去重语言列表并轮询执行识别
        let candidates = build_candidate_languages(&target_lang);
        use windows::Globalization::Language;
        use windows::Media::Ocr::OcrEngine;

        let mut all_engines_missing = true;

        for cand in candidates {
            let engine_res = if cand == "system" {
                OcrEngine::TryCreateFromUserProfileLanguages()
            } else {
                match Language::CreateLanguage(&windows::core::HSTRING::from(cand)) {
                    Ok(l) => OcrEngine::TryCreateFromLanguage(&l),
                    Err(_) => continue,
                }
            };

            let Ok(engine) = engine_res else {
                continue;
            };
            all_engines_missing = false;

            let recognize_op = match engine.RecognizeAsync(&bitmap) {
                Ok(op) => op,
                Err(_) => continue,
            };

            if let Ok(ocr_result) = recognize_op.join() {
                if let Ok(hstr) = ocr_result.Text() {
                    let text = hstr.to_string();
                    if !text.trim().is_empty() {
                        return Ok(text.trim().to_string());
                    }
                }
            }
        }

        if all_engines_missing {
            Err(OcrErrorDto::no_language_pack())
        } else {
            Err(OcrErrorDto::empty_text())
        }
    })
    .await
    .map_err(|e| OcrErrorDto::internal(format!("异步任务调度异常: {e}")))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_candidate_languages_zh_cn() {
        let candidates = build_candidate_languages("zh-CN");
        assert_eq!(candidates, vec!["en-US", "system"]);
    }

    #[test]
    fn test_build_candidate_languages_non_zh() {
        let candidates = build_candidate_languages("en");
        assert_eq!(candidates, vec!["system", "en-US"]);
    }

    #[test]
    fn test_rgba_to_bgra_swizzle() {
        let rgba = vec![10, 20, 30, 255, 40, 50, 60, 200];
        let bgra = rgba_to_bgra(&rgba);
        assert_eq!(bgra, vec![30, 20, 10, 255, 60, 50, 40, 200]);
    }

    #[test]
    fn test_clamp_dimensions_under_limit() {
        assert_eq!(clamp_dimensions(800, 600, 2600), (800, 600));
    }

    #[test]
    fn test_clamp_dimensions_exceeding_limit() {
        let (nw, nh) = clamp_dimensions(5200, 2600, 2600);
        assert_eq!(nw, 2600);
        assert_eq!(nh, 1300);
    }
}
