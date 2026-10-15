# extension/src (浏览器扩展源码)

> Manifest V3 扩展的三个执行上下文：Service Worker、Content Script、Popup。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

三个上下文各自运行在独立进程/环境，只能通过 `chrome.runtime.sendMessage` 通信。本目录的代码**不持有任何 API Key，也不直连外网翻译接口**，所有翻译统一经 `127.0.0.1:39281` 交由桌面端调度。

## 目录

- [上下文划分](#上下文划分)
- [消息契约](#消息契约)
- [构建](#构建)
- [许可证](#许可证)

## 上下文划分

- **background/index.ts**：Service Worker。负责与本地 HTTP 服务通信（端口自适应）、维护每 Tab 的翻译进度快照、自动重试与驱动工具栏徽标（Badge）。
- **content/index.ts**：Content Script。整页文本节点收集（流式 TreeWalker 与视口优先切批）、原地 DOM 替换与一键还原、`MutationObserver` 动态增量监听与悬停查看原文 Tooltip。持有任务代际 `currentGeneration`。
- **selection/index.ts**：Content Script（按需注入）。划词微型悬浮图标与只读译文气泡（均渲染在封闭式 Shadow DOM 中），支持一键复制与快捷关闭，持有气泡代际 `bubbleGeneration`。
- **popup/index.ts**：Popup UI 逻辑。连接探测、一键自动安全握手配对、引擎/语言选择、双语对照模式切换、站点自动翻译记忆与操作反馈。
- **shared/**：跨入口共享纯逻辑模块，附带 `node:test` 单测：
  - `batchPlan.ts`：批量切批规划（按项数与字符总量双上限切批，与 Rust 侧 `plan_batch_ranges` 严格对齐）。
  - `domTraversal.ts`：DOM 遍历与开放模式 Shadow DOM 递归穿透、节点类型守卫。
  - `richText.ts`：富文本段落行内标签保真聚合、占位保护与拓扑还原。
- **i18n/index.ts**：`chrome.i18n` 文案查询封装与静态节点国际化属性注入（`data-i18n` / `data-i18n-placeholder` / `data-i18n-title`）。

## 消息契约

Content Script 与 Service Worker 之间使用以下消息类型（`type` 字段）：

| 消息 | 方向 | 说明 |
| :--- | :--- | :--- |
| `TRANSLATE_BATCH` | content → background | 批量翻译。仅发送 `items`（带 stable ID）；服务端保留 `texts` 兼容分支但本端不再发送。失败响应携带 HTTP `status`，413 时 content 侧对半拆批重试 |
| `GET_BATCH_LIMITS` | content → background | 读取服务端批量上限（background 从 `/health` 拉取并缓存），供 `planBatchRanges` 切批 |
| `GET_TASK_PROGRESS` | popup → background | 按 `tabId` 拉取进度快照 |
| `CHECK_DESKTOP_SERVER` | popup → background | 健康检查 + `/verify_token` 鉴权探针 |
| `PING_SELECTION_SCRIPT` | background → selection | 划词脚本注入探测（与 `PING_CONTENT_SCRIPT` 同理，避免重复注入重置气泡状态） |
| `SHOW_SELECTION_TRANSLATION` | background → selection | 投递待翻译选中文本 + 目标语言 / 引擎，由 selection 侧渲染气泡并发起 `TRANSLATE_BATCH` |
| `TRANSLATION_STARTED` / `_PROGRESS` / `_FINISHED` / `_PARTIAL_FAILED` / `_FAILED` / `_CANCELLED` / `_RESTORED` | content → background | 任务生命周期广播 |

**终态广播约定**：`TRANSLATION_FINISHED` / `_PARTIAL_FAILED` / `_FAILED` 只由**任务级入口**（首屏翻译、重试、增量批次组）在单组结束后广播一次，不在 `translateEntries` 内部广播。否则动态增量监听会为每个 pending root 各广播一次，导致徽标持续闪烁且进度百分比永不收敛。

**切批约定**：`shared/batchPlan.ts` 的 `planBatchRanges` 按项数与字符总量双上限切批，上限由 `/health` 提供。禁止在 content 侧写死项数（旧实现写死 30，与服务端的字符总量上限量纲不同，导致整批 413）。

**划词气泡的选区归属**：右键菜单回调只能拿到 `info.selectionText`，无法拿到选区几何位置，因此锚点坐标由 selection 侧读取 `window.getSelection()` 的 Range 矩形获得；文本同样优先取页面实时选区，仅在取不到时回落到 `selectionText`。滚动/缩放时按「记录时的滚动偏移」回推视口坐标重新定位。

## 构建

```sh
pnpm build        # 在 extension/ 目录下
```

产物输出到 `extension/dist`。构建入口是 `scripts/build.mjs`（**不要直接 `vite build`**）：四个执行上下文逐个独立构建，保证每个产物自包含——`chrome.scripting.executeScript` 注入的 `content.js` / `selection.js` 与 `popup.html` 的经典 `<script>` 都不支持 ESM 导入，单次多入口构建会把 `i18n` 这类共享模块拆成独立 chunk 并在产物顶部写入 `import`，导致扩展能加载但功能静默失效。`scripts/build.mjs` 在构建后断言产物不含 ESM 导入、且不存在 `dist/assets/`。`manifest.json`、`popup.html` 与 `_locales/` 由 `scripts/postbuild.mjs` 复制。

## 许可证

[商业专有许可](../../LICENSE)
