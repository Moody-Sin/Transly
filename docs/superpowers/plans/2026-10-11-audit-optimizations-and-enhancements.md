# 全面调研评审优化增强项实施计划 (Audit Optimizations & Enhancements Implementation Plan)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 落地全面调研评审发现的全部优化与缺陷修复，包括智能语言路由修正、划词原地替换缓存与引擎对齐、批翻译单项容错、浮窗拖拽与配对弹窗隔离、TTS 发音代码归一化、SQLite 并发锁超时及冷启动异步优化。

**Architecture:** 严格遵循架构规范：核心算法与容错逻辑在 `transly-core`，平台适配与后台维护在 `src-tauri`，UI 交互及无边框窗口拖拽在 `src` 前端；各层保持最小改动与严格类型对齐。

**Tech Stack:** Rust (Tokio, Rusqlite, Reqwest, Aho-Corasick), Tauri 2, React 19, TypeScript, Tailwind CSS, Chrome Extension MV3.

---

## 任务拆解与执行步骤

### Task 1: 核心算法与底层稳定性增强 (`transly-core`)

**Files:**
- Modify: `crates/transly-core/src/detector.rs`
- Modify: `crates/transly-core/src/glossary.rs`
- Modify: `crates/transly-core/src/engine/google.rs`
- Modify: `crates/transly-core/src/cache.rs`
- Modify: `crates/transly-core/src/ocr/mod.rs`
- Test: `crates/transly-core/src/detector.rs` (内置测试)
- Test: `crates/transly-core/src/glossary.rs` (内置测试)
- Test: `crates/transly-core/src/cache.rs` (内置测试)
- Test: `crates/transly-core/src/ocr/mod.rs` (内置测试)

- [ ] **Step 1: 编写/更新针对拉丁语系智能路由的单测用例**
  - 在 `crates/transly-core/src/detector.rs` 的 `tests` 模块中增加：当用户目标语言为 `en` 时，输入法文/德文不应强制转为 `zh-CN`。
- [ ] **Step 2: 优化 `detector.rs` 逻辑让测试通过**
  - 完善 `resolve_smart_target_lang`：仅当源文本被高置信度识别为中/日/韩/俄/阿时参与对转；纯拉丁字母且用户目标语言非中文时，尊重用户目标语言。
- [ ] **Step 3: 优化 `glossary.rs` 复合词组边界判定**
  - 增加针对多词短语（如 `"large language model"`）首尾字符独立检查单词边界的逻辑和测试。
- [ ] **Step 4: 为 `GoogleWebEngine` 增加标准浏览器 User-Agent 标头**
  - 在 `GoogleWebEngine::new()` 的 `ClientBuilder` 中配置浏览器 User-Agent，减少 429 风险。
- [ ] **Step 5: 为 `TranslationCache` 配置 SQLite `busy_timeout`**
  - 在 `cache.rs` 的 `writer` 与 `reader` 初始化批处理中加入 `PRAGMA busy_timeout = 5000;`。
- [ ] **Step 6: 扩充 OCR 候选语言识别包**
  - 在 `ocr/mod.rs` 中为 Windows 候选 OCR 列表补充 `"zh-Hans-CN"`、`"zh-CN"` 等，增强中文截图识别率。
- [ ] **Step 7: 运行 `cargo test -p transly-core` 验证测试全绿**

---

### Task 2: 后端平台适配与批量容错优化 (`src-tauri`)

**Files:**
- Modify: `crates/transly-core/src/engine/mod.rs`
- Modify: `crates/transly-core/src/engine/openai.rs`
- Modify: `crates/transly-core/src/engine/google.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/src/server/mod.rs`
- Test: `src-tauri/src/server/tests.rs`

- [ ] **Step 1: 批量翻译单项容错优化**
  - 改进 `OpenAIEngine::translate_batch` 与 `GoogleWebEngine::translate_batch`：逐项收集结果，捕获单项错误而不提前 `?` 退出，使正常项能正常返回并存入缓存。
- [ ] **Step 2: 划词原地替换集成缓存与用户配置**
  - 在 `src-tauri/src/commands/mod.rs` 的 `trigger_selection_replacement` 中：
    - 读取用户配置的首选引擎（优先于默认 Google 引擎）；
    - 翻译前先查 `TranslationCache`，命中时直接使用；
    - 翻译成功后写入 `TranslationCache`；
    - 增加替换失败时的告警与状态日志。
- [ ] **Step 3: `main.rs` 启动缓存清理后台异步化**
  - 将 `main.rs` 中 `cleanup_older_than_days` 与 `trim_to_max_items` 放入 `tauri::async_runtime::spawn_blocking` 中异步执行，减少主线程启动延迟。
- [ ] **Step 4: 运行 `cargo test --workspace` 确保所有 Rust 测试通过**

---

### Task 3: 前端交互与悬浮窗体验优化 (`src`)

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/HistoryPanel.tsx`
- Create: `src/utils/speech.ts`
- Test: `src/task/e2e.test.ts` / 前端类型检查

- [ ] **Step 1: 浮窗拖拽支持**
  - 在 `src/App.tsx` 的悬浮迷你窗（`isFloatWindow`）顶部 Header 元素上增加 `data-tauri-drag-region` 属性，使悬浮窗支持鼠标拖拽位移。
- [ ] **Step 2: 隔离扩展配对弹窗**
  - 在 `src/App.tsx` 的 `transly://pairing-requested` 事件监听器开头增加 `if (isFloatWindow) return;`，防止在迷你悬浮窗中重复弹出破版配对卡片。
- [ ] **Step 3: 提取统一 TTS 发音工具函数**
  - 创建 `src/utils/speech.ts` 中的 `speakText(text, lang, detectedLang)`，当 `lang === 'auto'` 时根据检测语言或文本特征规整为标准 BCP-47 代码，在 `App.tsx` 与 `HistoryPanel.tsx` 中复用。
- [ ] **Step 4: 运行 `pnpm run lint` 和 `pnpm run build` 确保前端构建无误**

---

### Task 4: 综合验证与更新日志维护

- [ ] **Step 1: 运行全量 CI 检查命令**
  - `cargo fmt --check`
  - `cargo clippy --workspace -- -D warnings`
  - `cargo test --workspace`
  - `pnpm run lint`
  - `pnpm run test:task`
  - `pnpm run build`
  - `pnpm run build:extension`
- [ ] **Step 2: 记录 `Lessons Learned.md` 与更新 `CHANGELOG.md`**
  - 维护 CHANGELOG.md 变更记录；若有深层次架构踩坑提炼至 `Lessons Learned.md`。
- [ ] **Step 3: 提交并向老大汇报实施成果**
