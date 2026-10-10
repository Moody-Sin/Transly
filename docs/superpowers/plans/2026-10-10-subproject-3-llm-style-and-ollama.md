# 子项目 3：LLM 翻译专业领域风格预设与本地 Ollama 预置实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 LLM 翻译引擎提供专业领域风格预设（学术/文学/商务/通用），并在设置中心提供风格切换与 Ollama 本地模型一键预置向导。

**Architecture:**
1. Rust Core：`OpenAIEngine` 引入 `TranslationStyle` 枚举，动态生成各风格专属 System Prompt。
2. UI 前端：设置中心新增翻译风格选择器与 Ollama 一键填入向导，国际化语言包对齐。

**Spec:** `docs/superpowers/specs/2026-10-10-subproject-3-llm-style-and-ollama-design.md`

## Global Constraints
- 遵循简体中文约定式提交规范。
- CI 全绿门禁。

---

### Task 1: Core 扩展 Prompt 领域风格预设 (TDD)

**Files:**
- Modify: `crates/transly-core/src/engine/openai.rs`
- Test: `crates/transly-core/src/engine/openai.rs`

- [ ] **Step 1: 编写风格提示词构建单元测试**
- [ ] **Step 2: 实现 `TranslationStyle` 与 `system_prompt` 动态构建**
- [ ] **Step 3: 运行测试验证通过**
- [ ] **Step 4: 提交代码**
```bash
git add crates/transly-core/src/engine/openai.rs
git commit -m "feat(core): 为 llm 翻译引擎增加学术、文学、商务多领域风格预设"
```

---

### Task 2: UI 设置中心新增风格选择器与 Ollama 一键预置向导

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/i18n/zh-CN.json`
- Modify: `src/i18n/en.json`

- [ ] **Step 1: 补充 i18n 键并跑通对称性单测**
- [ ] **Step 2: 在设置中心实现风格切换与 Ollama 预设按钮**
- [ ] **Step 3: 运行前端任务测试与构建**
- [ ] **Step 4: 提交代码**
```bash
git add src/App.tsx src/i18n/zh-CN.json src/i18n/en.json
git commit -m "feat(ui): 设置中心支持 llm 领域风格选择与 ollama 本地模型一键预设"
```

---

### Task 3: 全量构建与端到端回归验证

- [ ] **Step 1: 运行全量 CI 校验**
- [ ] **Step 2: 更新变更日志与设计文档**
- [ ] **Step 3: 提交文档**
```bash
git add CHANGELOG.md docs/superpowers/
git commit -m "docs: 记录 llm 风格预设与 ollama 引导实施计划与日志"
```
