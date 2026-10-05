use tauri_plugin_clipboard_manager::ClipboardExt;
use transly_core::{build_candidate_languages, clamp_dimensions, rgba_to_bgra, OcrErrorDto};

#[cfg(not(target_os = "windows"))]
pub async fn recognize_clipboard_image(
    _app: &tauri::AppHandle,
    _target_lang: &str,
) -> Result<String, OcrErrorDto> {
    Err(OcrErrorDto::unsupported_platform())
}

#[cfg(target_os = "windows")]
struct ComGuard {
    should_uninitialize: bool,
}

#[cfg(target_os = "windows")]
impl ComGuard {
    fn new() -> Result<Self, OcrErrorDto> {
        let hr = unsafe {
            windows::Win32::System::WinRT::RoInitialize(
                windows::Win32::System::WinRT::RO_INIT_MULTITHREADED,
            )
        };

        if hr.is_ok() {
            Ok(Self {
                should_uninitialize: true,
            })
        } else {
            Err(OcrErrorDto::internal(format!(
                "WinRT Apartment (MTA) 初始化失败或线程模式冲突: HRESULT {hr:?}"
            )))
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.should_uninitialize {
            unsafe {
                windows::Win32::System::WinRT::RoUninitialize();
            }
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
        let _guard = ComGuard::new()?;

        // 1. 在后台线程读取剪贴板图片（避免平台死锁）
        let img = match app_handle.clipboard().read_image() {
            Ok(im) => im,
            Err(_) => {
                return Err(OcrErrorDto::no_image());
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
            let resized =
                image::imageops::resize(&dyn_img, nw, nh, image::imageops::FilterType::Triangle);
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
