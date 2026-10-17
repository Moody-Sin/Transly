//! Linux（X11）屏幕捕获：纯 Rust `xcb` 的 `GetImage`，全流程内存零落盘。
//!
//! # 为什么不用系统截图 API / 不落临时文件
//!
//! Wayland 下的系统级截图接口（portal + PipeWire）要求先把帧写进临时文件，
//! 直接违背「全流程内存零落盘」承诺；且 Mutter 等 compositor 不认
//! `setAlwaysOnTop`（没有 `zwlr_layer_shell_v1`），覆盖窗无法保证置顶。
//! 故 Wayland 一律降级并给出 X11 指引（`OcrErrorDto::wayland_unsupported`）。
//!
//! # 为什么用 `GetImage` 而不是截图工具/临时文件
//!
//! X11 的 `GetImage` 请求直接把根窗口的像素回给客户端，整个链路上没有
//! 文件、没有子进程、没有磁盘；而 `import`/`scrot` 这类工具要么写临时文件，
//! 要么再解析 PNG，都不如直接拿像素干净。

use transly_core::{
    bgrx_to_rgba_in_place, detect_wayland_session, map_capture_error, MonitorRect, OcrErrorDto,
};
use xcb::{randr, x};

/// 捕获失败的错误种类由 `transly_core::map_capture_error` 决定，**绝不自己拼 `internal`**：
/// 捕获失败属于环境问题（无 X server、DISPLAY 未设、X server 不可达），
/// 归成 `internal` 会让用户看到无信息、也无法自助解决。
fn mapped(raw: String) -> OcrErrorDto {
    OcrErrorDto::new(map_capture_error(&raw), raw)
}

/// 捕获指定显示器在 X11 根窗口上的完整画面。
pub fn capture_monitor(monitor: &MonitorRect) -> Result<image::RgbaImage, OcrErrorDto> {
    // 顺序很重要：先判 Wayland。它没有 X11 根窗口可截，路径完全不同。
    if detect_wayland_session(
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
    ) {
        return Err(OcrErrorDto::wayland_unsupported());
    }

    // WSL2 无 X server、DISPLAY 未设置、X server 不可达都会在这里失败——这是
    // **预期行为**，不是缺陷。错误信息里带 "display" 关键词，会被
    // map_capture_error 映射成 UnsupportedPlatform。
    let (conn, screen_num) = xcb::Connection::connect(None)
        .map_err(|e| mapped(format!("Unable to open X display: {e}")))?;

    let setup = conn.get_setup();
    let screen = setup
        .roots()
        .nth(screen_num as usize)
        .ok_or_else(|| OcrErrorDto::capture_failed("X11 屏幕索引越界，无法定位根窗口"))?;
    let root = screen.root();

    let geometry = resolve_geometry(&conn, root, monitor);

    // x/y 是 i16、width/height 是 u16。4K 屏宽 3840 在 u16 范围内，但多屏横向
    // 拼接时虚拟桌面坐标可能超出 i16 范围（±32767）；`as` 会静默环绕成负数，
    // 把「截错区域」伪装成成功。必须显式报错。
    let x = i16::try_from(geometry.x)
        .map_err(|_| OcrErrorDto::capture_failed("显示器横向坐标超出 X11 协议 i16 范围"))?;
    let y = i16::try_from(geometry.y)
        .map_err(|_| OcrErrorDto::capture_failed("显示器纵向坐标超出 X11 协议 i16 范围"))?;
    let width = u16::try_from(geometry.width)
        .map_err(|_| OcrErrorDto::capture_failed("显示器宽度超出 X11 协议 u16 范围"))?;
    let height = u16::try_from(geometry.height)
        .map_err(|_| OcrErrorDto::capture_failed("显示器高度超出 X11 协议 u16 范围"))?;
    if width == 0 || height == 0 {
        return Err(OcrErrorDto::capture_failed("显示器尺寸为零，无法捕获"));
    }

    let cookie = conn.send_request(&x::GetImage {
        format: x::ImageFormat::ZPixmap,
        drawable: x::Drawable::Window(root),
        x,
        y,
        width,
        height,
        plane_mask: u32::MAX,
    });
    let reply = conn
        .wait_for_reply(cookie)
        .map_err(|e| mapped(format!("XCB GetImage failed: {e}")))?;

    // 色深只处理 24/32——现代桌面实际只会用到这两种，且它们的 ZPixmap 数据
    // 都是每像素 4 字节的 BGRX。8/16 位色深在当代桌面极罕见，直接给出可操作
    // 的降级文案，而不是悄悄按错误步长解析出一张糊图。
    let depth = reply.depth();
    if depth != 24 && depth != 32 {
        return Err(OcrErrorDto::capture_failed(format!(
            "不支持的 X11 色深 {depth}（本实现仅支持 24/32 位、每像素 4 字节的 BGRX 布局）"
        )));
    }

    // ZPixmap 下每扫描行按 32 位边界对齐；bpp=32 时行跨距即 width*4，无额外填充。
    let data = reply.data();
    let stride = width as usize * 4;
    let needed = stride * height as usize;
    if data.len() < needed {
        return Err(OcrErrorDto::capture_failed(format!(
            "X11 返回的像素数据长度不足（{} < {needed}）",
            data.len()
        )));
    }

    let mut rgba = data[..needed].to_vec();
    bgrx_to_rgba_in_place(&mut rgba);

    image::RgbaImage::from_raw(u32::from(width), u32::from(height), rgba)
        .ok_or_else(|| OcrErrorDto::capture_failed("构建冻结帧内存位图失败"))
}

struct Geometry {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

/// 用 randr 的显示器布局校准请求几何。
///
/// randr 是 X server 对显示器布局的**权威视图**；拿它与 tao 上报的
/// `MonitorRect` 按几何核对，可以不依赖 tao 的坐标单位假设（见
/// `find_monitor_for_point` 的说明）。核对不上时（扩展缺失、几何被驱动取整、
/// 混合 DPI 下 tao 单位漂移）回退到请求几何——真正的错配由调用方的尺寸
/// 自校验兜住，这里不做「猜一台显示器」这种更危险的动作。
fn resolve_geometry(conn: &xcb::Connection, root: x::Window, requested: &MonitorRect) -> Geometry {
    let fallback = Geometry {
        x: requested.x,
        y: requested.y,
        width: requested.width,
        height: requested.height,
    };

    let cookie = conn.send_request(&randr::GetMonitors {
        window: root,
        get_active: true,
    });
    let Ok(reply) = conn.wait_for_reply(cookie) else {
        return fallback;
    };

    for m in reply.monitors() {
        let (mx, my) = (i32::from(m.x()), i32::from(m.y()));
        let (mw, mh) = (u32::from(m.width()), u32::from(m.height()));
        // 允许 ±1 像素取整误差：tao 的物理量由逻辑量乘缩放系数得来，
        // 缩放系数非整数时末位会漂 1 像素。
        if (mx - requested.x).abs() <= 1
            && (my - requested.y).abs() <= 1
            && mw.abs_diff(requested.width) <= 1
            && mh.abs_diff(requested.height) <= 1
        {
            return Geometry {
                x: mx,
                y: my,
                width: mw,
                height: mh,
            };
        }
    }

    fallback
}
