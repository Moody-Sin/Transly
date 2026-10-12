# src-tauri

> Transly 桌面端原生服务与 Tauri 2 平台集成适配层。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

本模块作为 Tauri 2 后端应用，负责与系统底层 API、全局快捷键、托盘、系统凭据以及本地 HTTP 服务进行交互。

## 目录

- [结构](#结构)
- [安全规则](#安全规则)
- [操作与测试指南](#操作与测试指南)
- [许可证](#许可证)

## 结构

- **commands**: 对外暴露的 Tauri command 强类型调用接入层（薄封装），下沉 SQLite 与 Keyring 阻塞 I/O 至 `spawn_blocking`。
- **engine_pool.rs**: 引擎实例连接池（单引擎直查、读写锁隔离、无锁异步凭据加载、网络代理热重载）。
- **cache_controller.rs**: 缓存开关代际门禁，保障开关与清理的并发一致性。
- **clipboard_gate.rs**: 剪贴板监听、去重、划词模拟复制/粘贴与系统互斥锁（`CLIPBOARD_SIMULATION_MUTEX`）保护。
- **ocr.rs**: Windows 平台原生 `Windows.Media.Ocr` 硬件加速识别（非 Windows 平台安全返回未支持）。
- **proxy.rs**: Windows 注册表原生系统代理探测与自动/手动网络代理策略转换。
- **secrets.rs**: 封装 `keyring` 调用系统原生凭据管理器（Windows Credential Manager / macOS Keychain / Linux Secret Service），商业 API Key 与配对 Token 严格不落盘。
- **server**: 基于 `axum` 搭建的轻量本地 HTTP 服务（严格监听 `127.0.0.1:39281`），为浏览器插件提供无缝批量翻译接口、安全配对握手与 DNS Rebinding 防御。
- **storage.rs**: 应用数据目录解析（支持单文件免安装便携版 `portable` 模式）、漫游目录安全迁移。平台目录假设集中于此，`transly-core` 保持零平台假设。
- **capabilities**: Tauri 2 ACL 能力声明。应用自有命令不受 ACL 约束，但**插件命令（`plugin:*`）必须在此显式放行**，否则前端调用会被运行时静默拒绝（已放行 `core:event:default` 供浮窗与配对广播使用，`core:window:allow-show/hide/set-focus` 供窗口展示与失焦关闭使用）。

## 安全规则

1. 严禁明文落盘 API Key与通信 Token，统一由系统安全凭据管理器（Keyring）托管。
2. 本地 HTTP 服务严格绑定 `127.0.0.1` 环回接口，外网无法访问；独立校验 `Host` 头阻断 DNS Rebinding。
3. 严格遵循 CORS 来源检查，仅允许标准浏览器扩展 Origin 发起请求。
4. 严格执行并发限制（Semaphore）与防 DoS 校验（2MB 体积上限、50,000 字符上限）。

## 操作与测试指南

- **绑定生成**：运行 `pnpm run generate:bindings` 导出强类型接口。
- **本地构建**：运行 `pnpm run tauri build` 生成 release 二进制与安装包；单文件免安装便携版运行 `pnpm run build:portable`。
- **构建与调试**：本地开发用 `pnpm run tauri dev`；发版必须走 CLI（`pnpm run tauri build`）或 CI —— 它负责注入打包所需配置。切勿裸跑 `cargo build --release`，产物不含前端资源，启动后会去加载 `devUrl`。
- **单元与集成测试**：运行 `cargo test --workspace` 验证所有核心功能。

## 许可证

[MIT](../LICENSE)
