# 子工程 2：浏览器扩展深度阅读与交互增强设计规范 (Spec)

## 1. 目标与背景 (Goal & Context)

为 Transly 浏览器扩展（Chromium MV3 与 Firefox Gecko）落地深度阅读与便捷交互特性：
1. **E9: 划词选区动态悬浮图标 (Selection Hover Icon)**：用户选中文本后，在光标右上角动态弹出微型轻量 Transly 图标（封闭 Shadow DOM），点击直接平滑呼出翻译气泡，无需繁琐右键菜单。
2. **E10: 整页翻译双语对照模式 (Bilingual Mode)**：在 Popup 中提供「仅译文（原地替换）」与「双语对照（原文+译文）」模式切换。双语对照模式在原正文文本节点后追加带浅色样式的兄弟译文节点。
3. **E11: 站点记忆规则（自动翻译与禁用黑名单）**：支持将当前网站域名加入“进站自动翻译”或“永不翻译此网站”规则列表，提升高频外文站点阅读效率。

---

## 2. 详细技术方案

### 2.1 E9: 划词选区动态悬浮图标 (`extension/src/selection/index.ts` / `content`)
- **生命周期**：
  - 监听宿主页面 `mouseup` 事件（防抖 80ms）；
  - 检查当前 `window.getSelection()`：若选区为空、长度 < 2 字符、或选区落在 `input`/`textarea`/`[contenteditable]` 等可编辑区域，则不展示并销毁已有悬浮图标；
  - 悬浮图标渲染在专属的封闭式 Shadow DOM（`mode: 'closed'`, `all: initial`）中；
  - 点击微型图标时，触发划词翻译并平滑过渡展开为现有划词气泡。

### 2.2 E10: 双语对照模式 (`extension/src/content/index.ts` & `popup`)
- **配置持久化**：
  - `chrome.storage.local` 存储 `transly_translate_mode: 'replace' | 'bilingual'`，Popup 提供两种模式的单选切换。
- **DOM 操作策略**：
  - `replace` 模式：沿用现有的 `node.nodeValue = translatedText` 原地替换。
  - `bilingual` 模式：保持原文本节点不变，在其后插入带 `class="transly-bilingual-trans"` 的 `<font>` 或 `<span>` 兄弟节点，设置专属浅紫/暗色样式，并在还原原文时批量清理。

### 2.3 E11: 站点规则记忆 (`extension/src/popup/` & `background`)
- **规则存储**：
  - `chrome.storage.local` 存储 `transly_site_rules: Record<string, 'auto' | 'never'>`。
- **进站自动检测**：
  - 页面初次加载时检查当前主机名（`location.hostname`）。若命中 `'auto'` 且当前未翻译，自动派发整页翻译任务；若命中 `'never'` 则禁用划词悬浮图标与提示。
  - Popup 界面底栏提供“为此域名记住自动翻译”快速切换入口。
