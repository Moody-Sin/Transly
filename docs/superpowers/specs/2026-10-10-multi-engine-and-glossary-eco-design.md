# 子工程 3：专业多引擎对比与术语生态包设计规范 (Spec)

## 1. 目标与背景 (Goal & Context)

为 Transly 桌面端落地多引擎同屏并发对比翻译与术语库定向语言绑定、CSV 批量导入/导出功能：
1. **E1: 多引擎并发对比翻译 (Multi-Engine Comparison)**：在主界面增加「多引擎对照」模式，一次输入，同时并发调用 Google、DeepL、DeepSeek 或已配置的 LLM，分栏呈现各引擎译文，支持分别复制或朗读。
2. **E3: 术语库定向语言绑定与 CSV 批量导入/导出**：在术语库中支持设置生效的语言对（如仅在 `en -> zh-CN` 生效），并在设置中心提供 CSV 文件的批量导入与导出。

---

## 2. 详细技术方案

### 2.1 E1: 多引擎并发对比模式 (`src/App.tsx`)
- 在主界面控制栏增加一个切换开关：「单引擎 / 多引擎对照 (Compare)」。
- 开启多引擎对照时，输入文本后，前端并发调用：
  - `commands.translate(text, 'auto', targetLang, 'google')`
  - 若已配置 DeepL，并发调用 `commands.translate(text, 'auto', targetLang, 'deepl')`
  - 若已配置 DeepSeek/OpenAI，并发调用 `commands.translate(text, 'auto', targetLang, 'deepseek')`
- 右侧译文区域由单栏自适应切换为多栏网格（Grid），每个引擎卡片拥有独立的译文展示、引擎标签、复制与发音按钮。

### 2.2 E3: 术语库 CSV 批量导入与导出 (`src/App.tsx` & `transly-core`)
- **CSV 导出**：将当前所有术语条目导出为 `source,target,match_case` 的 CSV 文件。
- **CSV 导入**：用户选择本地 CSV 文件，前端解析每一行并通过 `commands.addGlossaryTerm` 批量写入，自动去重与持久化。
