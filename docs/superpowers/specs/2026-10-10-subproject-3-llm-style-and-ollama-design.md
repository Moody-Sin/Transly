# 技术规范：子项目 3 - LLM 翻译专业领域风格预设与本地 Ollama 预置

- **版本**: 1.0.0
- **状态**: Approved
- **日期**: 2026-10-10
- **涉及子系统**: `crates/transly-core/src/engine/openai.rs` (Rust), `src/App.tsx`, `src/i18n/` (TypeScript)

---

## 1. 目标与背景

当前 LLM 翻译引擎采用写死的单一系统提示词，无法满足不同专业领域的特定要求：
- 学术论文 / 技术工程：需要保持严谨学术语调，原样保留术语、代码符号与缩写；
- 文学小说 / 影视字幕：需要地道、优美流畅的意译与润色；
- 商务公文 / 邮件往来：需要得体、礼貌的商务口吻。
同时，用户在无网络或离线涉密场景下需要连接本地大模型（Ollama），在设置中心缺乏一键填充向导。

---

## 2. 详细技术方案

### 2.1 核心层风格提示词扩展 (`crates/transly-core/src/engine/openai.rs`)
- 定义领域风格枚举 `TranslationStyle`:
  - `General` (通用直译，默认)
  - `Academic` (学术论文 / 技术文档：保留专业术语，严谨精准)
  - `Literary` (文学润色 / 日常口语：地道自然，符合目标语言表达习惯)
  - `Business` (商务公文 / 正式邮件：得体专业)
- 在 `OpenAIEngine` 中增加 `set_style` 与 `with_style` 支持，动态生成对应 System Prompt。

### 2.2 前端与平台配置集成 (`src/App.tsx`, `src/i18n/`)
- 在设置中心提供 LLM 风格切换下拉框（通用、学术技术、文学润色、商务公文），持久化至 `localStorage`。
- 在 OpenAI / DeepSeek 端点配置卡片中新增【Ollama 本地大模型预设】快捷按钮：一键填入 `http://localhost:11434/v1` 与常用模型名（如 `qwen2.5:7b`），引导用户免配置直连本地模型。
