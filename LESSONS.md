# Transly 经验教训与开发备忘 (LESSONS.md)

本文件用于记录项目开发与迭代过程中反复踩坑、具有长期工程指导价值的核心经验教训。

---

### 1. 浏览器富文本段落保真与 LLM 占位符容错
- **现象**：大模型（尤其在整页批量或多语种翻译时）偶发会在特殊占位符括线内部引入空格或格式微调（如将 `⟦T_1⟧` 输出为 `⟦ T_1 ⟧` 或 `⟦/T_1 ⟧`）。
- **教训**：
  - 无论在 Rust 核心层（`preserve.rs`）还是浏览器插件端（`richText.ts`），占位符提取和还原切禁使用严格的子串查找（`indexOf`）；
  - 必须采用宽松正则（如 `⟦\s*T_(\d+)\s*⟧`）先对输入执行统一归一化替换，再进行语序回填和 DOM 节点恢复，杜绝占位符数量比对失配导致整段回退丢译文。

### 2. 独立多窗口桌面应用的引擎状态同步
- **现象**：主窗口与划词悬浮迷你窗（`FloatWindow`）解耦运行时，若悬浮窗内引擎状态被硬编码为 `google`，会导致用户在设置中配置的 DeepSeek / DeepL / 本地 Ollama 在高频悬浮查词场景下完全不生效。
- **教训**：
  - 所有次级独立窗口（悬浮窗、通知栏、弹窗）在初始化时，必须通过统一 IPC 读取用户的首选偏好（`getPreferredReplaceEngine`）；
  - 界面上需提供可见的引擎切换控件，确保不同窗口的交互体验与能力预期完全对齐。

### 3. 跨端关键契约参数严禁双边硬编码
- **现象**：配对会话倒计时 TTL、分批上限等跨端数值如果在前后端各写一份（如前端硬编码 `60` 秒），两端升级不一致时会导致倒计时脱节或批次 413 拒绝。
- **教训**：
  - 遵循单一契约来源（Single Source of Truth）：服务端通过 `/health` 或事件广播（如 `transly://pairing-requested` 携带 `expires_in`）权威下发，前端直接消费后端动态下发的参数。

### 4. LaTeX 数学公式保真识别边界
- **现象**：Markdown 中的 LaTeX 数学公式（`$...$`、`$$...$$`）容易被外部翻译引擎误翻译或反斜杠破坏，但在提取行内公式时容易误伤日常金额符号（如 `$100 and $200`）。
- **教训**：
  - 行内公式识别必须遵循规范约束：排除反斜杠转义（`\$`），左 `$` 之后不得紧跟空白或数字（规避货币符号），右 `$` 前不得为反斜杠或空白，且行内公式不得跨行。

### 5. Playwright 浏览器扩展 E2E 测试的信道选择与 Chrome 137+ 限制
- **现象**：在 Linux CI（GitHub Actions `ubuntu-latest`）中若优先使用 `{ channel: 'chrome' }` 启动持久上下文，会命中系统预装的品牌版 Google Chrome，而 Chrome 137+ 已彻底移除品牌版对 `--load-extension` 参数的支持，导致扩展静默不加载、`waitForEvent('serviceworker')` 超时失败。
- **教训**：
  - 加载未打包 MV3 扩展的自动化测试必须优先使用 Playwright 自带的 Chromium（`{ channel: 'chromium' }` 或默认无 channel 配置，对应 Chrome for Testing），并在启动循环内校验 Service Worker 就绪后再锁定浏览器上下文。

### 6. React 异步 State 更新与立即触发命令的 Ref 时序竞态
- **现象**：在下拉框 `onChange` 中先调用 `setEngine(newEng)` 再立即调用 `runner.run(text)` 时，由于 React 状态更新异步生效，Hook 内部通过 `engineRef.current = engine` 同步的 Ref 仍停留在上一轮渲染的旧值，导致重跑请求实际发往旧引擎。
- **教训**：
  - 当命令式触发函数紧随 `setState` 同步调用时，必须通过调用参数（如 `run(text, { engine: newEng })`）显式传递新值并立即覆写内部 Ref，切忌假设 `setState` 后下一行代码能从渲染期同步的 Ref 中读到新状态。
