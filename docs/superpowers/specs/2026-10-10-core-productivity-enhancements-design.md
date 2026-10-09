# 子工程 1：桌面端与核心生产力质跃包设计规范 (Spec)

## 1. 目标与背景 (Goal & Context)

为 Transly 桌面端落地首批核心生产力与交互增强功能，包含：
1. **E2**: 后端 LLM 流式翻译请求取消与资源回收（通过 `cancel_stream_task`，避免前端取消或换代后后端连接空耗与用户 Token 浪费）。
2. **E4**: Markdown 与代码块智能占位保真（在 `transly-core` 中保护 ` ```code``` ` 与 `` `code` `` 不被翻译引擎误翻破版）。
3. **E6**: 全局快捷键智能录制器（`ShortcutRecorder` 组件，支持一键按键捕获并自动格式化，支持清除与回退）。
4. **E7**: 划词悬浮窗图钉（Pin）置顶与用户受控失焦隐藏。
5. **E5**: 桌面端全界面深色模式（Dark Mode，支持浅色/深色/跟随系统）。

---

## 2. 架构与详细设计 (Architecture & Design)

### 2.1 E4: transly-core Markdown 代码块保护 (`crates/transly-core/src/preserve.rs`)

- **原理**：纯 Rust 确定性状态机/正则替换：
  - 提取多行代码块：` ```[language]\n...\n``` `
  - 提取行内代码：`` `...` ``
  - 提取数学公式块（可选）：`$$...$$`
  - 将上述块有序提取并映射为无害占位标签：`⟦TRANSLY_CODE_0⟧`、`⟦TRANSLY_CODE_1⟧`（使用 Unicode 专有括线，极难被主流翻译引擎篡改）
  - 翻译引擎完成翻译并应用术语库后，执行 `restore_preserved(translated, &stash)` 还原原本代码块。
- **纯函数契约**：
  ```rust
  pub struct PreservedText {
      pub text: String,
      pub stash: Vec<String>,
  }
  pub fn stash_code_blocks(input: &str) -> PreservedText;
  pub fn restore_code_blocks(input: &str, stash: &[String]) -> String;
  ```

### 2.2 E2: LLM 流式请求中断取消 (`src-tauri/src/commands/mod.rs` & `transly-core`)

- **原理**：
  - 在 `src-tauri` 维护全局并发流式任务注册表：`StreamTaskRegistry(Arc<Mutex<HashMap<u32, tokio_util::sync::CancellationToken>>>)`。
  - 当调用 `translate_stream` 时，生成或注册该 `task_id` 对应的 `CancellationToken`。
  - 在 `tokio::select!` 中监听 `token.cancelled()`，若被取消立即安全退出并向调用方返回取消信号。
  - 导出命令 `cancel_stream_task(task_id: u32)`，前端 `useTaskRunner` 在换代或 `cancel()` 时主动 invoke 该命令。

### 2.3 E6: 全局快捷键智能录制器 (`src/ShortcutRecorder.tsx`)

- **交互体验**：
  - 点击“录制按键”按钮后进入录制监听态（高亮脉冲提示“请在键盘上按下快捷键...”）。
  - 拦截 `keydown` 事件，阻止默认行为与冒泡。
  - 自动归一化提取修饰键（`Ctrl`, `Alt`, `Shift`, `Meta`）与主按键（`A-Z`, `0-9`, `F1-F12`, `Space` 等）。
  - 按下 `Escape` 退出录制并不保存；按下有效按键组合后自动填入并结束录制。

### 2.4 E7: 划词悬浮窗图钉（Pin）模式 (`src/App.tsx` & `src-tauri`)

- **交互体验**：
  - 浮窗标题栏右上角增加图钉按钮（`Pin` 图标）。
  - 默认未固定时，失焦自动隐藏（除非在翻译中或错误态）。
  - 点击图钉（`pinned = true`）后，窗口切换为永久置顶粘滞态（同时设置 `set_float_sticky(true)`），失焦不再隐藏，方便用户一边对照译文一边敲代码或记笔记。

### 2.5 E5: 桌面端深色模式 (Dark Mode)

- **实现方案**：
  - `tailwind.config.js` 启用 `darkMode: 'class'`。
  - 在设置中心提供三态切换：`浅色 (Light)`、`深色 (Dark)`、`跟随系统 (System)`。
  - 在 `<html>` 标签上动态切换 `dark` 类名。
  - 适配主界面、浮窗、历史记录面板、设置中心的所有卡片、输入框与文字颜色。

---

## 3. 错误处理与质量保证

1. **E4 单测覆盖**：
   - 包含多行代码块、行内代码、中英夹杂及代码块内含有换行/特殊字符的提取与还原往返测试。
2. **E2 取消安全性**：
   - 取消后清理注册表，杜绝内存泄漏；未注册任务取消时幂等忽略。
3. **i18n 对称性**：
   - 新增的所有 UI 字段（录制快捷键、图钉提示、深色模式三态）均在中英文语言包严格对称落地，通过自动化测试校验。
