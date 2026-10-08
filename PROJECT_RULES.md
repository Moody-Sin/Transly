# Transly 项目规则

## 项目概述

项目定位：Transly 是一款基于 Rust 的跨平台桌面翻译软件，利用 Rust 的高性能与内存安全特性，提供轻量、快速、安全的翻译体验。

目标平台：
- 桌面端：Windows / macOS / Linux
- 浏览器插件端：Chromium 内核浏览器（Chrome, Edge 等），基于原生 TypeScript + Vite 构建
- 未来阶段：iOS / Android（移动端，架构预留，暂不实现）

核心原则：
- 翻译核心逻辑与平台无关，可复用、可测试
- 平台相关代码严格隔离
- 密钥安全第一，绝不落盘明文
- 桌面端与浏览器插件共享同一套核心逻辑

---

## 技术栈与架构规则

### 技术栈
- 桌面框架：Tauri 2
- 后端：Rust（edition 2021+）
- 前端：React + TypeScript（strict 模式）+ Tailwind CSS
- 异步运行时：tokio
- 本地存储：SQLite（`rusqlite`，WAL 模式）
- 密钥存储：系统原生凭据管理器（Windows Credential Manager / macOS Keychain / Linux Secret Service，使用 `keyring` crate）
- IPC 类型同步：`tauri-specta`（自动生成 TS 类型与强类型调用包装）
- Git Hooks：`lefthook`

### 架构分层

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
│  - 缓存 / 术语库               │
│  - 可被桌面端、CLI、WASM 复用             │
└─────────────────────────────────────────┘
```

### 硬性架构规则
1. 核心逻辑必须放在 `transly-core` crate，`src-tauri` 只做平台适配与 command 封装。
2. 前端不直接发翻译请求，一律走 Rust IPC。
3. 浏览器插件通过桌面端本地 HTTP 服务调用翻译，不重复实现引擎。
4. 新增翻译引擎只需实现 `transly-core` 中的 engine trait，不改动其他层。
5. 禁止过度设计：不为未立项的平台（移动端）预留空目录或冗余抽象。

---

## 代码规范

### Rust 侧
- 强制 `cargo fmt`，CI 校验
- 强制 `cargo clippy -- -D warnings`，CI 校验
- 错误处理：
  - 库层（transly-core）：`thiserror` 定义结构化错误
  - 应用层（src-tauri）：`anyhow` 聚合上下文
- 异步统一 `tokio`，Tauri command 内禁止阻塞调用
- 命名：模块/文件 `snake_case`，类型 `PascalCase`，常量 `SCREAMING_SNAKE_CASE`
- 禁止 `unwrap()` / `expect()` 出现在非测试代码（除非已证明不可能失败并注释说明）

### 前端侧
- ESLint + Prettier
- TypeScript `strict: true`，禁止 `any`
- 组件命名 `PascalCase`，hooks `useXxx`
- 与 Rust 通信使用 `tauri-specta` 自动生成的 client，禁止手写 IPC 调用

### 通用与 Git Hooks
- 提交前通过 `lefthook` 自动校验：`cargo fmt --check` + `cargo clippy` + `tsc --noEmit` + `eslint` + `pnpm run test:task`

---

## 目录结构

```
transly/
├── crates/
│   └── transly-core/          # 核心逻辑，无 Tauri 依赖
│       ├── src/
│       │   ├── lib.rs
│       │   ├── engine/        # 翻译引擎抽象与实现 (DeepL, OpenAI, Google...)
│       │   ├── ocr/           # OCR 辅助运算与 DTO
│       │   ├── cache.rs       # SQLite 缓存管理
│       │   ├── glossary.rs    # 术语库
│       │   ├── config.rs      # 配置模型
│       │   └── error.rs
│       └── Cargo.toml
│
├── src-tauri/                 # Tauri 桌面应用与平台集成
│   ├── src/
│   │   ├── main.rs            # 应用入口：状态挂载、托盘、快捷键、剪贴板监听任务
│   │   ├── lib.rs             # 模块声明与 app_command_builder! 命令注册宏
│   │   ├── commands/          # Tauri command 入口（薄层）
│   │   ├── server/            # 本地 HTTP 服务（供浏览器插件调用）+ tests.rs
│   │   ├── engine_pool.rs     # 引擎实例池（读写锁 + spawn_blocking 凭据读取）
│   │   ├── cache_controller.rs# 缓存开关 generation 门禁
│   │   ├── clipboard_gate.rs  # 剪贴板去重与划词抑制判定（纯函数）
│   │   ├── ocr.rs             # 平台原生 OCR（Windows WinRT，其余平台返回未支持）
│   │   └── secrets.rs         # 密钥读写（系统凭据管理器）
│   ├── capabilities/          # Tauri 2 ACL 能力声明（前端插件命令最小放行白名单）
│   ├── bin/export_bindings.rs # 导出 tauri-specta 强类型绑定
│   ├── Cargo.toml
│   └── tauri.conf.json
│
├── src/                       # 桌面端前端 (React + TS)
│   ├── App.tsx                # 主窗口与浮窗（#float）视图、设置中心
│   ├── task/                  # 共享任务状态机、代际隔离、useTaskRunner 钩子
│   ├── i18n/                  # zh-CN.json / en.json
│   └── bindings.ts            # tauri-specta 自动生成的类型与 API
│
├── extension/                 # 浏览器插件 (Vite + 原生 TypeScript)
│   ├── src/
│   │   ├── background/        # 后台通信（负责与桌面端 127.0.0.1 HTTP 通信）
│   │   ├── content/           # 页面 DOM 原地提取与替换
│   │   ├── i18n/              # chrome.i18n 查询封装与静态文案注入
│   │   └── popup/             # 插件工具栏弹窗 UI
│   ├── _locales/              # Chrome 原生 i18n 文案（zh_CN / en）
│   ├── manifest.json
│   └── package.json
│
├── docs/                      # 架构决策与跨端边界说明（索引见 docs/README.md）
│
├── .github/workflows/         # CI
├── lefthook.yml               # 本地 Git 提交钩子
└── Cargo.toml                 # Workspace 根配置
```

---

## 功能实现

### 翻译交互与实现方案
- **选中文本翻译**：以“全局快捷键触发”为基准。逻辑为：用户选中文本 $\to$ 触发全局快捷键 $\to$ 模拟按键复制到剪贴板 $\to$ Rust 读取并恢复旧剪贴板内容 $\to$ 弹出浮窗。避免高侵入、高权限风险的全局鼠标监听。
- **输入框即时翻译**：主窗口与常驻快捷浮窗内即输即译。
- **剪贴板监听翻译**：用户可选开启，监听系统剪贴板变动，检测到文本自动弹窗/通知。
- **截图 OCR 翻译**：
  - 调用系统截图/原生捕获能力获取图片。
  - OCR 引擎同样抽象为 trait，优先接入云端 API 或系统原生 OCR（Windows Media OCR / macOS Vision），不将庞大离线模型打入主程序包。

### 翻译引擎
- 可插拔设计，每个引擎实现统一 `TranslateEngine` trait
- 支持多引擎聚合 / 对比
- 支持引擎降级（主引擎失败自动切换备用）

### 缓存策略
- 单层 SQLite 持久化缓存（WAL 模式 + hash 索引），免除双层缓存一致性负担
- 缓存 key：`hash(text + source_lang + target_lang + engine)`
- 支持按条目上限与保留天数自动清理

### 离线能力（规划）
- 评估本地模型方案（CTranslate2 / ONNX）
- 作为可选引擎接入，不阻塞主流程

### 平台集成
- 全局快捷键：`tauri-plugin-global-shortcut`
- 剪贴板：`tauri-plugin-clipboard-manager`
- 截图：`tauri-plugin-screenshot` 或自研（平台差异大，隔离在 platform 层）
- 系统托盘：Tauri 2 内置

### 多语言与国际化
- 目前仅做简体中文与英文
- 前端 i18n 文件放 `src/i18n/`
- 语言包 key 命名统一，禁止硬编码文案

### 浏览器插件协作与同步实现
- **核心定位与主目标**：与桌面端同步开发的一等公民。**主目标是“整页翻译仅译文”**——极简、低干扰、高保真。类似原生浏览器翻译，用目标语言译文原地直接替换网页原始文本（仅译文模式），绝不做破坏排版的双语混排、不堆砌词典与花哨分析。
- **关键技术实现**：
  - **原地 DOM 替换**：精确定位文本节点，直接用译文替换原文内容，保留原有 CSS 样式与页面排版，并支持一键无感切换/还原原文。
  - **批量合并翻译（Batch Translation）**：遍历 DOM 聚合可视区域文本块，批量打包送至桌面端接口，降低网络和 IPC 开销。
  - **动态增量监听（MutationObserver）**：监听 SPA 单页应用与瀑布流内容的动态加载，自动按需补译。
- **架构模式**：极简瘦客户端。插件端**不存 API 密钥**、**不直连外网接口**：
  - 页面 DOM 提取 $\to$ 批量发送至本地桌面端 HTTP 服务 (`127.0.0.1`) $\to$ `transly-core`（走共享缓存与配置） $\to$ 返回译文数组 $\to$ 原地更新 DOM。
- **通信与鉴权**：基于 `axum` 的轻量 HTTP 服务，强制 Token 鉴权，防止恶意网站探测。

---

## 安全规则

1. 密钥只存系统凭据管理器（Windows Credential Manager / macOS Keychain / Linux Secret Service），禁止明文落盘、禁止进数据库、禁止进 Git。
2. 前端绝不接触 API 密钥，所有网络翻译请求一律在 Rust 侧发起。
3. 本地 HTTP 服务（供浏览器插件调用）必须强制绑定 `127.0.0.1`，并强制 Token 鉴权。
4. 禁止提交密钥、大文件、`node_modules`、target 构建产物。
5. CI 中强制执行 Secret 扫描（gitleaks）。

---

## 测试

- transly-core 单测覆盖率要求 ≥70%
- 引擎测试使用 mock，CI 不真调外部 API
- 关键路径集成测试放 src-tauri/tests
- 前端组件测试（可选，视复杂度）

---

## Git 与协作流程

### 分支模型
- `main`：稳定分支，只接受 release 与 hotfix 合并
- `dev`：开发主分支
- `feature/*`：功能分支，从 dev 切出
- `hotfix/*`：紧急修复，从 main 切出

### 提交信息规范（Conventional Commits）
- `feat: 添加划词翻译`
- `fix: 修复截图识别乱码`
- `refactor: 重构引擎 trait`
- `docs: 更新项目规则`
- `chore: 升级依赖`

### PR 规则
- 标题清晰，描述变更内容与测试方式
- 关联 issue 编号
- 必须通过 CI（fmt + clippy + test + TS 类型同步 + secret 扫描）
- 禁止提交大文件、密钥、`node_modules`

### 版本号
- 遵循 SemVer（主版本.次版本.修订号）

### 发布流程
1. 打 tag
2. CI 构建
3. 生成安装包
4. 写 Release Notes

---

## CI 检查清单（必须全部通过）

- [ ] `cargo fmt --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` (Linux + Windows 矩阵)
- [ ] `cargo test` (Linux + Windows 矩阵)
- [ ] `transly-core` 行覆盖率 ≥70%（`cargo llvm-cov --package transly-core --fail-under-lines 70`）
- [ ] 前端 `eslint` + `tsc --noEmit`（包含桌面端与插件端）
- [ ] 前端任务单测（`pnpm run test:task`）
- [ ] IPC TS 类型与 Rust 生成结果一致
- [ ] secret 扫描（gitleaks）
- [ ] 禁止提交文件检查（大文件、node_modules）