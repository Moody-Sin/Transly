//! 屏幕捕获（冻结帧）的平台分发层。
//!
//! 三个平台各有一份实现，统一入口签名 `capture_monitor`，由 `cfg` 三段式
//! 门控（`windows` / `macos` / 其余即 Linux）。**必须是三段式**：若只写
//! `#[cfg(not(target_os = "windows"))]`，tesseract/Linux 专用代码会被编进 macOS。
//!
//! # 零落盘
//!
//! 本模块所有路径都只在内存中搬运像素，**不调用任何 `fs` / `temp_dir`**。
//! 这是 README 对外承诺的「全流程内存零落盘」，也是 Linux Wayland 直接降级的
//! 核心理由（系统级截图 API 都要先落临时文件）。
//!
//! # 尺寸自校验
//!
//! 各平台只负责「尽力捕获」，**不在本层做尺寸校验**：捕获结果与显示器物理
//! 尺寸是否吻合由调用方（`overlay`）用 `transly_core::is_capture_size_consistent`
//! 统一判定（该判据是纯逻辑，下沉在 core 里可单测）。
//!
//! # 线程
//!
//! `capture_monitor` 是**同步阻塞**调用（X11 连接、GDI 位块传输、CGImage 拷贝
//! 均可能耗时数十毫秒），调用方必须用 `spawn_blocking` 派发到线程池，
//! 不得直接占用 Tauri 的 async 工作线程。

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::capture_monitor;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::capture_monitor;

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod linux;
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub use linux::capture_monitor;
