# src-tauri/src (桌面平台适配宿主层源码)

> Transly 桌面端原生集成层、生命周期调度、平台凭据与本地服务实现。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

本目录为 Tauri 2 原生平台适配层的源码实现。负责协调系统原生 API（托盘、快捷键、剪贴板、凭据管理、注册表代理、WinRT OCR），承载本地 HTTP 翻译服务，并通过薄层 Tauri Command 供前端调用。

## 目录

- [模块构成](#模块构成)
- [核心机制与设计规程](#核心机制与设计规程)
- [测试](#测试)
- [许可证](#许可证)

## 模块构成

- **main.rs**：应用入口。负责全局状态挂载（Newtype 包装防 TypeId 冲突）、系统托盘、快捷键注册、剪贴板监听后台任务及优雅退出。
- **lib.rs**：模块声明与 `app_command_builder!` 宏，集中向 Tauri 注册所有前端 IPC 命令。
- **commands/**：Tauri Command 薄封装入口。所有阻塞 I/O（SQLite、Keyring）均下沉至 `spawn_blocking`。
- **server/**：基于 `axum` 的本地回环 HTTP 服务（`127.0.0.1:39281`），负责浏览器扩展的批量翻译代理、一键配对握手与 DNS Rebinding 防护。
- **engine_pool.rs**：翻译引擎连接池。维护长连接实例，支持读写锁高并发查询、异步凭据安全加载及网络代理配置热重载。
- **cache_controller.rs**：缓存开关代际门禁，保障前台设置切换与后台历史清理的数据一致性。
- **clipboard_gate.rs**：剪贴板监听去重、划词模拟复制/粘贴与系统级 `CLIPBOARD_SIMULATION_MUTEX` 互斥保护。
- **ocr.rs**：Windows 平台原生 `Windows.Media.Ocr` 硬件加速文字识别（全流程内存零落盘；非 Windows 平台返回明确未支持错误）。
- **proxy.rs**：Windows 注册表系统代理原生探测（`Internet Settings`）与代理模式（Auto / System / Manual / None）切换。
- **secrets.rs**：操作系统原生凭据管理器封装（`keyring`），敏感商业 API Key 与配对 Token 严格不落盘。
- **storage.rs**：应用数据目录解析（支持单文件便携版 Portable 模式检测与数据隔离）及漫游目录（Roaming 到 Local）幂等迁移。

## 核心机制与设计规程

1. **异步运行时与阻塞 I/O 隔离**：Tauri Command 默认在主线程或 async 运行时执行。凡涉及平台 Keyring、SQLite 磁盘 I/O 或注册表读取，必须使用 `tokio::task::spawn_blocking` 包裹，绝不在异步锁临界区跨越阻塞调用。
2. **安全第一与回环绑定**：本地 HTTP 服务严格绑定 `127.0.0.1`，执行 Bearer Token 强鉴权与独立于 CORS 的 `Host` 头校验，彻底阻断 DNS Rebinding。
3. **剪贴板原子性保护**：划词原地替换采用三阶段拆分：模拟复制（锁内） -> 网络翻译（无锁并发） -> 模拟粘贴并恢复旧剪贴板（加锁并复核），杜绝长网络 I/O 挂死主线程及覆写用户新复制内容。

## 测试

```sh
cargo test -p src-tauri
```

## 许可证

[商业专有许可](../../LICENSE)
