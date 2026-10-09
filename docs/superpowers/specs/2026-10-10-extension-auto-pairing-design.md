# 浏览器插件自动配对 Token 设计规范

## 1. 目标与背景

Transly 桌面端通过本地 HTTP 回环服务（`127.0.0.1:39281`）为浏览器扩展提供整页翻译和划词翻译调度服务，受系统凭据管理器持久化的 256-bit Bearer Token 保护。

当前痛点：用户需在桌面端复制 64 位十六进制字符串并手动粘贴至浏览器插件 Popup，操作链路割裂。
本方案目标：实现**一键/自动安全握手配对**，用户在浏览器端点击配对后，仅需在桌面端点击“允许”，即可在无键盘输入的前提下全自动完成配对与凭据同步。

## 2. 安全边界与威胁建模

1. **防静默窃取（User-in-the-loop）**：
   严禁提供任何可被未经授权扩展无感读取的裸 Token 接口。配对过程必须在桌面端原生界面展示来源提示并经用户显式点击确认。
2. **来源校验（CORS 与 Origin）**：
   `/pair/request` 与 `/pair/poll` 严格复用现有 CORS 白名单（仅放行 `chrome-extension://[a-p]{32}` 与 `moz-extension://[UUID]`）。
3. **时效性与内存隔离**：
   配对会话仅存于桌面端内存中，单次会话有效时间为 60 秒，超时自动销毁。
4. **单次消耗与防重放**：
   配对 Token 仅在桌面端确认放行后通过 `POST /pair/poll` 下发一次，读取成功后立即销毁会话。
5. **并发抑制**：
   服务端全局同一时刻仅允许一个活跃配对会话，新配对请求会使未确认的旧请求失效。

## 3. 协议接口定义

### 3.1 POST /pair/request（发起配对申请）
- **鉴权**：无（受 CORS 白名单保护）
- **Request Body**:
  ```json
  {
    "client_id": "string (UUID / 32-char hex)",
    "browser_hint": "string (e.g. Chrome, Firefox)"
  }
  ```
- **Response (200 OK)**:
  ```json
  {
    "session_id": "string (64-char hex)",
    "expires_in": 60
  }
  ```
- **服务端行为**：
  在内存状态机写入 `PairingSession`，并通过 Tauri Event `transly://pairing-requested` 广播至前端 UI。

### 3.2 POST /pair/poll（轮询配对状态）
- **鉴权**：无
- **Request Body**:
  ```json
  {
    "session_id": "string",
    "client_id": "string"
  }
  ```
- **Response (200 OK)**:
  - 待确认：`{ "status": "pending" }`
  - 已允许：`{ "status": "approved", "token": "..." }` （单次读取后立即销毁 session）
  - 已拒绝：`{ "status": "rejected" }`
  - 已过期/不存在：`{ "status": "expired" }`

## 4. 桌面端（src-tauri 与 src）职责

1. **状态存储与管理（Rust）**：
   - `PairingManager` 状态槽：`Arc<RwLock<Option<PairingSession>>>`
   - Tauri 命令：
     - `approve_pairing(session_id: String) -> Result<(), String>`
     - `reject_pairing(session_id: String) -> Result<(), String>`
2. **桌面端交互（React）**：
   - 监听 `transly://pairing-requested`，弹窗展示插件来源及倒计时（60s）。
   - 用户点击“允许”调用 `approve_pairing`，“拒绝”调用 `reject_pairing`。
   - 若主窗口处于后台，请求窗口前置聚焦以提示用户。

## 5. 扩展端（extension/）职责

1. **Popup 交互升级**：
   - 当检测到未配对（`unpaired`）或 Token 失效（`tokenExpired`）时，突出显示【一键连接桌面端】按钮。
   - 保留手动折叠输入框作为备用通道。
2. **Background 握手闭环**：
   - 发起 `/pair/request`，获取 `session_id`。
   - 每 1 秒轮询 `/pair/poll`（带 60 秒总超时）。
   - 收到 `approved` 时将 Token 存入 `chrome.storage.local`（键 `transly_server_token`），完成自动配置。
   - 向前端广播连接成功状态。
