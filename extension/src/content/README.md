# extension/src/content (整页原地翻译 Content Script)

> 网页 DOM 节点收集、视口优先切批、原地无损替换与动态增量监听。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

本模块作为按需注入到目标网页的 Content Script，承载 Transly 浏览器的核心体验——**“整页翻译仅译文”**。在完全保留原网页样式与排版的前提下，执行高保真原地 DOM 文本节点替换，并支持一键无感还原。

## 目录

- [核心能力](#核心能力)
- [关键机制](#关键机制)
- [性能优化](#性能优化)
- [许可证](#许可证)

## 核心能力

- **原地仅译文替换**：定位文本节点直接更新 `nodeValue`，记录原始文本到内存 `WeakMap` 与节点自定义属性，保证排版 100% 保真与一键无损还原。
- **双语对照模式**：在目标节点后追加带样式隔离的兄弟译文节点，保留原文的同时兼顾对照阅读；还原时自动彻底销毁。
- **动态增量监听 (MutationObserver)**：自适应 SPA 单页应用及无限滚动瀑布流，增量补译动态加载内容；在还原或重译时彻底断开并清空。
- **悬浮查看原文 Tooltip**：替换后在译文父节点标记属性，鼠标悬停时在封闭式 Shadow DOM 浮层中呈现原文字句。

## 关键机制

1. **代际闸门与原子性写回**：持有任务代际 `currentGeneration`。用户点击“还原原文”时代际立即递增；在途 HTTP 响应在写入 DOM 前执行严格代际比对，已作废代际的响应坚决不写入页面。
2. **富文本段落保真**：段落内嵌行内标签（`<a>`、`<code>`、`<strong>`）整句打包占位翻译，并基于显式 `isInline` 属性安全拓扑回填，占位丢失时安全阻断，杜绝 DOM 错位与标签颠倒。
3. **开放式 Shadow DOM 穿透**：流式递归下探 Web Components 内部的 open ShadowRoot，完整覆盖现代组件库页面。
4. **终态广播一次约定**：任务级终态（完成 / 部分失败 / 失败）在整组批次结束后广播一次，不在子批次内频繁广播，防止扩展徽标持续闪烁。

## 性能优化

- **TreeWalker 流式遍历**：摒弃全局大数组分配，使用原生 TreeWalker 流式收集文本节点。
- **视口感知优先切批 (Viewport-First)**：先期计算 DOM 包围盒，将当前屏幕可视文本优先切批外呼，离屏段落保序排队，感官延迟直降。
- **包围盒计算 WeakMap 记忆**：对父容器的包围盒几何检测进行单趟记忆，避免连续触发浏览器强制同步重排（Forced Synchronous Layout）。

## 许可证

[MIT](../../../LICENSE)
