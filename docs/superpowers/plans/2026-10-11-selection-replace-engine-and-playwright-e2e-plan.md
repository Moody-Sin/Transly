# 划词原地替换首选引擎与浏览器插件端到端集成测试实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 落地划词原地替换首选引擎配置与直接报错提醒机制，并为浏览器扩展搭建基于真实 Headless Chromium 的 Playwright 端到端集成测试套件。

**Architecture:** 严格遵循架构规范：配置读取与失败报错在 `src-tauri`；前端通过 `tauri-specta` 强类型绑定提供设置项与 Toast 报错浮层；Playwright E2E 测试套件独立于 `extension` 产物之上，利用内置轻量测试服务器与系统已安装的 Chrome 执行真实 DOM 原地替换与 Shadow DOM 穿透断言。

**Tech Stack:** Rust (Tauri 2, Keyring, Tokio), React 19, TypeScript, Playwright (Chromium MV3 Extension Loading).

**Spec:** `docs/superpowers/specs/2026-10-11-selection-replace-engine-and-playwright-e2e-design.md`

## Global Constraints

- **存储策略**：`preferred_replace_engine` 必须托管于系统原生凭据管理器（`keyring`），严禁明文落盘。
- **失败策略**：划词原地替换若因缺少 API Key 或网络请求失败，**必须直接报错提醒**，禁止静默使用 Google 降级，且必须保护用户原剪贴板。
- **CI / 质量要求**：提交前必须通过 `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace` + `pnpm run lint` + `pnpm run test:task` + `pnpm run build`。

## Review Focus

1. 用户选择了商业引擎（如 DeepSeek）但未配置 API Key 时按下快捷键：必须立即弹出报错 Toast，不能白屏、不能卡住、不能污染剪贴板。
2. 划词原地替换执行失败时：原剪贴板内容必须保持不变。
3. Playwright 测试在加载 MV3 扩展时：必须能成功向页面注入 content script 并完成原地 DOM 替换。
4. 双语对照模式与还原在真实 DOM 下：追加的兄弟节点在点击还原后必须被彻底移除。
5. 前后端类型绑定：新增 IPC 命令必须通过 `pnpm run generate:bindings` 保证 `src/bindings.ts` 零漂移。

---

## 任务拆解与执行步骤

### Task 1: 划词原地替换首选引擎后端实现与事件通知机制 (`src-tauri`)

**Files:**
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/commands/mod.rs` (新增单元测试)

- [ ] **Step 1: 在 `src-tauri/src/commands/mod.rs` 中编写配置读取与保存命令及单测**
  - 新增命令：
    - `get_preferred_replace_engine() -> Result<String, String>`：读取 `preferred_replace_engine`，默认回退 `"google"`；
    - `save_preferred_replace_engine(engine: String) -> Result<(), String>`：严格校验 `google` | `deepl` | `deepseek` | `openai`，非法输入报错，合法值异步写入系统凭据。
  - 新增单测：验证白名单校验与默认回退逻辑。

- [ ] **Step 2: 改造 `trigger_selection_replacement` 工作流**
  - 在无锁第二阶段：
    - 读取 `preferred_replace_engine`；
    - 通过 `deps.pool.get_engine(&preferred).await` 获取引擎；
    - 若商业引擎缺少凭据：调用 `app.emit("transly://replacement-failed", ...)` 并退出；
    - 若调用 `active_engine.translate(&req).await` 失败：调用 `app.emit("transly://replacement-failed", ...)` 并安全退出；
    - 若成功：正常写入缓存、应用术语库并在第三阶段覆盖选区与安全还原原剪贴板。

- [ ] **Step 3: 在 `src-tauri/src/lib.rs` 的宏中注册新命令**
  - 注册 `get_preferred_replace_engine` 与 `save_preferred_replace_engine`。

- [ ] **Step 4: 运行 `cargo test --workspace` 确保后端测试全绿**

---

### Task 2: 前端绑定与设置中心交互集成 (`src`)

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/i18n/zh-CN.json`
- Modify: `src/i18n/en.json`
- Regenerate: `src/bindings.ts`
- Test: `src/task/i18n.test.ts`

- [ ] **Step 1: 重新生成并比对前后端绑定**
  - 运行 `pnpm run generate:bindings`，确保 `get_preferred_replace_engine` 与 `save_preferred_replace_engine` 成功导出至 `src/bindings.ts`。

- [ ] **Step 2: 补充中英双语国际化文案**
  - 在 `src/i18n/zh-CN.json` 与 `src/i18n/en.json` 中增加：
    - `settings.preferredReplaceEngine`: "划词原地替换引擎" / "Preferred In-Place Replacement Engine"
    - `settings.statusReplaceEngineSaved`: "原地替换引擎已更新" / "In-place replacement engine updated"
    - `error.replacementFailed`: "划词原地替换失败" / "In-place replacement failed"
  - 运行 `pnpm run test:task` 验证双语结构 100% 对称。

- [ ] **Step 3: 在 `src/App.tsx` 设置中心集成下拉选择器与事件监听**
  - 状态管理：`preferredReplaceEngine`；
  - 启动时自动从后端加载，修改时调用 `save_preferred_replace_engine`；
  - 在全局增加 `transly://replacement-failed` 事件监听，触发时调用 `showToastError` 弹出明确提示；
  - 运行 `pnpm run lint` 与 `pnpm run build` 确保无报错。

---

### Task 3: 浏览器扩展 Playwright 端到端集成测试搭建与用例套件 (`scripts/` / `extension/`)

**Files:**
- Modify: `package.json`
- Create: `scripts/test-extension-e2e.mjs`
- Test: 执行 `node scripts/test-extension-e2e.mjs`

- [ ] **Step 1: 安装 Playwright 核心依赖**
  - 安装 `playwright` 作为 devDependencies（使用系统内置 Chrome / Edge，不额外下载重型 Chromium）。

- [ ] **Step 2: 编写独立可执行的端到端测试运行器 `scripts/test-extension-e2e.mjs`**
  - 内置基于 Node.js 原生 `http` 模块的轻量 Mock 服务器：
    - 端口 `39281`（响应 `/health`, `/verify_token`, `/translate`）；
    - 托管测试页面（包含纯文本、行内标签富文本段落、Open Shadow DOM 组件）；
  - 启动无头 Chromium 并挂载 `extension/dist`；
  - 核心用例覆盖：
    1. 整页仅译文原地替换：验证原 DOM 文本节点内容被无损替换；
    2. 富文本段落嵌套保真：包含 `<a>` 和 `<strong>` 的段落整句替换后结构完好；
    3. 开放模式 Shadow DOM 穿透：Web Components 内部文本节点成功替换；
    4. 双语对照模式与一键还原：生成 `.transly-bilingual-trans`，还原后节点完全清理。

- [ ] **Step 3: 在 `package.json` 中配置脚本命令**
  - 添加 `"test:e2e:extension": "pnpm --filter transly-extension run build && node scripts/test-extension-e2e.mjs"`。

- [ ] **Step 4: 运行 `pnpm run test:e2e:extension` 验证端到端测试通过**

---

### Task 4: 全量流水线验证、更新日志与经验沉淀

- [ ] **Step 1: 运行全量 CI 门禁命令**
  - `cargo fmt --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - `pnpm run lint`
  - `pnpm run test:task`
  - `pnpm run build`
  - `pnpm run format:check`
  - `pnpm run test:e2e:extension`

- [ ] **Step 2: 更新 `CHANGELOG.md` 与 `Lessons Learned.md`**
  - 在 `CHANGELOG.md` 记录“支持划词原地替换首选引擎配置与直接报错提醒”、“引入浏览器插件 Playwright 真实无头环境端到端测试”；
  - 若有平台/测试踩坑记录至 `Lessons Learned.md`。
