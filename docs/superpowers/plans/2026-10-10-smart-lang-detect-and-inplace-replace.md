# 离线源语言自动检测与划词原地替换实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 Transly 实现极速零网络源语言探测与中英智能双向互转，并落地全局快捷键“划词原地翻译并替换”生产力工作流。

**Architecture:** 
1. 在 `transly-core` 实现基于 Unicode 脚本分布的轻量无依赖语言检测器 `detect_language` 与智能目标语言推断器 `resolve_smart_target_lang`，保持纯 Rust 与微秒级开销。
2. 在 `src-tauri` 增加 `detect_language` 强类型 IPC 命令与 `trigger_selection_replacement` 流程控制（模拟复制 -> 异步翻译 -> 模拟粘贴覆盖 -> 恢复原剪贴板），并注册 `Alt+Shift+R` 全局热键。
3. 在 `src` 前端及浮窗任务调度中打通智能双向互转开关与替换快捷键设置面板，保持代际安全与状态机一致性。

**Tech Stack:** Rust (transly-core, tauri 2, enigo, specta/tauri-specta), React 19, TypeScript, Tailwind CSS.

**Spec:** 基于本次全面调研评审的第一阶段落地项规范（P0 级：智能源语言检测互转与划词原地替换）。

## Global Constraints

- `transly-core` 必须保持纯 Rust 且零 Tauri 依赖，禁止引入庞大模型文件或重型外部机器学习库。
- 遵循 Lessons Learned 第 1 条：Specta 导出禁止使用 `BigInt`（如 `u64`/`i64`），IPC DTO 统一使用 `u32`/字符串。
- 遵循 Lessons Learned 第 4 & 6 条：系统凭据与平台阻塞 I/O 必须在 `spawn_blocking` 中执行，Tauri command 保持 `async fn`。
- 遵循 Lessons Learned 第 2 条：全局状态注入使用强类型包装元组，禁止重复 `manage` 相同原生类型。
- 提交前必须通过：`cargo fmt --check` + `cargo clippy --workspace -- -D warnings` + `cargo test --workspace` + `pnpm run lint` + `pnpm run build`。

## Review Focus

1. **混合多语言文本检测**：对于中英夹杂文本（如 "使用 Rust 开发"），应依据字符权重准确判定主要语系，不应误判或崩溃。
2. **空文本与纯标点符号边界**：对空串、纯空白或纯符号输入，语言检测器必须安全返回 `None` 或兜底默认值。
3. **划词替换中的剪贴板保护**：原地粘贴替换后必须妥善恢复用户原有剪贴板历史，且模拟复制粘贴期间必须置 `selection_flag` 抑制剪贴板监听器自触发。
4. **只读文本或无选区容错**：在无法复制或无法粘贴的窗口按下替换快捷键时，流程必须安全超时退出，不锁死剪贴板或崩溃。
5. **前后端类型契约自动生成**：新增命令必须在 `app_command_builder!` 中注册并执行 `export_bindings`，保证 `src/bindings.ts` 强类型同步。

---

### Task 1: transly-core 离线语言检测与智能对转器

**Files:**
- Create: `crates/transly-core/src/detector.rs`
- Modify: `crates/transly-core/src/lib.rs:1-26`
- Modify: `crates/transly-core/src/validation.rs`
- Test: `crates/transly-core/src/detector.rs`

**Interfaces:**
- Produces:
  ```rust
  pub fn detect_language(text: &str) -> Option<&'static str>;
  pub fn resolve_smart_target_lang(source_text: &str, user_target_lang: &str) -> String;
  ```

- [ ] **Step 1: Write the failing tests in `crates/transly-core/src/detector.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_language_chinese() {
        assert_eq!(detect_language("你好，世界！这是一个测试"), Some("zh-CN"));
        assert_eq!(detect_language("使用 Rust 编写高性能软件"), Some("zh-CN"));
    }

    #[test]
    fn test_detect_language_english() {
        assert_eq!(detect_language("Hello world, this is a test."), Some("en"));
        assert_eq!(detect_language("Fast and lightweight translation tool"), Some("en"));
    }

    #[test]
    fn test_detect_language_japanese_and_korean() {
        assert_eq!(detect_language("こんにちは世界"), Some("ja"));
        assert_eq!(detect_language("안녕하세요 세계"), Some("ko"));
    }

    #[test]
    fn test_detect_language_empty_and_symbols() {
        assert_eq!(detect_language(""), None);
        assert_eq!(detect_language("   123456 !!! ??? --- "), None);
    }

    #[test]
    fn test_resolve_smart_target_lang() {
        // 用户默认目标是中文：输入中文时应自动对转为英文；输入外文时保持中文
        assert_eq!(resolve_smart_target_lang("你好世界", "zh-CN"), "en");
        assert_eq!(resolve_smart_target_lang("Hello world", "zh-CN"), "zh-CN");
        assert_eq!(resolve_smart_target_lang("こんにちは", "zh-CN"), "zh-CN");

        // 用户默认目标是英文：输入英文时自动对转为中文；输入非英文时保持英文
        assert_eq!(resolve_smart_target_lang("Hello world", "en"), "zh-CN");
        assert_eq!(resolve_smart_target_lang("你好世界", "en"), "en");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p transly-core detector`
Expected: FAIL (module or functions not found)

- [ ] **Step 3: Implement `detect_language` and `resolve_smart_target_lang` in `crates/transly-core/src/detector.rs`**

实现基于 Unicode 字符区段统计的高性能语言探测算法：
- 统计汉字（`\u{4e00}..=\u{9fa5}`）、日文假名（`\u{3040}..=\u{30ff}`）、韩文（`\u{ac00}..=\u{d7af}`）、拉丁字母（`a..=z`, `A..=Z`）。
- 假名优先判 `ja`，谚文优先判 `ko`，汉字占比高判 `zh-CN`，拉丁字母占比高判 `en`。
- 在 `crates/transly-core/src/lib.rs` 导出 `pub mod detector; pub use detector::{detect_language, resolve_smart_target_lang};`。

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p transly-core detector`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/transly-core/src/detector.rs crates/transly-core/src/lib.rs
git commit -m "feat(core): 实现离线语言检测与智能双向目标语言解析器"
```

---

### Task 2: src-tauri 平台层原地替换与快捷键注册

**Files:**
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/commands/mod.rs`

**Interfaces:**
- Produces:
  ```rust
  // Commands:
  pub async fn detect_text_language(text: String) -> Result<Option<String>, String>;
  pub async fn get_replace_shortcut() -> Result<String, String>;
  pub async fn set_replace_shortcut(app: tauri::AppHandle, shortcut: String) -> Result<(), String>;
  
  // Logic:
  pub fn trigger_selection_replacement(
      handle: tauri::AppHandle,
      sel_flag: Arc<AtomicBool>,
      target_lang: Option<String>,
  );
  ```

- [ ] **Step 1: Write tests for language detection IPC and shortcut helpers in `src-tauri`**

在 `src-tauri/src/commands/mod.rs` 中为 `detect_text_language` 与快捷键有效性添加单元测试。

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p src-tauri commands::tests::test_detect_text_language`
Expected: FAIL (commands not found)

- [ ] **Step 3: Implement selection replacement and shortcut handlers**

1. 实现 `trigger_selection_replacement`：
   - 备份旧剪贴板内容，置 `sel_flag = true`。
   - 模拟复制（Ctrl+C / Cmd+C）。
   - 轮询读取选中文本，利用 `resolve_smart_target_lang` 自动判定目标语言（默认目标语言若为 `zh-CN`，中文则对转为 `en`）。
   - 调用引擎池翻译该文本。
   - 写入译文到剪贴板，模拟粘贴（Ctrl+V / Cmd+V）。
   - 延迟 100ms 后恢复旧剪贴板内容，解除 `sel_flag`。
2. 暴露 `detect_text_language`、`get_replace_shortcut`、`set_replace_shortcut` 命令。
3. 在 `src-tauri/src/lib.rs` 的宏 `app_command_builder!` 中注册新命令并运行 `pnpm run generate:bindings`。
4. 在 `src-tauri/src/main.rs` 启动时注册替换快捷键（默认 `Alt+Shift+R`）。

- [ ] **Step 4: Run tests and bindings verification**

Run: `cargo test -p src-tauri`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/mod.rs src-tauri/src/main.rs src-tauri/src/lib.rs src/bindings.ts
git commit -m "feat(tauri): 实现划词原地翻译替换与自定义替换快捷键注册"
```

---

### Task 3: 前端任务状态机与 UI 交互集成

**Files:**
- Modify: `src/task/useTaskRunner.ts`
- Modify: `src/App.tsx`
- Modify: `src/i18n/zh-CN.json`
- Modify: `src/i18n/en.json`
- Test: `src/task/generation.test.ts` / `src/task/e2e.test.ts`

**Interfaces:**
- Consumes: `commands.detectTextLanguage`, `commands.getReplaceShortcut`, `commands.setReplaceShortcut`

- [ ] **Step 1: Update i18n language files with symmetric keys**

在 `src/i18n/zh-CN.json` 和 `src/i18n/en.json` 中同步增加：
- `smartAutoToggle`: "中英智能双向对转" / "Smart Bi-directional Toggle"
- `replaceShortcut`: "划词原地替换快捷键" / "In-place Replace Shortcut"
- `detectedLang`: "已识别源语言" / "Detected Language"

- [ ] **Step 2: Run i18n test to verify symmetry**

Run: `pnpm run test:task`
Expected: PASS (键结构保持完全对称)

- [ ] **Step 3: Update `useTaskRunner.ts` & `App.tsx`**

1. 在 `useTaskRunner` 中集成离线智能语言判定：
   - 当 `source_lang` 为 `auto` 且开启 `smartAutoToggle` 时，根据输入文本自适应确定目标语言，并在 UI 上呈现“已自动切换至目标语言”标识。
2. 在 `App.tsx` 设置面板中增加：
   - 划词原地替换快捷键设置输入框（支持自定义如 `Alt+Shift+R`）。
   - 智能双向对转开关（默认开启）。
3. 主输入框增加轻量源语言探测标签（如输入中文时显示“检测为中文”）。

- [ ] **Step 4: Run all lint, tests, and build**

Run:
```bash
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
pnpm run lint
pnpm run test:task
pnpm run build
```
Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add src/App.tsx src/task/useTaskRunner.ts src/i18n/zh-CN.json src/i18n/en.json
git commit -m "feat(ui): 主界面与设置中心接入智能双向对转与原地替换热键"
```

---

### Task 4: 更新文档与更新日志归档

**Files:**
- Modify: `README.md`
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Document new features in README.md**

更新桌面端用法说明，说明 `Alt+Shift+D` 悬浮翻译与 `Alt+Shift+R` 原地替换双工作流，以及智能语言对转。

- [ ] **Step 2: Append CHANGELOG.md following Keep a Changelog**

在 `[Unreleased]` 或新版本段落记录 `Added` 条目。

- [ ] **Step 3: Commit**

```bash
git add README.md CHANGELOG.md
git commit -m "docs: 记录智能语言检测与划词原地替换特性至说明文档与更新日志"
```
