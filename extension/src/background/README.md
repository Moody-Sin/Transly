# extension/src/background (扩展后台 Service Worker)

> MV3 后台 Service Worker：负责与本地桌面端通信、翻译请求代理与工具栏徽标驱动。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

本模块运行于浏览器后台 Service Worker 上下文中。作为扩展各前端入口（Content Script、Popup、Selection Bubble）与本地 Transly 桌面端 HTTP 服务的唯一代理网关，确保页面上下文不直连网络与桌面服务。

## 目录

- [职责](#职责)
- [核心机制](#核心机制)
- [消息处理](#消息处理)
- [许可证](#许可证)

## 职责

- **HTTP 通信中继**：代理发送 `POST /translate`、`GET /health`、`GET /verify_token` 等请求，并自动附带存储中的 Bearer Token。
- **端口自适应与测试支持**：支持从本地配置动态读取服务端口或 URL，回环权限放宽至 `http://127.0.0.1/*`，确保 E2E 测试与多实例不发生端口冲突。
- **状态维护与徽标驱动**：跟踪每个标签页（`tabId`）的整页翻译生命周期与进度快照，动态更新扩展图标角标（Badge，如 `50%`、`OK`、`WARN`）。
- **右键菜单集成**：初始化并监听右键菜单「Transly 翻译选中文本」，按需向当前标签页注入划词脚本。

## 核心机制

1. **瞬态重试与指数退避**：在请求本地服务遇到瞬态网络异常时，实施受控重试（至多 1 次退避重试），401/403 鉴权错误立即向上抛出。
2. **切批上限缓存**：在 Service Worker 启动阶段拉取桌面端 `/health` 接口获取最新的 `max_items`、`max_total_chars`、`max_item_chars` 并缓存，供 Content Script 规划切批。

## 消息处理

- `TRANSLATE_BATCH`: 接收 Content Script 的批次数据并转发至桌面端。
- `GET_BATCH_LIMITS`: 返回已缓存的批量上限。
- `GET_TASK_PROGRESS`: 响应 Popup 的当前 Tab 进度查询。
- `CHECK_DESKTOP_SERVER`: 连通性探测。
- `TRANSLATION_STARTED` / `_PROGRESS` / `_FINISHED` / `_PARTIAL_FAILED` / `_FAILED`: 接收任务状态并驱动 Badge。

## 许可证

[MIT](../../../LICENSE)
