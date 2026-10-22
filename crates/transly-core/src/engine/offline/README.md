# `offline` —— 全离线混合翻译引擎

> `crates/transly-core` 纯逻辑离线模块，结合内置 Aho-Corasick 高频词库矩阵与本地外部模型发现。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

## 目录

- [背景](#背景)
- [架构设计](#架构设计)
- [词库与匹配特性](#词库与匹配特性)
- [模型扩展](#模型扩展)
- [维护者](#维护者)
- [许可证](#许可证)

## 背景

传统翻译工具在断网或局域网保密环境下通常完全瘫痪，或需要用户预先下载数 GB 且极耗显存的庞大模型。
本引擎采用多层混合离线架构：内置微秒级纯 Rust 词库矩阵保障基础断网翻译；同时预留外部模型探测契约，支持无缝加载用户自备的量化本地模型。

## 架构设计

```
┌──────────────────────────────────────────────┐
│  OfflineEngine (本地进程内调度)               │
├──────────────────────┬───────────────────────┤
│  层级 1: 内置词库矩阵  │  层级 2: 外部模型扫描  │
│  - Aho-Corasick 自动机│  - models/ 目录探测   │
│  - 词边界与标点智能对转│  - ONNX / GGUF 识别   │
└──────────────────────┴───────────────────────┘
```

## 词库与匹配特性

1. **确定性最长左匹配**：采用 `MatchKind::LeftmostLongest`，多词短语优先于单词（如优先匹配「artificial intelligence」而非单个「intelligence」）。
2. **西文单词边界防护**：匹配前后进行字符判定，杜绝词内子串被误替换（如 `cat` 绝不误伤 `concatenate`）。
3. **标点符号智能对转**：英译中自动将西文半角标点转为全角；中译英自动将全角标点转为半角加空格。

## 模型扩展

自动扫描宿主注入的 `models/` 目录，探测 `.onnx`、`.bin`、`.gguf` 模型文件。未发现外部模型时平滑运行内置词库模式，绝不抛错。

## 维护者

[@Moody-Sin](https://github.com/Moody-Sin)

## 许可证

[Apache-2.0](../../../LICENSE)
