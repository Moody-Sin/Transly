//! macOS 屏幕捕获：CoreGraphics `CGDisplayCreateImage`，全流程内存零落盘。
//!
//! # 权限两段式
//!
//! ① 捕获前预检 `CGPreflightScreenCaptureAccess()`：未授权时先触发系统弹窗，
//! 再返回 `PermissionDenied`（文案含「授权后需重启应用」）。
//! ② 捕获后兜底：即便预检通过，`CGDisplayCreateImage` 仍可能返回 `None`
//! （授权后未重启、显示器睡眠）。**它返回的是 `None` 而非静默空白图**，
//! 故不需要「纯色帧检测」这类启发式判据。
//!
//! # 显示器匹配
//!
//! `CGDisplayCreateImage` 需要的是 `CGDirectDisplayID`，而进入本层的只有
//! 扁平的 `MonitorRect`。故用 `CGGetActiveDisplayList` 枚举活动显示器，
//! 按「原点 + 物理像素尺寸」匹配；取不到列表时回退主显示器。

use objc2::rc::autoreleasepool;
// `CGDisplayCreateImage` 带 `#[deprecated]`：Apple 建议改用 ScreenCaptureKit，
// 但它是当前可用的稳定 API，且比 `CGWindowListCreateImage` 更贴合按显示器捕获。
// 不 allow 会让 `-D warnings` 直接编译失败。
#[allow(deprecated)]
use objc2_core_graphics::{
    CGDataProvider, CGDirectDisplayID, CGDisplayBounds, CGDisplayCreateImage, CGDisplayPixelsHigh,
    CGDisplayPixelsWide, CGError, CGGetActiveDisplayList, CGImage, CGImageAlphaInfo,
    CGImageByteOrderInfo, CGMainDisplayID, CGPreflightScreenCaptureAccess,
    CGRequestScreenCaptureAccess,
};
use transly_core::{bgrx_to_rgba_in_place, is_capture_size_consistent, MonitorRect, OcrErrorDto};

/// 单次枚举的显示器数量上限。32 台远超任何消费级多屏配置。
const MAX_DISPLAYS: usize = 32;

pub fn capture_monitor(monitor: &MonitorRect) -> Result<image::RgbaImage, OcrErrorDto> {
    if !CGPreflightScreenCaptureAccess() {
        // 预检失败即已触发系统授权引导；用户拒绝后再次触发不会重复弹窗。
        let _ = CGRequestScreenCaptureAccess();
        return Err(OcrErrorDto::permission_denied());
    }

    let display = resolve_display_id(monitor);

    // 非主线程必须有 autoreleasepool，否则 Objective-C 对象只增不减。
    // 本函数由 `spawn_blocking` 在后台线程调用，故必须包。
    autoreleasepool(|_| {
        // Apple 建议改用 ScreenCaptureKit，但 `CGDisplayCreateImage` 是当前可用的
        // 稳定 API，且比 `CGWindowListCreateImage` 更贴合「按显示器捕获」语义。
        // 捕获全部封装在本函数内，将来替换 ScreenCaptureKit 不影响上层。
        #[allow(deprecated)]
        let image = CGDisplayCreateImage(display).ok_or_else(OcrErrorDto::permission_denied)?;
        decode_image(&image)
    })
}

/// 在活动显示器列表里定位与 `MonitorRect` 对应的 `CGDirectDisplayID`。
///
/// 匹配分两级：先要求「原点 + 物理像素尺寸」同时对上；若只有原点对上
/// （macOS 上 tao 的 `Monitor::size()` 疑似双重缩放，尺寸不可信），
/// 则接受该显示器。都匹配不到时回退主显示器——真正的错配由上层
/// 的尺寸一致性判定留痕，这里不做「猜最近的一台」这种更危险的动作。
fn resolve_display_id(monitor: &MonitorRect) -> CGDirectDisplayID {
    let mut displays = [0 as CGDirectDisplayID; MAX_DISPLAYS];
    let mut count: u32 = 0;
    // SAFETY：两个出参指针都指向本函数栈上、生命周期覆盖本次调用的有效内存。
    let err =
        unsafe { CGGetActiveDisplayList(displays.len() as u32, displays.as_mut_ptr(), &mut count) };
    if err != CGError::Success {
        return CGMainDisplayID();
    }

    let listed = (count as usize).min(MAX_DISPLAYS);
    let scale = if monitor.scale_factor > 0.0 {
        monitor.scale_factor
    } else {
        1.0
    };

    let mut origin_only = None;
    for &id in &displays[..listed] {
        // `CGDisplayBounds` 返回的是逻辑点（全局坐标，原点在主显示器左上），
        // 乘 scale 得物理量——与 tao `Monitor::position()` 的语义一致。
        let bounds = CGDisplayBounds(id);
        let origin_x = (bounds.origin.x * scale).round() as i32;
        let origin_y = (bounds.origin.y * scale).round() as i32;
        if origin_x != monitor.x || origin_y != monitor.y {
            continue;
        }

        // `CGDisplayPixelsWide/High` **已是物理像素**，不能再乘 scale。
        if is_capture_size_consistent(
            CGDisplayPixelsWide(id) as u32,
            CGDisplayPixelsHigh(id) as u32,
            monitor.width,
            monitor.height,
        ) {
            return id;
        }
        if origin_only.is_none() {
            origin_only = Some(id);
        }
    }

    origin_only.unwrap_or_else(|| CGMainDisplayID())
}

/// 把 `CGImage` 解包成 RGBA8。
///
/// 该 crate 里没有 plain 版 `CGBitmapContextCreate`，取像素只有
/// `CGDataProvider → CGDataProviderCopyData → CFData` 一条路。
fn decode_image(image: &CGImage) -> Result<image::RgbaImage, OcrErrorDto> {
    let width = CGImage::width(Some(image));
    let height = CGImage::height(Some(image));
    if width == 0 || height == 0 {
        return Err(OcrErrorDto::capture_failed("CGImage 尺寸为零"));
    }

    let bits_per_pixel = CGImage::bits_per_pixel(Some(image));
    if bits_per_pixel != 32 {
        return Err(OcrErrorDto::capture_failed(format!(
            "不支持的 CGImage 位深 {bits_per_pixel}（本实现仅支持每像素 32 位）"
        )));
    }

    let row_bytes = width
        .checked_mul(4)
        .ok_or_else(|| OcrErrorDto::capture_failed("CGImage 尺寸溢出"))?;
    let stride = CGImage::bytes_per_row(Some(image));
    if stride < row_bytes {
        return Err(OcrErrorDto::capture_failed(format!(
            "CGImage 行跨距 {stride} 小于一行像素所需 {row_bytes}"
        )));
    }

    let provider = CGImage::data_provider(Some(image))
        .ok_or_else(|| OcrErrorDto::capture_failed("CGImage 没有可用的像素数据提供者"))?;
    let data = CGDataProvider::data(Some(&provider))
        .ok_or_else(|| OcrErrorDto::capture_failed("CGDataProviderCopyData 未返回数据"))?;

    let len = data.length() as usize;
    let ptr = data.byte_ptr();
    if ptr.is_null() || len == 0 {
        return Err(OcrErrorDto::capture_failed("CGImage 像素数据为空"));
    }

    let mut rgba = vec![0u8; row_bytes * height];
    // `byte_ptr()` 的裸指针只在 `CFData` 存活期间有效，故逐行拷贝必须在本作用域内完成，
    // 不能把裸指针带出。逐行拷贝同时消化了 stride 与 row_bytes 的差异（行对齐填充）。
    for y in 0..height {
        let src = y * stride;
        if src + row_bytes > len {
            return Err(OcrErrorDto::capture_failed(format!(
                "CGImage 像素数据长度不足（行 {y} 需要 {} 字节，实有 {len}）",
                src + row_bytes
            )));
        }
        let dst = y * row_bytes;
        // SAFETY：上面已断言 src + row_bytes <= len，且 ptr 在 data 存活期内有效，
        // 目标切片长度与源切片一致（row_bytes），无重叠。
        let row = unsafe { std::slice::from_raw_parts(ptr.add(src), row_bytes) };
        rgba[dst..dst + row_bytes].copy_from_slice(row);
    }

    let order = CGImage::byte_order_info(Some(image));
    let alpha = CGImage::alpha_info(Some(image));

    // 通道序由 byte order 决定，不能假定。屏幕捕获的常见布局是
    // `kCGBitmapByteOrder32Little | kCGImageAlphaPremultipliedFirst`（内存序 BGRX）；
    // 明确认识小端 *First 与大端 *Last 两类，其余布局直接报错——
    // 按错误顺序解析会得到「红蓝互换」这种肉眼不易察觉的错色，报错更安全。
    let little_endian_first = order == CGImageByteOrderInfo::Order32Little
        && (alpha == CGImageAlphaInfo::First
            || alpha == CGImageAlphaInfo::PremultipliedFirst
            || alpha == CGImageAlphaInfo::NoneSkipFirst);
    let big_endian_last = (order == CGImageByteOrderInfo::Order32Big
        || order == CGImageByteOrderInfo::OrderDefault)
        && (alpha == CGImageAlphaInfo::Last
            || alpha == CGImageAlphaInfo::PremultipliedLast
            || alpha == CGImageAlphaInfo::NoneSkipLast
            || alpha == CGImageAlphaInfo::None);

    if little_endian_first {
        bgrx_to_rgba_in_place(&mut rgba);
    } else if big_endian_last {
        // 已是 RGBA 序；屏幕捕获不透明，把 alpha 补成 255（可能是预乘或填充 0）。
        for chunk in rgba.as_chunks_mut::<4>().0 {
            chunk[3] = 255;
        }
    } else {
        return Err(OcrErrorDto::capture_failed(format!(
            "不支持的 CGImage 像素布局（byte order {} / alpha info {}）",
            order.0, alpha.0
        )));
    }

    image::RgbaImage::from_raw(width as u32, height as u32, rgba)
        .ok_or_else(|| OcrErrorDto::capture_failed("构建冻结帧内存位图失败"))
}
