# Transly 项目智能体指引

本文件包含 Transly 项目的核心工程规范与架构边界，所有 AI 智能体与维护者必须严格遵循。

## 核心架构原则

1. **严格分层与无状态核心**：
   - 翻译、缓存、OCR、术语库、语言检测核心逻辑必须在 `crates/transly-core`（纯 Rust，禁止引入 Tauri 依赖）。
   - `src-tauri` 作为平台适配宿主层，仅负责 Tauri 命令薄封装、系统原生凭据（`keyring`）、系统托盘与全局快捷键。
   - `src` 前端只负责 UI 与交互，严禁在前端直接发起任何翻译网络请求，必须通过 `tauri-specta` 生成的强类型绑定调用后端。
   - `extension` 浏览器插件为“瘦客户端”，主打**整页翻译仅译文**（原地 DOM 替换、保真排版），插件端绝不存 API Key，所有请求均通过 `127.0.0.1:39281` 本地鉴权服务走桌面端统一调度。

2. **安全第一**：
   - 所有外部商业翻译 API Key（DeepL / OpenAI / DeepSeek）必须托管于系统原生凭据管理器（Windows Credential Manager / macOS Keychain / Linux Secret Service），严禁明文落盘。
   - 本地 HTTP 服务严格绑定 `127.0.0.1`，并强制 Bearer Token 强鉴权与扩展专用 CORS 白名单。
   - 前端配置严格内容安全策略 (CSP)。

3. **工程质量与 CI**：
   - 提交前必须通过：`cargo fmt --check` + `cargo clippy --workspace -- -D warnings` + `cargo test --workspace` + `pnpm run lint` + `pnpm run build`。
   - 严禁提交密钥、大型离线二进制模型文件与 `Cargo.lock` 忽略项。

