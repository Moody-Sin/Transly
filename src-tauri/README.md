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

- **commands**: 对外暴露的 Tauri command 强类型调用接入层。
- **secrets**: 封装 `keyring` 调用原生系统凭据管理器（Windows Credential Manager / macOS Keychain / Linux Secret Service）。
- **server**: 基于 `axum` 搭建的轻量本地 HTTP 服务（严格监听 `127.0.0.1`），为浏览器插件提供无缝批量翻译接口。
- **storage**: 应用数据目录解析（Tauri `identifier` 约定）与旧版缓存迁移。平台目录假设集中于此，`transly-core` 保持零平台假设。
- **capabilities**: Tauri 2 ACL 能力声明。应用自有命令不受 ACL 约束，但**插件命令（`plugin:*`）必须在此显式放行**，否则前端调用会被运行时静默拒绝（当前仅放行 `core:event:default`，供浮窗 `listen` 事件通道使用）。

## 安全规则

1. 严禁明文落盘 API Key与通信 Token，统一由系统安全凭据管理器（Keyring）托管。
2. 本地 HTTP 服务严格绑定 `127.0.0.1` 环回接口，外网无法访问。
3. 严格遵循 CORS 来源检查，仅允许标准浏览器扩展 Origin 发起请求。
4. 严格执行并发限制（Semaphore）与防 DoS 校验（2MB 体积上限、50,000 字符上限）。

## 操作与测试指南

- **绑定生成**：运行 `pnpm run generate:bindings` 导出强类型接口。
- **本地构建**：运行 `pnpm run tauri build` 生成 release 二进制与安装包。
- **构建与调试**：本地开发用 `pnpm run tauri dev`；发版必须走 CLI（`pnpm run tauri build`）或 CI —— 它负责注入打包所需配置。切勿裸跑 `cargo build --release`，产物不含前端资源，启动后会去加载 `devUrl`。
- **单元与集成测试**：运行 `cargo test --workspace` 验证所有核心功能。

## 许可证

[MIT](../../LICENSE)
