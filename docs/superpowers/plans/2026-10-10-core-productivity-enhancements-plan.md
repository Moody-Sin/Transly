# 桌面端与核心生产力质跃包实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 落地 Transly 桌面端与核心生产力增强特性，包括 Markdown 代码块保护、LLM 流式请求物理中断取消、全局快捷键智能录制器、浮窗图钉置顶与全界面深色模式。

**Architecture:** 
1. 在 `transly-core` 增加轻量级纯 Rust 文本预处理与还原模块 `preserve.rs`，在翻译前用专有占位符替换代码块并在翻译后精确还原；
2. 在 `src-tauri` 增加流式任务取消注册表与 `cancel_stream_task` 命令，并在 `main.rs` 中为 `float` 窗口打通图钉持久粘滞控制；
3. 在 `src` 前端实现 `ShortcutRecorder` 组件替代手动输入框，悬浮窗接入图钉按钮，Tailwind 接入 `darkMode: 'class'` 实现深色/浅色/系统主题自适应。

**Tech Stack:** Rust (transly-core, tauri 2, tokio, regex), React 19, TypeScript, Tailwind CSS, Lucide icons.

**Spec:** `docs/superpowers/specs/2026-10-10-core-productivity-enhancements-design.md`

## Global Constraints

- `transly-core` 保持纯 Rust 与零 Tauri 依赖，不引入沉重的大型 Markdown AST 编译器。
- 遵循 Lessons Learned 第 1 条：IPC DTO 禁止使用 `BigInt`（如 `u64`/`i64`），统一使用 `u32`/字符串。
- 遵循 Lessons Learned 第 2 条：Tauri 运行时状态注入使用强类型包装元组，严禁重复 `manage` 原生基础类型。
- 遵循约定式提交与中英语言包完全对称。
- 提交前必须通过：`pnpm run lint` + `pnpm run format:check` + `pnpm run test:task` + `pnpm run build`。

## Review Focus

1. **复杂代码块与嵌套反引号保真**：输入包含多行 ```` ```rust ... ``` ```` 与行内 `` `foo` `` 混合文本时，占位与回填必须 100% 保持原有代码、缩进与反引号不被引擎污染。
2. **流式取消幂等与无泄漏**：对已经完成或从未注册的任务调用 `cancel_stream_task` 必须幂等安全返回，且取消后任务注册表不应常驻泄漏。
3. **快捷键录制组合键冲突与 Escape 退出**：录制过程中按下 Escape 必须取消录制并恢复原值，按下单一修饰键（如仅按 `Ctrl`）不得误认定为有效快捷键。
4. **深色模式下所有容器与文字对比度**：在 dark 模式下，主输入框、结果框、历史记录面板、设置中心模态框底色与文字颜色必须对比度分明，无隐形文字。
5. **浮窗图钉状态切换不闪退**：点击图钉切换钉住/取消钉住状态时，窗口不能发生丢失焦点瞬间闪退。

---

### Task 1: transly-core Markdown 代码块智能提取与还原模块

**Files:**
- Create: `crates/transly-core/src/preserve.rs`
- Modify: `crates/transly-core/src/lib.rs`
- Test: `crates/transly-core/src/preserve.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct PreservedText {
      pub text: String,
      pub stash: Vec<String>,
  }
  pub fn stash_code_blocks(input: &str) -> PreservedText;
  pub fn restore_code_blocks(input: &str, stash: &[String]) -> String;
  ```

- [ ] **Step 1: Write failing tests in `crates/transly-core/src/preserve.rs`**
- [ ] **Step 2: Implement `stash_code_blocks` and `restore_code_blocks` with Unicode-safe placeholders**
- [ ] **Step 3: Export in `crates/transly-core/src/lib.rs`**
- [ ] **Step 4: Integrate into `translate` and `translate_stream` workflows**
- [ ] **Step 5: Verify tests and commit**

---

### Task 2: 后端流式翻译请求中断取消机制 (E2)

**Files:**
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/main.rs`
- Modify: `src/bindings.ts`

**Interfaces:**
- Produces:
  ```rust
  #[tauri::command]
  pub async fn cancel_stream_task(task_id: u32, ...) -> Result<(), String>;
  ```

- [ ] **Step 1: Implement `StreamTaskRegistry` with tokio broadcast/watch cancel triggers in `src-tauri/src/commands/mod.rs`**
- [ ] **Step 2: Wrap active `translate_stream` execution with `tokio::select!` cancellation gate**
- [ ] **Step 3: Register `cancel_stream_task` in `app_command_builder!` and update bindings**
- [ ] **Step 4: Hook into frontend `useTaskRunner.cancel()` and new generation triggers**
- [ ] **Step 5: Verify and commit**

---

### Task 3: 全局快捷键智能录制器组件 (E6)

**Files:**
- Create: `src/ShortcutRecorder.tsx`
- Modify: `src/App.tsx`
- Modify: `src/i18n/zh-CN.json`
- Modify: `src/i18n/en.json`

**Interfaces:**
- Produces:
  ```tsx
  export interface ShortcutRecorderProps {
    value: string;
    onChange: (value: string) => void;
    onSave: () => void;
    t: I18nSchema;
  }
  ```

- [ ] **Step 1: Create `src/ShortcutRecorder.tsx` with robust keyboard event normalization**
- [ ] **Step 2: Replace manual text inputs in `src/App.tsx` for both selection & replace shortcuts**
- [ ] **Step 3: Add symmetrical i18n keys for recorder status and actions**
- [ ] **Step 4: Verify with `pnpm run test:task` and `pnpm run build`**
- [ ] **Step 5: Commit**

---

### Task 4: 划词悬浮窗图钉（Pin）置顶与交互优化 (E7)

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/i18n/zh-CN.json`
- Modify: `src/i18n/en.json`

**Interfaces:**
- Consumes: `commands.setFloatSticky(pinned: boolean)`

- [ ] **Step 1: Add Pin button and state in `#float` header in `src/App.tsx`**
- [ ] **Step 2: Sync pin state with `setFloatSticky` to keep window visible on blur when pinned**
- [ ] **Step 3: Add tooltips and visual indication (active pin color)**
- [ ] **Step 4: Verify and commit**

---

### Task 5: 桌面端全界面深色模式适配 (E5)

**Files:**
- Modify: `tailwind.config.js`
- Modify: `src/App.tsx`
- Modify: `src/HistoryPanel.tsx`
- Modify: `src/ShortcutRecorder.tsx`
- Modify: `src/i18n/zh-CN.json`
- Modify: `src/i18n/en.json`

- [ ] **Step 1: Enable `darkMode: 'class'` in `tailwind.config.js`**
- [ ] **Step 2: Add theme state (`light` | `dark` | `system`) with localStorage persistence and DOM class sync**
- [ ] **Step 3: Add dark-mode classes across App, HistoryPanel, and settings modals**
- [ ] **Step 4: Add symmetrical i18n keys for theme options**
- [ ] **Step 5: Run full test and lint pipeline and commit**
