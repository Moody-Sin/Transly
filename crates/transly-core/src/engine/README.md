# crates/transly-core/src/engine (翻译引擎实现)

> 多翻译引擎抽象契约、主备降级与受控重试容灾体系。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

本模块定义了 Transly 核心翻译引擎 trait 以及各外部与本地翻译适配器的具体实现，纯 Rust 构建，零 UI 与平台依赖。

## 目录

- [结构](#结构)
- [核心机制](#核心机制)
- [测试](#测试)
- [许可证](#许可证)

## 结构

- **mod.rs**：核心 `TranslateEngine` trait 定义，规范单条翻译、流式输出（`translate_stream`）与批量分片契约，内置指数退避受控重试逻辑。
- **google.rs**：`GoogleWebEngine` 免密钥网页翻译适配，实现多段换行自动切块与防超长分片。
- **deepl.rs**：`DeepLEngine` 官方 API 适配，支持 Free/Pro 双端点切换与 50 项批量分片。
- **openai.rs**：`OpenAIEngine` 大模型翻译适配（支持 OpenAI 与 DeepSeek），支持自定义 Base URL 端点、模型名及 SSE 流式打字机输出。
- **fallback.rs**：`FallbackEngine` 主备容灾代理包装器，当主引擎网络故障时平滑切换至备用引擎，密钥缺失等确定性鉴权错误向上真实冒泡。

## 核心机制

1. **瞬态网络错误重试**：底层实现 `send_with_retry`，对网络中断、超时及 429/500/502/503/504 等临时故障执行至多 1 次指数退避重试；401/403 等鉴权与输入错误绝不重试。
2. **零明文 Key 传入**：API 密钥由外部宿主层在调用时以结构化凭据注入，本模块不读取平台 Keyring，不持久化敏感信息。

## 测试

```sh
cargo test -p transly-core engine
```

## 许可证

[MIT](../../../../LICENSE)
