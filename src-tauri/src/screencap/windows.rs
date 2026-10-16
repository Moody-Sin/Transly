//! Windows 屏幕捕获：GDI `BitBlt`，全流程内存零落盘。
//!
//! # 为什么是 GDI 而不是 WGC
//!
//! 受保护窗口（DRM 播放器、显式设置 `WDA_EXCLUDEFROMCAPTURE` 的窗口）下，
//! GDI 会露出壁纸或纯色，WGC（Windows Graphics Capture）则是零帧/超时。
//! 两者都拿不到内容，但 GDI 的失败在画面上直接可见，WGC 的零帧极难诊断；
//! 且 GDI 不需要额外的 crate 与 COM 初始化。故走 GDI。
//!
//! **受保护内容是 OS 层封锁，任何用户态后端都无效**，故不做「检测纯色再换
//! 后端」的重试——那只会把可见的失败变成不可见的失败。

use transly_core::{bgrx_to_rgba_in_place, MonitorRect, OcrErrorDto};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits,
    GetWindowDC, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    HBITMAP, HDC, HGDIOBJ, SRCCOPY,
};

/// 屏幕 DC 的 RAII 释放。
///
/// GDI 句柄是**进程级**资源：泄漏不会立刻报错，只会在「连续触发 5 次」这类
/// 验收里表现为资源耗尽。故所有路径（含每个提前 return）都必须释放。
struct ScreenDc(HDC);

impl Drop for ScreenDc {
    fn drop(&mut self) {
        unsafe {
            ReleaseDC(None, self.0);
        }
    }
}

struct MemDc(HDC);

impl Drop for MemDc {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteDC(self.0);
        }
    }
}

struct GdiBitmap(HBITMAP);

impl Drop for GdiBitmap {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(self.0 .0));
        }
    }
}

/// 已选入内存 DC 的位图。
///
/// 位图**在被选入 DC 期间删除是未定义行为**，故析构顺序必须是
/// 「先把原对象选回 DC、再删除位图」。`Drop::drop` 先于字段析构执行，
/// 正好给出这个顺序（字段 `bitmap` 在 `previous` 恢复之后才被释放）。
struct SelectedBitmap {
    dc: HDC,
    bitmap: GdiBitmap,
    previous: HGDIOBJ,
}

impl Drop for SelectedBitmap {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.previous);
        }
    }
}

pub fn capture_monitor(monitor: &MonitorRect) -> Result<image::RgbaImage, OcrErrorDto> {
    let width = i32::try_from(monitor.width)
        .map_err(|_| OcrErrorDto::capture_failed("显示器宽度超出 GDI 可表示范围"))?;
    let height = i32::try_from(monitor.height)
        .map_err(|_| OcrErrorDto::capture_failed("显示器高度超出 GDI 可表示范围"))?;
    if width <= 0 || height <= 0 {
        return Err(OcrErrorDto::capture_failed("显示器尺寸为零，无法捕获"));
    }

    // SAFETY：以下全部是 GDI 的原始句柄调用。每个句柄都包在 RAII guard 里，
    // 因此任何提前 return 都不会泄漏；像素缓冲区在 GetDIBits 返回前始终存活。
    unsafe {
        // NULL 窗口句柄 = 整个屏幕的 DC；其坐标即虚拟桌面绝对坐标，
        // 副屏可以为负，故 BitBlt 的源坐标直接用 MonitorRect 的物理坐标。
        let screen = ScreenDc(GetWindowDC(None));
        if screen.0.is_invalid() {
            return Err(OcrErrorDto::capture_failed(
                "GDI GetWindowDC 未能取得屏幕设备上下文",
            ));
        }

        let mem = MemDc(CreateCompatibleDC(Some(screen.0)));
        if mem.0.is_invalid() {
            return Err(OcrErrorDto::capture_failed("GDI CreateCompatibleDC 失败"));
        }

        let hbitmap = CreateCompatibleBitmap(screen.0, width, height);
        if hbitmap.is_invalid() {
            return Err(OcrErrorDto::capture_failed(
                "GDI CreateCompatibleBitmap 失败（可能显存不足）",
            ));
        }
        let bitmap = GdiBitmap(hbitmap);
        let selected = SelectedBitmap {
            dc: mem.0,
            bitmap,
            previous: SelectObject(mem.0, HGDIOBJ(hbitmap.0)),
        };

        // hdcsrc 必须是 Some(screen.0)：传 None 会被当成 NULL 源 DC，拿不到屏幕内容。
        BitBlt(
            mem.0,
            0,
            0,
            width,
            height,
            Some(screen.0),
            monitor.x,
            monitor.y,
            SRCCOPY,
        )
        .map_err(|e| OcrErrorDto::capture_failed(format!("GDI BitBlt 失败: {e}")))?;

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                // 负高度 = 自上而下存储。用正值会得到上下颠倒的冻结帧，
                // 而上下颠倒的截图在框选时几乎不可能被用户察觉（只有译文会错位）。
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut buf = vec![0u8; width as usize * height as usize * 4];
        let copied = GetDIBits(
            mem.0,
            selected.bitmap.0,
            0,
            height as u32,
            Some(buf.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        );
        if copied != height {
            return Err(OcrErrorDto::capture_failed(format!(
                "GDI GetDIBits 仅复制了 {copied}/{height} 行扫描线"
            )));
        }

        // GDI 的 BGRA32 第 4 字节在屏幕捕获下恒为 0，必须改写为不透明 alpha，
        // 否则 RgbaImage 会编码出一张全透明 PNG。
        bgrx_to_rgba_in_place(&mut buf);

        image::RgbaImage::from_raw(width as u32, height as u32, buf)
            .ok_or_else(|| OcrErrorDto::capture_failed("构建冻结帧内存位图失败"))
    }
}
