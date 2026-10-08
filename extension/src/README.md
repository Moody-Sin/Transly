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

- **background/index.ts**：Service Worker。转发翻译请求到本地 HTTP 服务、维护每 Tab 的进度快照、驱动工具栏徽标（Badge）。
- **content/index.ts**：Content Script。整页文本节点收集、批量打包、原地 DOM 替换与还原、`MutationObserver` 动态增量监听。持有任务代际 `currentGeneration`。
- **popup/index.ts**：Popup UI 逻辑。连接探测、Token 配对、引擎/语言选择、进度回显与操作按钮。

## 消息契约

Content Script 与 Service Worker 之间使用以下消息类型（`type` 字段）：

| 消息 | 方向 | 说明 |
| :--- | :--- | :--- |
| `TRANSLATE_BATCH` | content → background | 批量翻译。仅发送 `items`（带 stable ID）；服务端保留 `texts` 兼容分支但本端不再发送 |
| `GET_TASK_PROGRESS` | popup → background | 按 `tabId` 拉取进度快照 |
| `CHECK_DESKTOP_SERVER` | popup → background | 健康检查 + `/verify_token` 鉴权探针 |
| `TRANSLATION_STARTED` / `_PROGRESS` / `_FINISHED` / `_PARTIAL_FAILED` / `_FAILED` / `_CANCELLED` / `_RESTORED` | content → background | 任务生命周期广播 |

**终态广播约定**：`TRANSLATION_FINISHED` / `_PARTIAL_FAILED` / `_FAILED` 只由**任务级入口**（首屏翻译、重试、增量批次组）在单组结束后广播一次，不在 `translateEntries` 内部广播。否则动态增量监听会为每个 pending root 各广播一次，导致徽标持续闪烁且进度百分比永不收敛。

## 构建

```sh
pnpm build        # 在 extension/ 目录下
```

产物输出到 `extension/dist`，`manifest.json` 与 `popup.html` 由 `scripts/postbuild.mjs` 复制。

## 许可证

[MIT](../../LICENSE)
