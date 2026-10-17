# `screencap` —— 三平台屏幕捕获

截图翻译的「冻结帧」来源。三平台各自实现，统一入口 `capture_monitor`。

## 目录

| 文件 | 平台 | 实现 |
|---|---|---|
| `mod.rs` | — | 三段式 `cfg` 分发，模块文档 |
| `windows.rs` | Windows | GDI `BitBlt` + `GetDIBits` |
| `macos.rs` | macOS | `CGDisplayCreateImage` + `CGDataProvider` |
| `linux.rs` | Linux | X11 `xcb` 的 `GetImage`（Wayland 降级） |

## 契约

```rust
pub fn capture_monitor(monitor: &MonitorRect)
    -> Result<image::RgbaImage, transly_core::OcrErrorDto>;
```

- 返回**整台显示器**的 RGBA8 冻结帧，坐标原点左上。
- 调用方必须用 `spawn_blocking` 派发：三个后端都是同步阻塞调用。
- 失败一律走 `transly_core::map_capture_error` / `OcrErrorDto` 的标准构造器，
  不自己拼 `internal`。

## 硬约束

- **零落盘**：全程只在内存搬运像素，不调用任何 `fs` / `temp_dir`。
  这是 `README.md` 对外的「全流程内存零落盘」承诺，也是 Wayland 直接降级的理由
  （系统级截图 API 都要先落临时文件）。
- **三段式 `cfg`**：`windows` / `macos` / `not(any(...))`。写成「`not(windows)`
  即 Linux」会把 xcb 代码编进 macOS。平台专属 `use` 同样要门控，否则
  `-D warnings` 下会因 unused import 失败。
- **不引入 `xcap`**：其 Linux 依赖链强制拉入 pipewire/libspa，在 Ubuntu 22.04 上编译失败，
  详见 `src-tauri/Cargo.toml` 的注释。
- **不用 tao 的 `monitor_from_point`**：其入参单位与 `cursor_position()` 的返回值不一致
  （已实测），选屏由 `transly_core::find_monitor_for_point` 用纯物理量完成。

## 上游 API 陷阱（已核实，勿凭记忆推翻）

- **`x::GetImage` 在 `xcb::x` 命名空间下**，不是 `xcb::GetImage`；`GetImageReply`
  的 `length()` 是**私有方法**，取像素必须用 `data()`（`length()` 直接调用编不过）。
- **`xcb` crate 有 `build.rs`**（用 `quick-xml` 从自带 XML 生成绑定）。它**不依赖系统头文件**，
  但首次编译较慢，别误判为卡死。其 randr 扩展**零 `extern "C"`**（全走 X11 wire protocol），
  故**不需要** `libxcb-randr.so` / `libxcb-randr0-dev`；链接的是 `libxcb.so.1`（GUI 环境必有）。
- **`x::GetImage` 的 `x`/`y` 是 `i16`、`width`/`height` 是 `u16`**：多屏横向拼接时虚拟桌面
  坐标可能超出 `i16` 范围，`as` 会**静默环绕成负数**并截到错误区域，故必须显式 `try_from` 报错。
- **`CGDisplayCreateImage` 带 `#[deprecated]`**，`-D warnings` 下必须 `#[allow(deprecated)]`；
  它**无权限时显式返回 `None`**（不是空白图），故不需要纯色帧启发式检测。
- **macOS 取像素只有 `CGDataProvider → CFData` 一条路**（该 crate 无 plain 版
  `CGBitmapContextCreate`），且 `byte_ptr()` 的裸指针**只在 `CFData` 存活期内有效**。
- **tao 的 `Monitor::size()` 在 macOS 上疑似双重缩放**：`CGDisplayPixelsWide()` 已是物理像素，
  却又套 `PhysicalSize::from_logical()`（再乘一次 scale）。故**窗口尺寸以捕获结果为准**
  （见 `overlay/mod.rs`），不要直接信任 `Monitor::size()`。

## 已知限制

- **Linux 仅 X11**。Wayland 下系统截图 API 需落临时文件，且 compositor 不认置顶，
  故直接降级并给出 X11 指引。
- **受保护内容（DRM / `WDA_EXCLUDEFROMCAPTURE`）无法捕获**：这是 OS 层封锁，
  任何用户态后端都无效（Windows 上 GDI 露壁纸、WGC 零帧）。
- **尺寸自校验在 core**：捕获结果与显示器上报尺寸的一致性判定位于
  `transly_core::is_capture_size_consistent`（纯逻辑，可单测），本层不做该判定。
