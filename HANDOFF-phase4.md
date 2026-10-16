# Transly 截图翻译 —— 阶段 4 施工交接单（批量翻译 + 叠译文）

> **交接对象**：接手实施阶段 4 的智能体。
> **前置**：阶段 0/1/2/3/5 已完成。阶段 2 通过 Windows 真机验收；阶段 3 的 macOS 分支**无真机验证**。
> **工作分支**：`feature/screenshot-translation`（HEAD = `aff23b0`）
> **全局设计**：`~/.claude/plans/sunny-swimming-quail.md` §九 流程图的第 10–11 步。

---

## 0. 铁律（违反即返工）

`AGENTS.md` + `HANDOFF-phase2.md` §0 全部生效。本阶段重点：

1. **分层**：字号自适应、百分比定位、溢出钳制都是**纯逻辑**，必须放 `crates/transly-core` 或前端 `src/utils/`（可单测），**不得**散在组件里。
2. **顺序不可调换**：批量翻译必须走 `translate_batch_with_cache`（阶段 1 已实现并单测），
   **绝不在 `src-tauri` 里再写一份**「术语库→缓存→引擎→缓存→术语库」的编排
   —— 那会是第四份复制（见该模块文档）。
3. **LESSONS 第 2 条**：覆盖窗是独立窗口，**必须能切引擎/语言**，且切换后旧结果不得覆盖新结果（代际闸门）。
4. **LESSONS 第 12 条**：E2E 禁用 `if (await x.isVisible())` 式静默跳过；横幅须声明覆盖范围。
5. **门禁**：fmt + clippy `-D warnings` + test + `pnpm lint` + `pnpm build` + `pnpm format:check` + `pnpm test:task` + `pnpm test:e2e:ui`。改命令后跑 `export_bindings` 并确认 `src/bindings.ts` 同步。
6. **LESSONS 第 11 条**：新增关键测试后，**必须把实现改回缺陷版确认 FAILED**，再恢复。
7. **i18n 中英双份**（en.json 缺键编译期报错）。
8. **E2E mock 必须登记新命令**（漏登记静默 `return null` 且**不被 `consoleErrors` 捕获**）。

---

## 1. 范围

| 交付物 | 路径 | 说明 |
|---|---|---|
| 批量翻译命令 | `src-tauri/src/commands/screenshot.rs` | `translate_screenshot_blocks` |
| 发往主窗口命令 | `src-tauri/src/commands/screenshot.rs` | `push_screenshot_text_to_main` |
| 前端布局纯逻辑 | `src/utils/screenshotLayout.ts` + `.test.ts` | `fitFontSize` / `toPercentStyle` / `clampRenderedBox` |
| 覆盖窗译文层 | `src/ScreenshotOverlay.tsx` | 叠译文、还原原文、引擎/语言下拉、复制、单块重试 |
| i18n | `src/i18n/{zh-CN,en}.json` | 中英双份 |
| E2E | `scripts/test-desktop-ui-e2e.mjs` | mock 登记 + 断言 |

**不属于本阶段**：文档收尾（阶段 6）。

### 1.2 验收检查点

1. **中译英 / 英译中**均正常，块按原坐标叠加。
2. **含术语库词条**时译文按词库替换（源端预替换 + 译文端后替换）。
3. **断网**：错误可读且可重试（不是白屏或卡死）。
4. **LLM 长耗时中切语言** → 立即重译，且**旧结果不覆盖新结果**（LESSONS 第 2 条）。
5. **译文长于原文**时字号自适应且有**最小字号下限**（不能缩到不可读）。
6. `pnpm run build && lint && format:check` 通过。

---

## 2. 详细设计

### 2.1 `translate_screenshot_blocks` 命令

```rust
#[tauri::command]
#[specta::specta]
pub async fn translate_screenshot_blocks(
    app: AppHandle,
    texts: Vec<String>,
    target_lang: String,
    engine: Option<String>,
    generation: u32,
    deps: tauri::State<'_, Arc<TranslationDependencies>>,
) -> Result<Vec<ScreenshotBlockResultDto>, TranslateErrorDto>;
```

**契约要点**：

- **代际闸门**：`generation` 与会话比对，不等则拒绝（陈旧窗口的结果不得落到新会话）。
- **逐项容错**：`translate_batch_with_cache` 返回 `Vec<Result<..>>`，**必须保留逐项结果**
  （单块失败不能让整批失败——用户要能对失败块单独重试）。
- **扁平 DTO**（specta 禁嵌套 `Option`、禁 `u64`/`i64`）：

```rust
#[derive(serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ScreenshotBlockResultDto {
    pub index: u32,
    pub text: String,           // 成功时为译文，失败时为空串
    pub failed: bool,
    pub error_kind: Option<ErrorKind>,   // 失败时供前端映射 i18n
    pub error_message: Option<String>,
    pub engine: String,         // 实际执行引擎（可能降级）
}
```

- **源语言**：`"auto"`（截图内容语言未知，交给引擎/离线检测）。
- **引擎**：`engine` 为 `None` 时用 `pool.default_engine()`；显式指定时先 `validate_engine_id`。
- **`spawn_blocking`**：`translate_batch_with_cache` 内部会同步访问 SQLite 缓存。
- 命令返回 `Result<Vec<..>, TranslateErrorDto>`：**整批级失败**（如引擎鉴权失败）走 `Err`，
  **单项失败**走 `Vec` 里的 `failed: true`。两者不可混用。

### 2.2 `push_screenshot_text_to_main`

把译文（或原文）送到主窗口编辑重译（全局设计 §十二：砍掉「OCR 结果可编辑」后的替代路径）。

```rust
#[tauri::command]
#[specta::specta]
pub async fn push_screenshot_text_to_main(app: AppHandle, text: String) -> Result<(), String>;
```

- 实现：`app.emit_to("main", "transly://screenshot-to-main", payload)` + 显示并聚焦主窗口 + 关闭覆盖窗。
- 前端 `App.tsx` 监听该事件 → 写入 `sourceText` 并触发翻译。
- **不要**新增一套 IPC 拉取机制；既有 `FloatRequest` 的「先暂存再广播」范式仅用于浮窗，此处 emit 即可
  （主窗口在截图期间一直存活，不存在「窗口未就绪」竞态）。

### 2.3 前端布局纯逻辑（`src/utils/screenshotLayout.ts`）

**必须可单测**（放 `src/utils/`，用 `node:test`，与 `src/utils/shortcutKeys.test.ts` 同构；
`pnpm run test:task` 会跑 `src/utils/*.test.ts`）。

```ts
/** 归一化矩形 → CSS 百分比定位。禁止在组件里重复实现。 */
export function toPercentStyle(rect: { x: number; y: number; w: number; h: number }): {
  left: string; top: string; width: string; height: string
}

/**
 * 为「译文塞进原文框」挑选字号（px）。
 * 返回 null 表示连最小字号都放不下——调用方应溢出显示而非缩到不可读。
 */
export function fitFontSize(
  text: string,
  box: { w: number; h: number },   // 物理像素
  opts?: { maxPx?: number; minPx?: number; lineHeight?: number },
): number | null

/** 把渲染框钳制在容器内（避免译文溢出到覆盖窗之外）。 */
export function clampRenderedBox(
  box: { x: number; y: number; w: number; h: number },
  container: { w: number; h: number },
): { x: number; y: number; w: number; h: number }
```

**要点**：

- `toPercentStyle` 必须**四舍五入到 4 位小数**再转百分比，否则 `0.1 * 100` 会得到
  `10.000000000000002%`（阶段 2 已踩，见 `ScreenshotOverlay.tsx` 的 `pct()`）。
- `fitFontSize` 的估算必须**保守**：CJK 按 1 字宽 = 1 字号，西文按约 0.55 字号估算。
  **宁可猜小也不要溢出**（溢出会盖住相邻块）。
- `fitFontSize` 返回 `null` 时，调用方用 `minPx` 渲染并加 `overflow-hidden`（不要缩到 2px 的鬼影字）。
- **最小字号下限**是硬要求（检查点 5）：建议 `minPx = 9`。

### 2.4 覆盖窗译文层（`src/ScreenshotOverlay.tsx`）

在阶段 3 已有的 `shot-block` 渲染基础上扩展：

| 交互 | 要求 |
|---|---|
| 叠译文 | 按块归一化坐标定位，字号由 `fitFontSize` 决定；译文为空/失败时显示原文并标记失败态 |
| 还原原文 / 显示译文 | **纯前端切换，零 IPC**（阶段 2 已有 `showTranslation` 状态） |
| 引擎下拉 | 复用 `translation.engine*` 文案；**切换后立即用已有块重译**（不重新 OCR） |
| 语言下拉 | 复用 `translation.language*`；同样立即重译 |
| 复制全部译文 | 拼接所有成功块，`navigator.clipboard.writeText` |
| 单块重试 | 只重发该块；失败态可视化（红框/图标） |

**引擎/语言切换的实现要点**：

- 块文本已缓存在组件状态里（阶段 3 的 `blocks`），切换时**直接重发 `translate_screenshot_blocks`**，
  **不要**重新调 `recognize_screenshot_selection`（OCR 贵且结果不变）。
- **代际闸门**：每次发起翻译前递增本地 `translationGeneration`，落地前比对，
  丢弃过期结果（LESSONS 第 2 条 + 与 `useTaskRunner` 同构）。
  > ⚠️ 这里有两层 generation：**会话 generation**（Rust 侧，防陈旧窗口）
  > 与**翻译代际**（前端，防旧译文覆盖新译文）。不要混用同一个变量。

### 2.5 i18n

需新增（中英双份）：译文/原文切换、复制全部、单块重试、翻译中、失败标记、发送到主窗口、
以及引擎/语言下拉的标签（可复用 `translation.*`）。

---

## 3. 测试要求

### 3.1 core / 前端单测

| 被测 | 必须能捕获的缺陷 |
|---|---|
| `toPercentStyle` | 浮点误差未取整（`10.000000000000002%`）；漏乘 100 |
| `fitFontSize` | 字号估算过于乐观导致溢出；忽略最小字号下限 |
| `clampRenderedBox` | 只钳左/上不钳右/下；负尺寸未归零 |
| 批量结果与请求的**下标对应** | 失败项与成功项错位（前端会把译文贴到错误的块上） |

### 3.2 端到端

- **本机可做（必须做）**：用一个**假引擎**（或 mock IPC）验证「块下标 ↔ 译文」不错位；
  E2E 用例 10 覆盖：翻译后块渲染、还原原文、切换语言触发重译、单块失败可视化。
- **Windows 真机（可做）**：完整链路（截图 → OCR → 翻译 → 叠译文），
  含**断网**与**切语言**两个场景。

---

## 4. E2E 断言（新增用例 10）

1. mock 登记 `translate_screenshot_blocks` 与 `push_screenshot_text_to_main`。
2. 识别出 2 个块后，翻译结果按 `index` 回填到**对应**块（不是按数组顺序硬贴）。
3. 点「还原原文」后译文不可见、背景图仍在。
4. 切换目标语言 → **再次调用** `translate_screenshot_blocks` 且入参 `target_lang` 已变，
   **不得**再次调用 `recognize_screenshot_selection`（OCR 不该重跑）。
5. 某块返回 `failed: true` → 该块显示失败态，**其余块正常显示译文**（逐项容错）。
6. 横幅用例数从 9 改为 10；保持覆盖范围声明。

---

## 5. 坑表

| 坑 | 说明 |
|---|---|
| **两套 generation 混用** | 会话 generation（Rust）≠ 翻译代际（前端）。混用会让「切语言」误判为陈旧会话 |
| **切语言重跑 OCR** | 块文本已在内存，重跑 OCR 既慢又浪费；只重发翻译 |
| **按数组顺序贴译文** | 必须按 `index` 回填；失败项占位后数组顺序会与块顺序错位 |
| **浮点百分比** | `0.1*100 = 10.000000000000002`，必须取整（阶段 2 已踩） |
| **specta 禁 `u64`/`i64`/嵌套 Option** | `index` 用 `u32`，错误信息用扁平字段 |
| **`translate_batch_with_cache` 顺序** | 术语库源端 → 缓存 → 引擎 → 缓存 → **译文端 `apply_to_translation`**（用 `apply` 会反向改写，LESSONS 第 8 条） |
| **E2E mock 漏登记** | 静默 `return null` 且不被 `consoleErrors` 捕获 |
| **Rust 的 `TranslateErrorDto.message` 是中文** | 前端用 `t.error[kind]` 映射，不透传（阶段 2 踩过同类） |
| **覆盖窗是独立窗口** | 语言/主题必须自读 localStorage（LESSONS 第 2 条，阶段 2 已实现，勿退化） |

---

## 6. 开工前自查

- [ ] `git log --oneline -3` 确认 HEAD 是 `aff23b0`，`git status` 干净
- [ ] 基线门禁：`cargo test --workspace` 应得 **268 passed**；`pnpm run test:e2e:ui` 应得 **9/9**
- [ ] 读过 `LESSONS.md` 第 2、8、11、12、20、24 条
- [ ] 确认 `translate_batch_with_cache` 的签名与顺序（`crates/transly-core/src/engine/batch_pipeline.rs`）

## 7. 完成后交付要求

1. 全部门禁通过，附**实际输出关键行 + 数字**。
2. §1.2 六项检查点在能做的环境上做过，**如实说明哪些未做及原因**。
3. 反向验证执行过，说明**哪几处确认 FAILED**。
4. 汇报里明确列出**未完成项与已知限制**。

---

## 附：阶段 6 概览（**本单不要求**）

E2E 用例扩展（如需）；CHANGELOG / README / PROJECT_RULES / 各 README 更新；`pnpm run generate:bindings && git diff --exit-code src/bindings.ts`。
