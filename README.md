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

### 预编译安装包

从 [Releases](https://github.com/Moody-Sin/Transly/releases) 页面下载对应操作系统的安装包：
- **Windows**: `Transly_0.1.0_x64-setup.exe` 或 `.msi`
- **macOS**: `Transly_0.1.0_aarch64.dmg`
- **Linux**: `Transly_0.1.0_amd64.deb` 或 `.AppImage`
- **浏览器扩展**: 下载 `transly-extension-v0.1.0.zip` 并解压。

### 源码开发构建

需要满足以下开发环境要求：
- [Node.js](https://nodejs.org/) (>= 22.6) 与 [pnpm](https://pnpm.io/)
- [Rust](https://www.rust-lang.org/) (edition 2021+)
- 目标平台对应的 C/C++ 构建工具链（如 Windows 上的 MSVC，Linux 上的 build-essential 和 WebKitGTK 依赖等）

克隆代码并安装依赖：

```sh
git clone https://github.com/Moody-Sin/Transly.git
cd transly
pnpm install
```

## 用法

### 1. 桌面端使用
启动桌面端开发模式或安装包：

```sh
pnpm tauri dev
```

- **快捷悬浮翻译**：选中文本按下 `Alt+Shift+D`，自动唤起轻量悬浮窗并即时翻译。
- **剪贴板监听**：在主界面开启“剪贴板监听”，复制文本即可自动翻译。
- **独立术语库**：在“术语库”选项卡添加自定义词条（如 `Agent -> 智能体`），全局强制优先精确匹配。
- **缓存与隐私**：可在设置中开启或关闭“持久化翻译缓存”。关闭时系统将自动清空磁盘翻译历史，且不影响术语库。

### 2. 浏览器插件使用与配对
1. 打开 Chrome / Edge 扩展管理页面（`chrome://extensions`），开启“开发者模式”。
2. 点击“加载已解压的扩展程序”，选择 `extension/dist`（或解压后的插件包目录）。
3. 在 Transly 桌面端设置中复制内部配对 Token。
4. 打开浏览器插件 Popup，粘贴 Token 并点击“保存配对 Token”，连接状态将变为“● 已连接且配对有效”。
5. 点击“翻译此网页 (仅译文)”，即可享受原地无缝 DOM 替换翻译；点击“还原原文”瞬间恢复。
6. **Token 轮换**：若在桌面端点击“重新生成扩展配对 Token”，旧 Token 将即时失效（401 拦截），在扩展重新粘贴新 Token 即可恢复。

### 3. 已知限制与注意事项
- **复杂富文本编辑器**：为保障页面编辑安全，Transly 默认不对 `contenteditable`、富文本代码编辑器（如 Monaco / CodeMirror）及包含 `.notranslate` 属性的节点进行强行替换。
- **图片 OCR 翻译 (Windows 原生)**：支持主输入框底栏「识别剪贴板图片」入口或 `ocr_clipboard_image` 命令。Windows 平台基于系统 `Windows.Media.Ocr` 原生硬件加速识别并自动触发即时翻译，全流程内存零落盘。非 Windows 平台调用返回明确的未支持错误。
- **外部 API 差异**：DeepL、OpenAI 及 DeepSeek 需在设置中配置有效 API Key（安全托管于系统凭据管理器中，绝不上传第三方服务器）。

## 维护者

[@Moody-Sin](https://github.com/Moody-Sin)

## 贡献

欢迎提交 PR 或创建 Issue！请遵循 Conventional Commits 约定式提交规范。

## 许可证

[MIT](LICENSE)
