# crates/transly-core/src (核心模块源码)

> Transly 核心翻译、缓存、离线语言探测、术语库与代码保真算法源码。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

本目录为 `transly-core` crate 的源码实现。遵循纯 Rust、零平台原生 API 依赖与零 UI 绑定的原则，对外提供无状态且线程安全的纯函数与持久化组件。

## 目录

- [源码结构](#源码结构)
- [核心设计原则](#核心设计原则)
- [测试](#测试)
- [许可证](#许可证)

## 源码结构

- **lib.rs**：模块声明与全局常量（`MAX_BATCH_ITEMS`、`MAX_BATCH_TOTAL_CHARS`、`MAX_ITEM_CHARS`）统一重导出。
- **engine/**：翻译引擎抽象 trait (`TranslateEngine`) 及 Google Web、DeepL、OpenAI / DeepSeek 实现。
- **ocr/**：OCR 跨平台统一 DTO、几何行聚合与 4000 万像素防 OOM 硬预算守卫。
- **cache.rs**：SQLite (WAL 模式) 读写分离持久化缓存管理、90 天衰减清理与全量历史数据后台流式导出（CSV/JSON）。
- **detector.rs**：纯 Rust 离线多语言检测（基于 Unicode 脚本与西欧拉丁语系特征加权）与中英智能双向对转。
- **preserve.rs**：Markdown 多行代码块与行内代码 Unicode 专有括线（`⟦TRANSLY_CODE_N⟧`）占位保护与容错还原。
- **glossary.rs**：专有名词与自定义术语库，采用 `aho-corasick` 单趟自动机流式扫描，支持大小写敏感/不敏感与西文/CJK 词边界保护。
- **proxy.rs**：跨平台网络代理契约模型（`ProxyMode`、`ProxyConfig`）与代理 URL 规范化。
- **task.rs**：批处理任务分片与容量规划常量定义。
- **validation.rs**：输入文本、语言代码与引擎标识的全局严格校验。
- **config.rs**：全局配置模型。
- **error.rs**：基于 `thiserror` 的核心结构化错误枚举与前端 TS 错误分类导出。

## 核心设计原则

1. **绝对纯净与无状态**：严禁引入 Tauri 或平台特定 API（如 WinRT、Keychain）。所有文件系统路径均由外部宿主注入。
2. **单一契约来源**：批量限制与批次规划切分常量全工程唯一定义于此，杜绝跨端数字漂移。

## 测试

```sh
cargo test -p transly-core
```

## 许可证

[MIT](../../../LICENSE)
