# 移除自动降级功能实现计划 (Remove Auto-Fallback Implementation Plan)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 彻底移除跨翻译引擎的自动降级（Fallback）功能，使各引擎职责单一、直接报错，消除静默降级、状态割裂与双重连接池缓存。

**Architecture:** 从 `crates/transly-core` 中删除 `FallbackEngine` 及能力字段，简化缓存查询为单引擎直查；在 `src-tauri` 中解绑 `EnginePool` 对降级包装器的依赖并合并双重缓存为单一缓存；清理划词替换中的防御代码；收敛 UI 语言包与文档。

**Tech Stack:** Rust (Tokio, Tauri 2, Axum), TypeScript, React, i18n JSON.

**Spec:** `docs/superpowers/specs/2026-10-12-remove-auto-fallback-design.md`

## Global Constraints

- 全程使用简体中文，称呼用户为老大。
- 遵循外科手术式修改原则与简单至上原则，绝不添加未经要求的“灵活性”或投机代码。
- 不影响非跨引擎的内部回退机制（如便携目录回退、富文本占位符丢失回退、单字符符号键回退）。
- 提交前必须通过：`cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace` + `pnpm run lint` + `pnpm run build`。

## Review Focus

1. 引擎调用失败时直接返回对应引擎的原始错误，不再静默调用 Google Web 引擎。
2. 缓存查询 `lookup_cache_with_fallback` 与 `lookup_batch_with_fallback` 仅查询当前引擎缓存，杜绝跨引擎命中备用引擎历史缓存。
3. `EnginePool` 仅维护单一 `cache`，`get_engine` 与 `get_raw_engine` 均返回原生引擎，且在 `reload_proxy` 与 `invalidate` 时行为完全一致。
4. 原地划词替换在首选引擎失败时继续触发报错与 Toast 告警，并保护系统剪贴板。
5. 前端中英文语言包键结构 100% 对齐，去除了 `(自动降级)` 与 `(auto fallback)`。

---

### Task 1: 核心库删除 FallbackEngine 与精简 EngineCapabilities

**Files:**
- Delete: `crates/transly-core/src/engine/fallback.rs`
- Modify: `crates/transly-core/src/engine/mod.rs`
- Modify: `crates/transly-core/src/lib.rs`
- Modify: `crates/transly-core/src/engine/google.rs`
- Modify: `crates/transly-core/src/engine/deepl.rs`
- Modify: `crates/transly-core/src/engine/openai.rs`

**Interfaces:**
- Consumes: `TranslateEngine`, `EngineCapabilities`
- Produces: `EngineCapabilities` (已移除 `fallback_engine: Option<String>`)

- [ ] **Step 1: 删除 `fallback.rs` 并在 `mod.rs` 和 `lib.rs` 中取消模块引入与导出**
- [ ] **Step 2: 从 `EngineCapabilities` 结构体及各引擎实现（google, deepl, openai）中移除 `fallback_engine` 字段**
- [ ] **Step 3: 运行 `cargo check -p transly-core` 验证编译**
- [ ] **Step 4: 运行 `cargo test -p transly-core` 验证 transly-core 测试编译与运行**

---

### Task 2: 核心库缓存查询精简

**Files:**
- Modify: `crates/transly-core/src/engine/mod.rs`

**Interfaces:**
- Consumes: `TranslationCache`, `TranslateEngine`, `BatchItemRequest`
- Produces: `lookup_cache_with_fallback`, `lookup_batch_with_fallback` (仅查主引擎缓存)

- [ ] **Step 1: 编写/更新针对单引擎缓存查询的测试（验证不再跨引擎命中）**
- [ ] **Step 2: 简化 `lookup_cache_with_fallback`：仅查询 `engine.id()`，移除备用引擎逻辑**
- [ ] **Step 3: 简化 `lookup_batch_with_fallback`：仅查询 `engine.id()` 的 primary_keys，移除 `missing_fallback_keys` 二级查询逻辑**
- [ ] **Step 4: 运行 `cargo test -p transly-core -- test_lookup_batch_with_fallback` 验证测试通过**

---

### Task 3: 桌面宿主层 EnginePool 极简化与划词防御清理

**Files:**
- Modify: `src-tauri/src/engine_pool.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/server/tests.rs`

**Interfaces:**
- Consumes: `EnginePool`
- Produces: `EnginePool::get_engine`, `EnginePool::get_raw_engine` (统一返回原生引擎，内部单缓存)

- [ ] **Step 1: 修改 `src-tauri/src/engine_pool.rs`：删除 `raw_cache`，`create_engine_blocking` 直接返回原生引擎，`get_raw_engine` 委托 `get_engine`**
- [ ] **Step 2: 修改 `src-tauri/src/commands/mod.rs`：移除原地替换中的 `(fallback)` 防御性分支**
- [ ] **Step 3: 重构 `src-tauri/src/server/tests.rs` 中的 `test_fallback_cache_hit_on_secondary` 为直接单引擎缓存测试，或更新断言**
- [ ] **Step 4: 运行 `cargo test -p src-tauri` 验证桌面端测试通过**

---

### Task 4: 前端语言包更新与无意义提示收敛

**Files:**
- Modify: `src/i18n/zh-CN.json`
- Modify: `src/i18n/en.json`

**Interfaces:**
- Consumes: JSON i18n
- Produces: 更新后的中英文翻译文案

- [ ] **Step 1: 修改 `zh-CN.json`：将 `"DeepL (需API Key / 自动降级)"` 改为 `"DeepL (需API Key)"`，OpenAI 与 DeepSeek 同理**
- [ ] **Step 2: 修改 `en.json`：将 `"DeepL (API key required / auto fallback)"` 改为 `"DeepL (API key required)"`，OpenAI 与 DeepSeek 同理**
- [ ] **Step 3: 运行 `pnpm run test:task` 验证语言包对称性测试通过**

---

### Task 5: 文档同步与全工作区验证

**Files:**
- Modify: `PROJECT_RULES.md`
- Modify: `crates/transly-core/README.md`
- Modify: `crates/transly-core/src/engine/README.md`
- Modify: `CHANGELOG.md`

- [ ] **Step 1: 更新各级 README 与项目规范文档，删除关于 FallbackEngine 自动降级的描述**
- [ ] **Step 2: 在 `CHANGELOG.md` 的 `[Unreleased]` 中记录移除自动降级功能的变更**
- [ ] **Step 3: 运行全量验证命令套件：`cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace` + `pnpm run lint` + `pnpm run build`**
