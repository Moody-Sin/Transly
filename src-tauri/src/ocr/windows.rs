//! Windows OCR：系统原生 `Windows.Media.Ocr`（WinRT），支持中日韩。
//!
//! # 坐标空间
//!
//! `OcrWord::BoundingRect()` 的单位是**喂给 `RecognizeAsync` 那张
//! `SoftwareBitmap` 的像素**。本模块在大图时先 `clamp_dimensions` 下采样，
//! 识别完成后按比例缩放回原始输入图的 `(width, height)` 像素尺寸，
//! 保证 `recognize_words` 对外契约始终与入参 `image` 的像素空间对齐。
//!
//! # 置信度
//!
//! WinRT 的 `OcrWord` **不提供置信度**，故 `PixelBox::confidence` 恒为 `None`。
//! `filter_blocks` 对 `None` 放行（不做置信度过滤），这是刻意的：宁可多送一个块，
//! 也不要因缺元数据把有效文本丢掉。

use transly_core::{
    build_candidate_languages, clamp_dimensions, rgba_to_bgra_in_place, OcrErrorDto, PixelBox,
};

/// WinRT 需要线程处于 MTA 单元；`spawn_blocking` 的线程默认未初始化。
pub struct ComGuard {
    should_uninitialize: bool,
}

impl ComGuard {
    pub fn new() -> Result<Self, OcrErrorDto> {
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

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.should_uninitialize {
            unsafe {
                windows::Win32::System::WinRT::RoUninitialize();
            }
        }
    }
}

pub fn recognize_words(
    image: &image::RgbaImage,
    target_lang: &str,
) -> Result<Vec<PixelBox>, OcrErrorDto> {
    let _guard = ComGuard::new()?;

    let width = image.width();
    let height = image.height();
    if width == 0 || height == 0 {
        return Err(OcrErrorDto::no_image());
    }

    // 像素预算校验必须在任何全量拷贝**之前**（与剪贴板路径同理）。
    if !transly_core::is_within_ocr_pixel_budget(width, height) {
        return Err(OcrErrorDto::too_large());
    }

    // 尺寸安全下采样（规避 Windows OCR 的 MaxImageDimension 上限抛错）。
    let max_dim = windows::Media::Ocr::OcrEngine::MaxImageDimension().unwrap_or(2600);
    let (nw, nh) = clamp_dimensions(width, height, max_dim);

    let mut bgra_bytes = if nw != width || nh != height {
        let resized = image::imageops::resize(image, nw, nh, image::imageops::FilterType::Triangle);
        resized.into_raw()
    } else {
        image.as_raw().clone()
    };
    rgba_to_bgra_in_place(&mut bgra_bytes);

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

    use windows::Globalization::Language;
    use windows::Media::Ocr::OcrEngine;

    let candidates = build_candidate_languages(target_lang);
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
        let Ok(ocr_result) = recognize_op.join() else {
            continue;
        };
        let Ok(lines) = ocr_result.Lines() else {
            continue;
        };

        let mut words = Vec::new();
        for line in lines {
            let Ok(line_words) = line.Words() else {
                continue;
            };
            for word in line_words {
                let Ok(rect) = word.BoundingRect() else {
                    continue;
                };
                let Ok(text) = word.Text() else {
                    continue;
                };
                let text = text.to_string();
                if text.trim().is_empty() {
                    continue;
                }
                // WinRT 的 Rect 用 f32；负值理论上不出现，但取整前先夹到 0 避免 `as` 环绕。
                words.push(PixelBox::new(
                    rect.X.max(0.0) as u32,
                    rect.Y.max(0.0) as u32,
                    rect.Width.max(0.0) as u32,
                    rect.Height.max(0.0) as u32,
                    text,
                ));
            }
        }

        // 若当前候选引擎未识别出任何有效文字（例如英文引擎扫描纯中文画面），
        // 绝不可直接 return 空结果，必须继续尝试后续候选语言包（如 system / zh-Hans-CN）
        if words.is_empty() {
            continue;
        }

        // 若进行了尺寸下采样（规避 Windows OCR 的 MaxImageDimension 上限），
        // 词框像素坐标必须按比例还原回原始输入图的 (width, height) 像素空间，
        // 保证上层以原始图尺寸归一化时坐标不偏移、不变小。
        if nw != width || nh != height {
            let scale_x = width as f64 / nw as f64;
            let scale_y = height as f64 / nh as f64;
            for w in &mut words {
                let orig_x = ((w.x as f64 * scale_x).round() as u32).min(width);
                let orig_y = ((w.y as f64 * scale_y).round() as u32).min(height);
                let orig_w =
                    ((w.w as f64 * scale_x).round() as u32).min(width.saturating_sub(orig_x));
                let orig_h =
                    ((w.h as f64 * scale_y).round() as u32).min(height.saturating_sub(orig_y));
                w.x = orig_x;
                w.y = orig_y;
                w.w = orig_w;
                w.h = orig_h;
            }
        }

        return Ok(words);
    }

    if all_engines_missing {
        Err(OcrErrorDto::no_language_pack())
    } else {
        Ok(Vec::new())
    }
}
