# extension/src/selection (选区划词悬浮图标与 Shadow DOM 气泡)

> 选中文本后悬浮微型图标与译文气泡，在封闭式 Shadow DOM 中隔离渲染。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

本模块按需注入到目标网页中，负责处理网页内的选区划词翻译体验。包含光标右上角动态浮现的微型 Transly 悬浮图标，以及展开后的只读译文气泡。

## 目录

- [特性](#特性)
- [安全与样式隔离](#安全与样式隔离)
- [生命周期与代际控制](#生命周期与代际控制)
- [许可证](#许可证)

## 特性

- **微型悬浮图标**：在网页选中文本后，光标右上角平滑浮现微型 Transly 图标，用户点击直接展开译文，免去右键菜单繁琐操作。
- **右键菜单划词联动**：支持通过右键「Transly 翻译选中文本」直接弹出气泡。
- **译文交互**：气泡内支持一键复制译文、展示当前引擎/语言标签，按 `Esc` 键或点击气泡外部自动关闭。

## 安全与样式隔离

1. **封闭式 Shadow DOM 渲染**：图标与气泡均挂载在 `mode: 'closed'` 的 ShadowRoot 下，注入 `all: initial` 重置样式。宿主网页的 CSS 无法渗透污染气泡外观，宿主页面的恶意脚本亦无法访问气泡 DOM。
2. **整页翻译防误伤**：宿主挂载节点带有 `data-no-translate` 标记并挂载于 `documentElement`（而非 `body`），整页翻译的 TreeWalker 遍历从源头排除气泡自身文案。
3. **选区几何坐标计算**：结合 `window.getSelection().getRangeAt(0).getBoundingClientRect()` 与页面滚动偏移量精确定位锚点，锚点包含选区高度，气泡平滑置于选区下方，防止遮挡待读文本。

## 生命周期与代际控制

- 内部维护 `bubbleGeneration` 代际号。用户连续快速划词或重新发起翻译时，作废旧在途响应，防止迟到的网络响应错乱覆盖当前选区内容。

## 许可证

[商业专有许可](../../../LICENSE)
