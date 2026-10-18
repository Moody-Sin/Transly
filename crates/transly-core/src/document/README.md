# `document` —— 文档与字幕翻译模块

> `crates/transly-core` 底层纯逻辑模块，负责多格式文档与字幕文件的解析、待翻译块提取与原样保真重构。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

## 目录

- [背景](#背景)
- [支持格式](#支持格式)
- [核心架构与流程](#核心架构与流程)
- [硬性约束](#硬性约束)
- [维护者](#维护者)
- [许可证](#许可证)

## 背景

传统文档翻译往往直接粗暴地按全文翻译，容易导致 Markdown 代码块被破坏、LaTeX 公式语法被篡改、YAML Frontmatter 元数据丢失，或者导致 SRT 字幕的时间轴和序号错位崩溃。
本模块提供统一的纯 Rust 无平台依赖解析与重构流水线，在进入翻译前精确抽离非正文结构，翻译后原样回填。

## 支持格式

| 格式 | 后缀 | 特性与保真机制 |
|---|---|---|
| **纯文本** | `.txt` | 识别连续非空段落切分，保留段落间任意空行与换行符（CRLF/LF 保真） |
| **Markdown** | `.md`, `.markdown` | 免翻保护 YAML Frontmatter；通过 `stash_code_blocks` 保护代码围栏、行内代码与数学公式 |
| **SubRip 字幕** | `.srt` | 严格锁定序号行与时间轴行（`00:00:00,000 --> 00:00:00,000`），仅回填字幕译文 |

## 核心架构与流程

1. **解析提取 (`parse_*`)**：将文档切分为待翻译正文块（`Vec<String>`）和一个拥有所有权、完全包含还原所需信息的重构闭包（`DocumentReconstructor`）。
2. **切批调度 (`translate_document_content`)**：
   - 过滤纯空白块（空白块直接透传，规避引擎空输入错误）；
   - 借助 `plan_batch_ranges` 将非空块严格切分为符合 `MAX_BATCH_ITEMS`（30）与 `MAX_BATCH_TOTAL_CHARS`（50,000）的批次；
   - 调用 `translate_batch_with_cache` 享受本地 SQLite/L1 缓存与原子事务写入；
   - 逐项容错隔离：单个块失败时安全回退至该块原文，绝不抛出整篇文档失败。
3. **闭包重构**：将译文块与透传块按原始索引回填至定界槽位，输出保真重构文档。

## 硬性约束

- **纯 Rust 核心**：本模块禁止引入任何 Tauri 或宿主依赖。
- **Fail-Closed 容错**：翻译局部出错绝不得破坏原文档物理结构或抛弃未翻译段落。

## 维护者

[@Moody-Sin](https://github.com/Moody-Sin)

## 许可证

[Apache-2.0](../../LICENSE)
