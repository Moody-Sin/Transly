# src-tauri/src/server (浏览器扩展本地 HTTP 服务)

> 基于 Axum 的轻量本地回环通信服务、安全配对状态机与 DNS Rebinding 防护。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

本模块在本地宿主监听 `127.0.0.1:39281`，作为桌面端核心与浏览器扩展之间的唯一桥梁。浏览器扩展所有整页翻译与划词翻译请求均经由本服务统一调度，共享桌面端的 SQLite 缓存与 API 密钥。

## 目录

- [结构与端点](#结构与端点)
- [安全防御架构](#安全防御架构)
- [测试](#测试)
- [许可证](#许可证)

## 结构与端点

- **mod.rs**：Axum 服务构建与生命周期管理，提供以下端点：
  - `GET /health`：无鉴权健康探针，公开服务端批量上限（`max_items`、`max_total_chars`、`max_item_chars`），供扩展端切批规划对齐。
  - `POST /translate`：批量翻译端点，强制 Bearer Token 强鉴权与批次体积限制（2MB、50,000 字符上限）。
  - `GET /verify_token`：Token 有效性快速校验探针。
  - `POST /pair/request` 与 `GET /pair/poll`：自动安全配对握手端点。
- **pairing.rs**：`PairingManager` 状态机。管理会话 ID、60 秒超时控制、单次读取销毁及 `MAX_PENDING_SESSIONS = 16` 防泛洪淘汰。
- **tests.rs**：集成测试套件，包含对 Bearer 鉴权、CORS、Host 守卫、配对生命周期及真实 TCP 端口监听全流程的自动化验证。

## 安全防御架构

1. **Host 守卫与 DNS Rebinding 阻断**：独立于 CORS 的 `host_guard` 中间件，仅放行 `Host: 127.0.0.1` / `localhost` / `[::1]`（含端口），拦截恶意网站通过域名解析指向 `127.0.0.1` 发起的同源穿透。
2. **Bearer Token 恒定时间比较**：内部基于 `subtle::ConstantTimeEq` 比对请求头中的 Bearer Token，消除计时攻击隐患；空 Token 强制 fail-closed 拦截。
3. **CORS 来源格式隔离**：仅放行符合浏览器扩展协议（`chrome-extension://`、`moz-extension://`）的请求。
4. **受控并发与 DoS 防御**：基于 `tokio::sync::Semaphore` 限制并发，超大请求体（>2MB）直接拒绝。

## 测试

```sh
cargo test -p src-tauri server::tests
```

## 许可证

[MIT](../../../LICENSE)
