# extension/src/shared (跨执行上下文共享纯逻辑)

> 纯函数切批规划、开放式 Shadow DOM 遍历与富文本段落保真算法集合。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

本目录承载被 Content Script、Service Worker 或测试套件复用的纯函数算法与共享数据契约，包含完善的 Node 原生测试套件。

## 目录

- [模块构成](#模块构成)
- [核心算法规程](#核心算法规程)
- [测试](#测试)
- [许可证](#许可证)

## 模块构成

- **batchPlan.ts**：批量切批规划器 `planBatchRanges`。严格遵守项数上限（`maxItems`）与字符总量上限（`maxTotalChars`）双重契约切批，语义与 Rust 后端 `plan_batch_ranges` 严格对称；支持视口（Viewport）优先拆批。
- **domTraversal.ts**：DOM 深度遍历器 `iterateElementsDeep` 与 `iterateTextNodesDeep`。支持流式递归下探开放模式（`open`）Shadow DOM，并放行 `DOCUMENT_FRAGMENT_NODE`（11）容器。
- **richText.ts**：富文本段落行内标签保真拆装。提取段落内嵌入的行内标签（`<a>`、`<code>`、`<strong>`）并生成 Unicode 括线占位符（`⟦T_N⟧`），基于显式的 `isInline` 属性安全拓扑还原，防止跨语种倒装破版。

## 核心算法规程

1. **双重契约切批同量纲**：禁止单按项数或单按字符切批。切批规划器必须同时以项数与字符总量为边界把关，并在单项自身超限时让其独占一批，杜绝 413 拒绝。
2. **富文本基于真实元属性而非索引奇偶**：段落结构拆解时显式传递 `isInline`，槽位拓扑对齐采用 `tokens.length + 1` 规则，坚决杜绝基于偶数主干/奇数行内的脆弱推断。
3. **ShadowRoot 继承自 DocumentFragment**：容器守卫与 Mock 必须对齐 W3C 标准（`nodeType === 11`），禁止假设 ShadowRoot 继承自 Element。

## 测试

```sh
pnpm run test:task
```

底层由 Node 原生测试运行器执行 `extension/src/shared/*.test.ts`。

## 许可证

[MIT](../../../LICENSE)
