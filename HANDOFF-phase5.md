# Transly 截图翻译 —— 阶段 5 施工交接单（三入口 + 降级探测）

> **交接对象**：接手实施阶段 5 的智能体。
> **前置**：阶段 0/1/2 已完成并**通过 Windows 真机验收**（2560×1600 @150%，三个检查点全 PASS）。
> 阶段 3（OCR）已完成但 macOS 分支**无真机验证**。
> **工作分支**：`feature/screenshot-translation`（HEAD = `9720d71`）
> **本阶段只依赖阶段 2**，故可先于阶段 4 实施（阶段 4 见 §附）。

---

## 0. 铁律（违反即返工）

`AGENTS.md` + `HANDOFF-phase2.md` §0 全部条款继续生效。本阶段新增/最易踩的：

1. **`ShortcutStatusSlot` 加第三槽位，但既有两份注册代码「本次不动」**。
   全局设计明确：既有两份（划词浮窗 `Alt+Shift+D`、原地替换 `Alt+Shift+R`）
   **保持零行为改动**，新增第三份（截图 `Alt+Shift+S`）用泛型 helper。
   > 若为「统一」而重写既有两份，就是超出范围的改动 —— 它们的竞态处理与回滚逻辑
   > 是历史踩坑的产物（见 `selection.rs` 的长注释），动它们风险远大于收益。
2. **快捷键注册失败必须原子回滚 + 状态可见**。`Alt+Shift+S` 极易被占用；
   静默丢弃会让用户看到「截图功能莫名不工作」且无任何线索。参照既有
   `set_shortcut` 的四段式（注销旧键 → 注册 → 失败则重注册旧键 → 持久化，持久化失败也回滚）。
3. **降级必须给可执行指引，不能笼统说「不支持」**（全局设计 §十一）。
   Wayland 的文案已由 `OcrErrorDto::wayland_unsupported()` 提供，直接复用。
4. **门禁**：`cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings`
   + `cargo test --workspace` + `pnpm run lint` + `pnpm run build`。
   改命令后必须跑 `cargo run -p src-tauri --bin export_bindings` 并确认 `src/bindings.ts` 同步。
5. **LESSONS 第 11 条**：新增关键测试后，**必须把实现改回缺陷版确认 FAILED**，再恢复。
6. **i18n 中英双份**：`src/i18n/index.ts` 把 `I18nSchema` 显式标注为 zh-CN 类型，
   **en.json 缺键会编译期报错**（有意的门禁）。另有 `src/i18n/*.test.ts` 的对称性检查。
7. **E2E mock 必须登记新命令**：未登记的命令会**静默 `return null` 且 E2E 不失败**
   （`default` 分支只 `console.warn`，**不被 `consoleErrors` 捕获**）——这是最易漏的坑。

---

## 1. 范围

### 1.1 交付物

| 交付物 | 路径 | 说明 |
|---|---|---|
| 第三个快捷键 + 状态槽 | `src-tauri/src/commands/mod.rs` | `ShortcutStatusSlot` 加 `screenshot_error`；`ShortcutStatusDto` 加字段；新增 `DEFAULT_SCREENSHOT_SHORTCUT = "Alt+Shift+S"` |
| 快捷键读写命令 | `src-tauri/src/commands/mod.rs` | `get_screenshot_shortcut` / `set_screenshot_shortcut`（四段式回滚，与既有一致） |
| 能力探测命令 | `src-tauri/src/commands/screenshot.rs` | `get_screenshot_capability()` → 见 §2.1 |
| 快捷键注册（第三份） | `src-tauri/src/main.rs` | 泛型 helper，**不动既有两份** |
| 托盘菜单项 | `src-tauri/src/main.rs` | 加「截图翻译」项；**注意菜单弹层会被截进画面，需延迟 200–250ms 再触发** |
| 主窗口按钮 | `src/App.tsx` | 复用 `ShortcutRecorder`；按钮在 `capability.supported == false` 时禁用并显示原因 |
| 设置中心卡片 | `src/App.tsx` | 第三个快捷键录制器 + 冲突提示（与既有两张卡片同构） |
| i18n | `src/i18n/{zh-CN,en}.json` | 中英双份 |
| E2E | `scripts/test-desktop-ui-e2e.mjs` | mock 登记新命令；新增断言（见 §4） |

**不属于本阶段**：译文渲染（阶段 4）、CHANGELOG/README 收尾（阶段 6）。

### 1.2 验收检查点

1. **三入口各触发一次** → 覆盖窗正常弹出。
2. **故意设冲突快捷键** → 提示出现在**对应卡片**，且**不影响另两个**。
3. **Wayland 下按钮禁用并显示原因**（本机是 WSLg，`WAYLAND_DISPLAY=wayland-0`
   且 `XDG_SESSION_TYPE` 未设 → `detect_wayland_session` 会判为 Wayland，**本机可验**）。
4. 托盘点击触发截图，且**菜单弹层不被截进冻结帧**。

---

## 2. 详细设计

### 2.1 `get_screenshot_capability()` —— 降级探测

```rust
#[derive(serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ScreenshotCapabilityDto {
    /// 当前环境是否支持框选截图
    pub supported: bool,
    /// 不支持时的**可执行**原因（直接展示给用户，不要自己再拼文案）
    pub reason: Option<String>,
    /// OCR 后端是否可用（Linux 依赖外部 tesseract）
    pub ocr_available: bool,
}
```

**探测逻辑（必须下沉到 `transly-core`，因为它是纯判定）**：

平台能力判定本身是纯逻辑，但**「tesseract 是否存在」是文件系统探测**，属平台层。
建议：

- core 已有 `detect_wayland_session(xdg_session_type, wayland_display) -> bool`，**直接复用**。
- `src-tauri` 侧组装：
  - Linux：`detect_wayland_session(...)` 为真 → `supported=false`，`reason` 用
    `OcrErrorDto::wayland_unsupported().message`（**复用既有文案，不要另写一份**）；
    否则 `supported=true`；`ocr_available` = `which tesseract` 成功（用
    `Command::new("tesseract").arg("--version")` 探测，**不要引新依赖**）。
  - Windows / macOS：`supported=true`（权限问题在捕获时才能确定，不在探测阶段拦）。
    macOS 可选：用 `CGPreflightScreenCaptureAccess()` 预检并据此设 `supported`。
  - `ocr_available`：Windows/macOS 恒 `true`（系统原生）；Linux 看 tesseract。

**⚠️ 不要**在探测里真的去捕获屏幕（那会闪一下且开销大）。探测只回答「理论上能不能」。

### 2.2 快捷键状态槽加第三槽位

```rust
pub struct ShortcutStatusDto {
    pub ok: bool,
    pub reason: Option<String>,
    pub translate_error: Option<String>,
    pub replace_error: Option<String>,
    pub screenshot_error: Option<String>,   // 新增
}
```

- `ShortcutStatusSlot` 加 `screenshot_error: Mutex<Option<String>>` + `set_screenshot_ok/failed`。
- `get()` 的 `reason` 聚合需把三者都纳入（现在是两两 match，**改成对 `[t,r,s]` 的通用聚合**，
  否则三槽位下会漏）。
- **既有字段语义不变**（`translate_error` / `replace_error` 仍是各自独立字段）。

### 2.3 第三份快捷键注册

`main.rs` 的既有两份是手写的四段式。新增第三份时：

- 用**泛型 helper** 收敛重复（全局设计 §十一「快捷键注册代码三份漂移」的缓解措施），
  但**不得改动既有两份的调用点**（它们的行为必须逐字节等价）。
- 默认值 `Alt+Shift+S`，持久化键 `custom_screenshot_shortcut`（与既有命名风格一致）。
- 触发时调用 `crate::overlay::start(...)`（**异步**，需 `tauri::async_runtime::spawn`）。
- **目标语言与引擎**：从 `preferred_replace_engine` 读引擎（与 `commands/screenshot.rs`
  既有的 `load_prefs` 一致）；目标语言暂固定 `zh-CN`（阶段 4 才有语言下拉）。
  **直接复用 `commands::screenshot::start_screenshot_translation` 的实现路径**，
  不要另写一份取偏好的逻辑。

### 2.4 托盘入口

```rust
let shot_i = MenuItem::with_id(app, "screenshot", "截图翻译", true, None::<&str>)?;
```

- 在 `on_menu_event` 里加 `"screenshot" => { ... }`。
- **必须延迟 200–250ms 再触发**：菜单弹层（popup）此时仍在屏幕上，
  直接捕获会把菜单截进冻结帧。用 `tauri::async_runtime::spawn` + `sleep`。
- 若当前环境不支持（`get_screenshot_capability` 为 false），点击后应给提示
  而不是静默无反应。可复用 `Notification` 或直接什么都不做并打印日志——
  **按最小改动原则，先用「不做任何事 + eprintln 留痕」**，不要为此引新依赖。

### 2.5 主窗口按钮

在既有「识别剪贴板图片」按钮旁加一个「截图翻译」按钮：

- 与 `handleOcrClipboard` 同构：调 `commands.startScreenshotTranslation()`，
  失败时用 `t.ocrError[err.kind] || err.message` 映射文案（**注意这是阶段 2 踩过的坑**：
  不要把 Rust 的硬编码中文 message 直接透传）。
- `capability.supported == false` 时 `disabled` 并显示 `reason`（tooltip 或内联提示）。
- 挂载时拉一次 `getScreenshotCapability()`（与既有 `getShortcutStatus` 的时机一致）。

### 2.6 设置中心第三张卡片

- 复用 `ShortcutRecorder`，与既有两张卡片同构。
- 冲突提示读 `status.screenshot_error`。
- 新增 i18n 键：卡片标题、按钮文案、禁用原因、快捷键冲突提示。

---

## 3. 测试要求

### 3.1 core 单测（受 70% 覆盖率门禁，当前 92.02%，**不得下降**）

| 被测 | 必须能捕获的缺陷 |
|---|---|
| 三槽位状态聚合（`reason` 拼接） | 只聚合前两槽、漏第三槽；三个都失败时丢失信息 |

> 注：`ShortcutStatusSlot` 在 `src-tauri`（无覆盖率门禁），但其**聚合逻辑是纯函数**，
> 若为它加测试，请放在 `src-tauri` 的 `#[cfg(test)]` 里（既有先例：
> `test_shortcut_status_slot_transitions`）。**不要为了覆盖率把宿主状态搬进 core。**

### 3.2 端到端验证

- **本机可验（必须做）**：
  1. `get_screenshot_capability` 在 WSLg 下应返回 `supported=false` + Wayland 原因
     （本机 `WAYLAND_DISPLAY=wayland-0`、`XDG_SESSION_TYPE` 未设 → 判为 Wayland）。
     强制 `XDG_SESSION_TYPE=x11` 时应返回 `supported=true`。
  2. 三槽位：故意注册一个被占用的快捷键（如 `Alt+Shift+D` 已被划词占用），
     断言只有 `screenshot_error` 被设置，`translate_error`/`replace_error` 不受影响。
- **Windows 真机（可做，方法见 §5）**：三入口各触发一次 + 托盘菜单不被截进冻结帧。

---

## 4. E2E 断言（`scripts/test-desktop-ui-e2e.mjs`）

1. **mock 登记**新命令（漏登记会静默 null 且不失败）：
   `get_screenshot_capability`、`get_screenshot_shortcut`、`set_screenshot_shortcut`。
2. 新增断言：
   - 主窗口存在「截图翻译」按钮；
   - 点击后 `start_screenshot_translation` 被调用；
   - 当 mock 返回 `supported: false` 时按钮为 `disabled` 且展示原因；
   - 设置中心存在**第三个** `ShortcutRecorder`，且其冲突提示只受 `screenshot_error` 影响。
3. 横幅用例数若增加需同步改（当前「全部 8 项」）。
4. **不得**用 `if (await x.isVisible())` 式静默跳过（LESSONS 第 12 条）。

---

## 5. 真机验收方法（Windows，本机可用）

本机是 WSL2 + Windows 11 双环境，`windows-mcp` 已配置（服务在
`127.0.0.1:18006`，**需手动启动**：跑 Windows 上的
`~/.windows-mcp/start-server.cmd`，因为官方 `install` 注册计划任务会因用户名
含特殊字符失败）。要点：

- Windows 侧有 cargo 1.99 + MSVC + WebView2 + node/pnpm，且能访问
  `\\wsl.localhost\Ubuntu\root\xm\Transly`。
- 构建：设 `CARGO_TARGET_DIR=C:\transly-win-target` 与 `CARGO_INCREMENTAL=0`
  （**增量编译锁在 WSL 网络盘上会失败**）。
- 前端：`pnpm exec vite --host 0.0.0.0`（WebView2 用 `localhost`，只监听
  `127.0.0.1` 会连不上）。
- 启动应用时设 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`，
  即可从 Linux 侧用 Playwright `connectOverCDP('http://127.0.0.1:9222')` 驱动真实应用。
- 本机显示器是 **2560×1600 @150%**（`innerW=1707`、`dpr=1.5`）。

---

## 6. 坑表

| 坑 | 说明 |
|---|---|
| **托盘菜单弹层被截进冻结帧** | 菜单 popup 在屏幕上时捕获会把它截进去 → 触发前延迟 200–250ms |
| **既有两份快捷键代码不许动** | 全局设计明确「对既有行为零改动」，其回滚/竞态逻辑是踩坑产物 |
| **`reason` 聚合漏第三槽** | 两两 match 的写法加到三槽会漏；改成数组聚合 |
| **Rust 的 `OcrErrorDto::message` 是硬编码中文** | 前端必须用 `t.ocrError[kind]` 映射（阶段 2 已踩，提交 `72b9247`） |
| **E2E mock 漏登记命令** | 静默 `return null` 且**不被 `consoleErrors` 捕获**，用例照样通过 |
| **`ShortcutStatusDto` 加字段是 breaking** | `src/bindings.ts` 会变；前端所有读取处要同步（TS 会编译报错，属好事） |
| **`manage()` 同类型重复会 panic** | 新增 state 必须 newtype（Tauri 按 TypeId 存） |
| **specta 禁 `u64`/`i64`、禁嵌套 `Option`** | 用扁平字段与 `u32` |
| **`WindowEvent` 是 `#[non_exhaustive]`** | match 必须带 `_ => {}` |
| **平台专属代码本机 clippy 看不到** | 用 `/tmp` 隔离 scratch crate 交叉 clippy（LESSONS 19） |
| **本机是 WSLg，捕获不可用** | 根窗口 `GetImage` 被 XWayland 拒绝（已用 libX11 独立证明是服务器限制）。**故本机只能验「降级探测」，不能验捕获** |

---

## 7. 开工前自查

- [ ] `git log --oneline -3` 确认 HEAD 是 `9720d71`，`git status` 干净
- [ ] 基线门禁：`cargo test --workspace` 应得 **267 passed**
- [ ] 读过 `LESSONS.md` 第 11、12、19、20、23 条
- [ ] 确认本机 `WAYLAND_DISPLAY=wayland-0`（用于验证降级探测）

## 8. 完成后交付要求

1. 全部门禁通过，附**实际输出关键行 + 数字**。
2. §1.2 四个检查点在能做的环境上做过，**如实说明哪些未做及原因**。
3. 反向验证执行过，说明**哪几处确认 FAILED**。
4. 汇报里明确列出**未完成项与已知限制**。

---

## 附：阶段 4 / 6 概览（**本单不要求**）

| 阶段 | 内容 | 前置 |
|---|---|---|
| **4** 批量翻译 + 叠译文（2d） | `translate_screenshot_blocks`（用 `translate_batch_with_cache`，阶段 1 已备）；前端 `layout.ts`（`fitFontSize`/`toPercentStyle`/`clampRenderedBox`）；译文层 + 还原原文 + 引擎/语言下拉 + 复制 + 单块重试 + `push_screenshot_text_to_main` | 阶段 3 ✅ |
| **6** E2E + 文档（1.5d） | E2E 用例扩展；CHANGELOG / README / PROJECT_RULES / 各 README | 全部 |
