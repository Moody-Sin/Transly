# Transly

> 基于 Rust 的高性能跨平台桌面翻译与浏览器整页沉浸式翻译工具。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

Transly 是一款基于 Rust 与 Tauri 2 构建的跨平台桌面翻译软件，拥有极低内存开销和毫秒级响应。同时配备深度协作的浏览器插件，主打“整页翻译仅译文”的极简沉浸式阅读体验。

## 目录

- [背景](#背景)
- [架构](#架构)
- [安装](#安装)
- [用法](#用法)
- [维护者](#维护者)
- [贡献](#贡献)
- [许可证](#许可证)

## 背景

许多传统翻译软件常伴随高内存占用、臃肿功能（不必要的社交、生词本背诵推送）以及破坏网页原有排版的双语排版。Transly 聚焦于核心翻译诉求：
1. **轻量极速**：Rust 核心逻辑，极简系统资源占用。
2. **纯粹沉浸**：浏览器插件主打“仅显示目标语言译文”，原地替换 DOM 文本节点，完全保真原有样式排版。
3. **安全透明**：所有密钥托管于系统原生凭据管理器，明文不落盘，前端与外部网页零接触 API Key。

## 架构

```
┌─────────────────────────────────────────┐
│  前端 (React + TS + Tailwind)           │
│  - 只负责 UI 与交互                       │
│  - 不直接发起翻译请求                     │
└───────────────┬─────────────────────────┘
                │ Tauri IPC（tauri-specta 强类型调用）
┌───────────────▼─────────────────────────┐
│  src-tauri（平台适配层）                  │
│  - Tauri command 薄封装                   │
│  - 平台集成：快捷键/剪贴板/截图/托盘       │
│  - 本地 HTTP 服务（供浏览器插件调用）      │
└───────────────┬─────────────────────────┘
                │ 纯 Rust 调用
┌───────────────▼─────────────────────────┐
│  transly-core（核心逻辑，无 Tauri 依赖）  │
│  - 翻译与 OCR 引擎抽象与实现              │
│  - 缓存 / 语言检测 / 术语库               │
│  - 可被桌面端、CLI、WASM 复用             │
└─────────────────────────────────────────┘
```

## 安装

需要满足以下开发环境要求：
- [Node.js](https://nodejs.org/) (>= 18) 与 [pnpm](https://pnpm.io/)
- [Rust](https://www.rust-lang.org/) (edition 2021+)
- 目标平台对应的 C/C++ 构建工具链（如 Windows 上的 MSVC，Linux 上的 build-essential 和 WebKitGTK 依赖等）

克隆代码并安装依赖：

```sh
git clone https://github.com/transly/transly.git
cd transly
pnpm install
```

## 用法

启动桌面端开发模式：

```sh
pnpm tauri dev
```

启动浏览器插件开发模式：

```sh
pnpm --filter extension dev
```

## 维护者

[@transly](https://github.com/transly)

## 贡献

欢迎提交 PR 或创建 Issue！请遵循 Conventional Commits 约定式提交规范。

## 许可证

[MIT](LICENSE)
