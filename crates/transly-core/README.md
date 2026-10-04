# transly-core

> Transly 核心翻译、缓存、语言检测与术语库逻辑（纯 Rust，无 Tauri 依赖）。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

本 crate 是 Transly 的底层核心库，可独立供桌面端、命令行工具以及未来潜在的 WASM 环境复用。

## 目录

- [架构](#架构)
- [模块](#模块)
- [测试](#测试)
- [许可证](#许可证)

## 架构

- **engine**: 定义了可扩展的 `TranslateEngine` trait 与多引擎适配。
- **cache**: SQLite (WAL 模式) 持久化缓存，以 SHA-256 为哈希主键，降低外部请求与网络开销。
- **detect**: 轻量语言识别能力。
- **glossary**: 专有名词与自定义术语库匹配机制。
- **config**: 全局配置模型定义。

## 测试

```sh
cargo test -p transly-core
```

## 许可证

[MIT](../../LICENSE)
