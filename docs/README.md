# docs (设计与架构文档)

> Transly 的架构决策与跨端边界说明文档。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

本目录存放不属于「使用说明」范畴、但维护者必须了解的设计依据。使用说明见根目录 `README.md`，工程规范见 `PROJECT_RULES.md`，踩坑记录见 `Lessons Learned.md`。

## 目录

- [文档索引](#文档索引)
- [撰写约定](#撰写约定)
- [许可证](#许可证)

## 文档索引

### 架构与长期有效

- **[architecture-state-machines.md](architecture-state-machines.md)**：桌面端与浏览器扩展两套任务状态机的语义对比、为什么不跨进程合并、以及各自的代际隔离边界。

### 评审报告

- **[2026-10-11-full-audit-review.md](2026-10-11-full-audit-review.md)**：全量代码评审报告（core / 平台层 / 前端 / 插件 / 基建五路），含实测基线、11 条 P0 与 22 条 P1，含「结论在什么情况下会失效」的复核说明。

### 实施计划（`docs/superpowers/plans/`）

按时间倒序。计划类文档是**当时**的决策快照，落地后不再回改；实际行为以代码与 CHANGELOG 为准。

- **[2026-10-12-remove-auto-fallback-plan.md](superpowers/plans/2026-10-12-remove-auto-fallback-plan.md)**：彻底移除跨翻译引擎自动降级（Fallback）功能的实施计划。
- **[2026-10-11-selection-replace-engine-and-playwright-e2e-plan.md](superpowers/plans/2026-10-11-selection-replace-engine-and-playwright-e2e-plan.md)**：划词原地替换首选引擎与浏览器插件 Playwright 端到端集成测试实施计划。
- **[2026-10-11-audit-optimizations-and-enhancements.md](superpowers/plans/2026-10-11-audit-optimizations-and-enhancements.md)**：全量评审发现项的落地实施计划。
- **[2026-10-10-core-productivity-enhancements-plan.md](superpowers/plans/2026-10-10-core-productivity-enhancements-plan.md)**：核心生产力增强（历史搜索导出、TTS、深色模式、快捷键录制、图钉置顶等）的实施计划。
- **[2026-10-10-smart-lang-detect-and-inplace-replace.md](superpowers/plans/2026-10-10-smart-lang-detect-and-inplace-replace.md)**：离线语言检测与划词原地替换的实施计划。
- **[2026-10-10-extension-auto-pairing.md](superpowers/plans/2026-10-10-extension-auto-pairing.md)**：扩展与桌面端一键自动配对握手的实施计划。
- **[2026-10-10-subproject-1-core-algorithms-and-experience.md](superpowers/plans/2026-10-10-subproject-1-core-algorithms-and-experience.md)**：子项目一（核心算法与体验）。
- **[2026-10-10-subproject-2-storage-concurrency-and-shadow-dom.md](superpowers/plans/2026-10-10-subproject-2-storage-concurrency-and-shadow-dom.md)**：子项目二（存储并发与 Shadow DOM）。
- **[2026-10-10-subproject-3-llm-style-and-ollama.md](superpowers/plans/2026-10-10-subproject-3-llm-style-and-ollama.md)**：子项目三（LLM 风格预设与 Ollama）。

### 设计稿（`docs/superpowers/specs/`）

计划文档对应的设计依据，解释「为什么这样设计」。

- **[2026-10-12-remove-auto-fallback-design.md](superpowers/specs/2026-10-12-remove-auto-fallback-design.md)**：彻底移除跨引擎自动降级机制、杜绝静默篡改与双重连接池缓存的设计依据。
- **[2026-10-11-selection-replace-engine-and-playwright-e2e-design.md](superpowers/specs/2026-10-11-selection-replace-engine-and-playwright-e2e-design.md)**：划词原地替换首选引擎配置及直接报错提醒机制，与 Playwright 插件 E2E 测试的设计依据。
- **[2026-10-10-core-productivity-enhancements-design.md](superpowers/specs/2026-10-10-core-productivity-enhancements-design.md)**：核心生产力增强的设计依据。
- **[2026-10-10-extension-auto-pairing-design.md](superpowers/specs/2026-10-10-extension-auto-pairing-design.md)**：自动配对握手协议的设计依据。
- **[2026-10-10-extension-deep-reading-enhancements-design.md](superpowers/specs/2026-10-10-extension-deep-reading-enhancements-design.md)**：扩展深读增强（双语对照、站点记忆、划词图标）的设计依据。
- **[2026-10-10-multi-engine-and-glossary-eco-design.md](superpowers/specs/2026-10-10-multi-engine-and-glossary-eco-design.md)**：多引擎与术语库生态的设计依据。
- **[2026-10-10-subproject-1-core-algorithms-and-experience-design.md](superpowers/specs/2026-10-10-subproject-1-core-algorithms-and-experience-design.md)**：子项目一设计依据。
- **[2026-10-10-subproject-2-storage-concurrency-and-shadow-dom-design.md](superpowers/specs/2026-10-10-subproject-2-storage-concurrency-and-shadow-dom-design.md)**：子项目二设计依据。
- **[2026-10-10-subproject-3-llm-style-and-ollama-design.md](superpowers/specs/2026-10-10-subproject-3-llm-style-and-ollama-design.md)**：子项目三设计依据。

## 撰写约定

- 每新增一篇文档，必须在本索引中登记，否则无人能发现它。
- 文档必须回答「为什么这样设计」以及「什么情况下这个结论会失效」，只复述代码的文档不予收录。

## 许可证

[MIT](../LICENSE)
