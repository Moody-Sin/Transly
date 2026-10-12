# transly-core

> Transly 核心翻译、缓存、语言代码校验与术语库逻辑（纯 Rust，无 Tauri 依赖）。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

本 crate 是 Transly 的底层核心库，可独立供桌面端、命令行工具以及未来潜在的 WASM 环境复用。

## 目录

- [架构](#架构)
- [测试](#测试)
- [许可证](#许可证)

## 架构

- **engine**: 定义了可扩展的 `TranslateEngine` trait 与多引擎适配（`GoogleWebEngine`、`DeepLEngine`、`OpenAIEngine`），支持单条、流式 SSE、批量逐项容错与瞬态指数退避重试。
- **cache**: SQLite (WAL 模式) 读写分离持久化缓存与历史沉淀，以 SHA-256 为哈希主键，内置自动衰减与上限清理，支持全量历史流式导出（CSV / JSON）。
- **detector**: 纯 Rust 离线多语言检测（基于 Unicode 脚本与西欧拉丁语系特征加权）与中英智能双向对转路由。
- **preserve**: Markdown 多行与行内代码块 Unicode 专有括线（`⟦TRANSLY_CODE_N⟧`）占位保护与容错还原。
- **glossary**: 专有名词与术语库匹配机制，基于 `aho-corasick` 单趟自动机流式扫描，支持忽略大小写与中西文词边界保护。
- **proxy**: 网络代理契约模型（`ProxyMode`、`ProxyConfig`）与代理 URL 规范化解析。
- **ocr**: OCR 几何文本行聚合重构、像素预算限制与位图跨通道处理。
- **task**: 核心批处理契约常量（`MAX_BATCH_ITEMS`、`MAX_BATCH_TOTAL_CHARS`、`MAX_ITEM_CHARS`）与批次切分规划。
- **validation**: 语言代码与引擎标识的全局严格验证与规范化。
- **config**: 全局配置模型定义。

## 测试

```sh
cargo test -p transly-core
```

## 许可证

[MIT](../../LICENSE)
