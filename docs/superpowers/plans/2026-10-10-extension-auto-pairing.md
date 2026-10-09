# 浏览器插件自动配对 Token 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现浏览器插件与桌面端无需手动复制粘贴 Token 的全自动安全握手配对。

**Architecture:** 
桌面端 Axum HTTP 服务暴露 `/pair/request` 与 `/pair/poll` 握手端点，通过内存状态机维护 60 秒有效会话并以 Tauri Event 通知前端显示授权确认浮层；用户在桌面端点击“允许”后通过 Tauri 命令标记授权，插件轮询拿到 Token 自动写入 `chrome.storage.local`，单次读取后 session 即刻擦除。

**Tech Stack:** Rust (Axum, Tauri v2, tokio), TypeScript (React, Chrome Extension Manifest V3, Web Storage API).

**Spec:** `docs/superpowers/specs/2026-10-10-extension-auto-pairing-design.md`

## Global Constraints

- 本地服务严格绑定 `127.0.0.1:39281`，CORS 白名单仅放行 `chrome-extension://` 与 `moz-extension://`。
- 绝不提供无需确认即可读取真实 Token 的接口；严禁明文持久化 session；单次读取后立即销毁。
- 代码改动必须通过：`cargo fmt --check` + `cargo clippy --workspace -- -D warnings` + `cargo test --workspace` + `pnpm run lint` + `pnpm run build`。
- 保持中英文双语国际化完整性（桌面端 `zh-CN.json` / `en.json`，插件端 `_locales/zh_CN` / `_locales/en`）。

## Review Focus

1. 会话 60 秒超时后轮询返回 `{ status: "expired" }`，不得泄漏 Token。
2. 同一 client_id 重复发起请求或新请求发起时，使旧请求安全作废，不存在内存泄露。
3. 多个扩展并发请求时全局最多仅维护 1 个未处理请求，按最新请求为准。
4. 成功轮询（approved）返回 Token 后，会话立即被一次性消耗并清除，二次请求返回 expired。
5. 桌面端用户点击拒绝后，轮询返回 `{ status: "rejected" }`。

---

### Task 1: 桌面端 HTTP 握手状态机与 `/pair/request`、`/pair/poll` 端点实现

**Files:**
- Create: `src-tauri/src/server/pairing.rs`
- Modify: `src-tauri/src/server/mod.rs`
- Modify: `src-tauri/src/server/tests.rs`

**Interfaces:**
- Produces: `PairingManager` 供 `ServerState` 与 Tauri 状态共享；
  - `pub fn new() -> Self`
  - `pub fn create_session(&self, client_id: String, browser_hint: Option<String>) -> (String, u64)` (返回 `session_id`, `expires_in`)
  - `pub fn poll_session(&self, session_id: &str, client_id: &str, token: &str) -> PairingPollResult`
  - `pub fn approve(&self, session_id: &str) -> bool`
  - `pub fn reject(&self, session_id: &str) -> bool`

- [ ] **Step 1: 编写状态机与握手端点失败测试**
  在 `src-tauri/src/server/tests.rs` 中编写测试：
  - `test_pairing_session_lifecycle`: 测试创建 session、轮询 pending、approve 授权、成功读取 token、二次读取返回 expired；
  - `test_pairing_reject_and_timeout`: 测试 reject 行为及超时过期逻辑；
  - `test_pairing_http_endpoints`: 测试未授权的 client 调用 `/pair/request` 与 `/pair/poll` 的 HTTP 往返。

- [ ] **Step 2: 运行测试验证失败**
  `cargo test -p src-tauri test_pairing`
  Expected: FAIL (模块或方法未定义)

- [ ] **Step 3: 实现 `src-tauri/src/server/pairing.rs` 并在 `server/mod.rs` 挂载路由**
  - 定义 `PairingSession`、`PairingStatus`、`PairingManager`；
  - `POST /pair/request` 路由处理函数 `pair_request_handler`；
  - `POST /pair/poll` 路由处理函数 `pair_poll_handler`；
  - 在 `create_router` 中将 `/pair/request` 与 `/pair/poll` 挂载到公开路由（与 `/health` 平级，不受 `auth_middleware` 拦截，但受 `cors` 保护）。

- [ ] **Step 4: 运行测试验证通过**
  `cargo test -p src-tauri test_pairing`
  Expected: PASS

- [ ] **Step 5: 提交代码**
  ```bash
  git add src-tauri/src/server/pairing.rs src-tauri/src/server/mod.rs src-tauri/src/server/tests.rs
  git commit -m "feat(server): 实现浏览器扩展自动配对握手协议端点"
  ```

---

### Task 2: Tauri 命令绑定与事件通知集成

**Files:**
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/src/server/mod.rs`

**Interfaces:**
- Consumes: `PairingManager`
- Produces: Tauri Commands:
  - `approve_pairing(state: State<'_, Arc<PairingManager>>, session_id: String) -> Result<(), String>`
  - `reject_pairing(state: State<'_, Arc<PairingManager>>, session_id: String) -> Result<(), String>`
- Tauri Event:
  - 广播事件 `"transly://pairing-requested"`，载荷: `{ session_id: String, browser_hint: Option<String> }`

- [ ] **Step 1: 在 `commands/mod.rs` 中增加测试**
  在 `commands/tests.rs` 或 `commands/mod.rs` 的测试模块中增加对 `approve_pairing` 与 `reject_pairing` 的单元测试。

- [ ] **Step 2: 验证测试失败**
  `cargo test -p src-tauri test_commands_pairing`
  Expected: FAIL

- [ ] **Step 3: 实现 Tauri 命令并在 App 启动时注入事件发射器**
  - 当 `/pair/request` 收到请求时，通过 Tauri `AppHandle::emit` 发送 `"transly://pairing-requested"`；
  - 暴露 `approve_pairing` 与 `reject_pairing` 并注册进 `tauri::Builder`；
  - 运行 `pnpm run bindings` 或触发 `export_bindings` 更新前端 `src/bindings.ts`。

- [ ] **Step 4: 运行测试与格式检查**
  `cargo clippy --workspace -- -D warnings && cargo test --workspace`
  Expected: PASS

- [ ] **Step 5: 提交代码**
  ```bash
  git add src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/src/main.rs src-tauri/src/server/mod.rs src/bindings.ts
  git commit -m "feat(tauri): 新增配对授权命令并广播配对申请事件"
  ```

---

### Task 3: 桌面端前端（React）授权对话框与 UI 交互

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/i18n/zh-CN.json`
- Modify: `src/i18n/en.json`

**Interfaces:**
- Consumes: Tauri Event `"transly://pairing-requested"`，`commands.approvePairing`，`commands.rejectPairing`。
- Produces: 桌面端授权弹窗组件，显示倒计时条，支持一键“允许”与“拒绝”。

- [ ] **Step 1: 补充 i18n 多语言文案**
  在 `zh-CN.json` 与 `en.json` 添加配对授权相关提示文案（如 `pairingRequestTitle`, `pairingRequestDesc`, `allow`, `reject`, `expiresIn` 等）。

- [ ] **Step 2: 在 `App.tsx` 中监听配对请求并实现弹层交互**
  - 使用 Tauri `listen('transly://pairing-requested')` 监听配对广播；
  - 收到请求时在界面置顶渲染带倒计时和脉冲效果的对话框；
  - 绑定点击“允许”触发 `commands.approvePairing(sessionId)` 并关闭浮层；
  - 绑定点击“拒绝”触发 `commands.rejectPairing(sessionId)` 并关闭浮层；
  - 倒计时归零时自动解散浮层。

- [ ] **Step 3: 验证桌面端前端编译与 Lint**
  `pnpm run lint && pnpm run build`
  Expected: PASS

- [ ] **Step 4: 提交代码**
  ```bash
  git add src/App.tsx src/i18n/zh-CN.json src/i18n/en.json
  git commit -m "feat(ui): 桌面端增加浏览器插件配对授权确认弹窗"
  ```

---

### Task 4: 插件端（extension/）自动配对与一键握手流程实现

**Files:**
- Modify: `extension/src/background/index.ts`
- Modify: `extension/src/popup/index.ts`
- Modify: `extension/popup.html`
- Modify: `extension/_locales/zh_CN/messages.json`
- Modify: `extension/_locales/en/messages.json`

**Interfaces:**
- Consumes: `POST http://127.0.0.1:39281/pair/request` 与 `POST http://127.0.0.1:39281/pair/poll`。
- Produces: 
  - `REQUEST_AUTO_PAIR`: 插件内部向 background 发起的自动配对指令；
  - 自动存储 `transly_server_token` 到 `chrome.storage.local`。

- [ ] **Step 1: 补充插件端 i18n 文案与 Popup 结构更新**
  - 在 `messages.json` 补充 `btnAutoPair`, `pairingInProgress`, `pairingRejected`, `pairingTimeout`, `pairingSuccess` 等；
  - 在 `popup.html` 的连接状态区下方增加【一键自动配对】高亮按钮，未配对时高亮提示；将手动输入框改为可展开选项。

- [ ] **Step 2: 在 `background/index.ts` 中实现握手逻辑**
  - 监听 `REQUEST_AUTO_PAIR` 消息；
  - 内部生成稳定/持久的 client_id；
  - 发送 `/pair/request`，获得 `session_id`；
  - 循环轮询 `/pair/poll`（每 1 秒一次，最多 60 次）；
  - 收到 `status === 'approved'` 时，写入 `chrome.storage.local`，并通知调用者成功；
  - 收到 `status === 'rejected'` 或超时处理错误。

- [ ] **Step 3: 在 `popup/index.ts` 中绑定按钮与状态刷新**
  - 点击【一键自动配对】时进入 loading 动画并提示“请在桌面端点击允许...”；
  - 成功时更新状态为绿色【已连接且配对有效】，隐藏自动配对按钮；
  - 失败或拒绝时给出明确文字提示。

- [ ] **Step 4: 扩展端编译与类型检查**
  `pnpm run build:extension`
  Expected: PASS

- [ ] **Step 5: 提交代码**
  ```bash
  git add extension/
  git commit -m "feat(extension): 实现一键自动配对桌面端并自动存储 Token"
  ```

---

### Task 5: 全链路端到端验证与工程门禁检查

- [ ] **Step 1: 执行所有测试套件**
  `cargo test --workspace`
  `pnpm run test`
  Expected: 全量通过

- [ ] **Step 2: 执行代码风格与静态检查**
  `cargo fmt --check`
  `cargo clippy --workspace -- -D warnings`
  `pnpm run lint`
  `pnpm run build`
  Expected: 零错误零警告

- [ ] **Step 3: 最终提交或文档更新**
  ```bash
  git status
  ```
