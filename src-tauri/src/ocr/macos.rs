//! macOS OCR：系统原生 Vision（`VNRecognizeTextRequest`），支持中日韩。
//!
//! # 坐标是唯一的坑
//!
//! Vision 的 `boundingBox()` 是**归一化**矩形且**原点在左下、y 向上**，
//! 与本项目「原点左上」的契约相反。转换由 `NormRect::from_vision_norm`
//! 统一完成（`y_top = 1 - y - h`）——**缺这一步会让整块译文垂直镜像**，
//! 且在对称排版下肉眼不易察觉。
//!
//! 本模块返回的是**像素**框，故在这里就完成「归一化 → 像素」的换算；
//! 上层的 `from_pixels_in` 再按同一张图的尺寸归一化回去。
//!
//! # 语言代码
//!
//! Vision 用 BCP-47（`zh-Hans`、`en-US`、`ja`）。**不得**把
//! `build_candidate_languages` 的产物直接传进来——那是 Windows 语义，
//! 其中的 `"system"` 会让 Vision 直接失败。
//!
//! # autoreleasepool
//!
//! 本函数由 `spawn_blocking` 在**后台线程**调用，必须自行包
//! `autoreleasepool`，否则 Objective-C 对象只增不减。

use objc2::rc::autoreleasepool;
use objc2::runtime::AnyObject;
// `alloc()` 来自 `AllocAnyThread` trait，不导入则找不到该方法。
use objc2::AllocAnyThread;
use objc2_foundation::{NSArray, NSDictionary, NSString};
use objc2_vision::{
    VNImageOption, VNImageRequestHandler, VNRecognizeTextRequest, VNRequest,
    VNRequestTextRecognitionLevel,
};
use transly_core::{is_within_ocr_pixel_budget, OcrErrorDto, PixelBox};

/// 把目标语言映射为 Vision 使用的 BCP-47 代码。
///
/// **绝不能返回 `"system"`**（那是 Windows 的语义，Vision 不认）。
fn vision_languages(target_lang: &str) -> Vec<&'static str> {
    let clean = target_lang.trim().to_lowercase();
    if clean.starts_with("ja") {
        vec!["ja", "en-US"]
    } else if clean.starts_with("ko") {
        vec!["ko", "en-US"]
    } else {
        // 中英双向支持：无论目标语言是中文还是英文，均同时加载中英语言模型，
        // 确保中译英（截中文）与英译中（截英文）均能自适应识别
        vec!["zh-Hans", "zh-Hant", "en-US"]
    }
}

pub fn recognize_words(
    image: &image::RgbaImage,
    target_lang: &str,
) -> Result<Vec<PixelBox>, OcrErrorDto> {
    let width = image.width();
    let height = image.height();
    if width == 0 || height == 0 {
        return Err(OcrErrorDto::no_image());
    }
    if !is_within_ocr_pixel_budget(width, height) {
        return Err(OcrErrorDto::too_large());
    }

    let rgba = image.as_raw().clone();
    let languages = vision_languages(target_lang);

    // 后台线程必须自行包 autoreleasepool。
    autoreleasepool(|_| recognize_inner(&rgba, width, height, &languages))
}

fn recognize_inner(
    rgba: &[u8],
    width: u32,
    height: u32,
    languages: &[&'static str],
) -> Result<Vec<PixelBox>, OcrErrorDto> {
    // RgbaImage → CGImage：用 `CGDataProvider` + `CGImageCreate`。
    // 这条路径同样全内存，且不需要先编码 PNG。
    let cg_image = build_cg_image(rgba, width, height)?;

    let request = VNRecognizeTextRequest::new();
    request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
    // 语言校正能显著改善连字与标点，但会略微拖慢；截图场景文本通常清晰，开启更划算。
    request.setUsesLanguageCorrection(true);

    let ns_languages: Vec<objc2::rc::Retained<NSString>> =
        languages.iter().map(|l| NSString::from_str(l)).collect();
    let lang_array = NSArray::from_retained_slice(&ns_languages);
    request.setRecognitionLanguages(&lang_array);

    let request_ref: &VNRequest = &request;
    let requests = NSArray::from_slice(&[request_ref]);

    // `options` 必须是 `NSDictionary<VNImageOption, AnyObject>`（泛型不能靠推断）。
    let options: objc2::rc::Retained<NSDictionary<VNImageOption, AnyObject>> = NSDictionary::new();
    let handler = unsafe {
        VNImageRequestHandler::initWithCGImage_options(
            VNImageRequestHandler::alloc(),
            &cg_image,
            &options,
        )
    };

    // `performRequests_error` 返回 `Result`，直接 `?`（不得 unwrap）。
    handler
        .performRequests_error(&requests)
        .map_err(|e| OcrErrorDto::internal(format!("Vision 识别请求失败: {e}")))?;

    let Some(results) = request.results() else {
        return Ok(Vec::new());
    };

    let mut words = Vec::new();
    for observation in results.iter() {
        // 取最佳候选；topCandidates(1) 不会为空。
        let candidates = observation.topCandidates(1);
        let Some(best) = candidates.firstObject() else {
            continue;
        };
        let text = best.string().to_string();
        if text.trim().is_empty() {
            continue;
        }

        // ⚠️ Vision 原点在**左下**、y 向上；本契约原点在左上。
        // 归一化 → 像素时 y 必须翻转：y_top = (1 - y - h) * img_h。
        let bbox = unsafe { observation.boundingBox() };
        let nx = bbox.origin.x as f32;
        let ny = bbox.origin.y as f32;
        let nw = bbox.size.width as f32;
        let nh = bbox.size.height as f32;

        let y_top = 1.0 - ny - nh;
        words.push(PixelBox::new(
            (nx.max(0.0) * width as f32) as u32,
            (y_top.max(0.0) * height as f32) as u32,
            (nw.max(0.0) * width as f32) as u32,
            (nh.max(0.0) * height as f32) as u32,
            text,
        ));
    }

    Ok(words)
}

/// 由 RGBA8 字节构造 `CGImage`（全内存，不落盘、不编码 PNG）。
fn build_cg_image(
    rgba: &[u8],
    width: u32,
    height: u32,
) -> Result<objc2_core_foundation::CFRetained<objc2_core_graphics::CGImage>, OcrErrorDto> {
    use objc2_core_foundation::CFData;
    use objc2_core_graphics::{
        CGBitmapInfo, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo, CGImageByteOrderInfo,
    };

    let data = CFData::from_bytes(rgba);
    let provider = CGDataProvider::with_cf_data(Some(&data))
        .ok_or_else(|| OcrErrorDto::internal("构造 CGDataProvider 失败"))?;

    let color_space = CGColorSpace::new_device_rgb()
        .ok_or_else(|| OcrErrorDto::internal("构造 CGColorSpace 失败"))?;

    // RGBA8：内存序 R,G,B,A → 小端 32 位下即 `ByteOrder32Big | AlphaLast`
    // （与 `kCGBitmapByteOrderDefault` 等价于大端序的 RGBA 布局）。
    let bitmap_info = CGBitmapInfo(CGImageByteOrderInfo::Order32Big.0 | CGImageAlphaInfo::Last.0);

    // `CGImage::new` 是 unsafe（裸指针参数），且需要 `CFData`/`provider` 存活到构造完成。
    let image = unsafe {
        CGImage::new(
            width as usize,
            height as usize,
            8,
            32,
            width as usize * 4,
            Some(&color_space),
            bitmap_info,
            Some(&provider),
            std::ptr::null(),
            false,
            objc2_core_graphics::CGColorRenderingIntent::RenderingIntentDefault,
        )
    }
    .ok_or_else(|| OcrErrorDto::internal("构造 CGImage 失败"))?;

    Ok(image)
}
