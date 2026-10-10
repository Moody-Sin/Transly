# 子项目 1：核心算法与整页体验加固实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 升级术语库为 Aho-Corasick 高效单趟替换算法消除级联替换缺陷，并在浏览器插件端实现视口优先（Viewport-First）调度与行内标签富文本段落保真翻译。

**Architecture:**
1. Rust Core：在 `crates/transly-core` 引入 `aho-corasick`，替换现有循环正则匹配为单趟自动机扫描，维护大小写敏感与边界检查。
2. Extension：重构 `extension/src/content/index.ts` 与 `shared` 模块，新增视口感知批次划分机制，新增行内元素占位保护与译文复原算法。

**Tech Stack:** Rust (aho-corasick, cargo test), TypeScript (Vite, DOM TreeWalker, Node.js test runner).

**Spec:** `docs/superpowers/specs/2026-10-10-subproject-1-core-algorithms-and-experience-design.md`

## Global Constraints
- `cargo clippy --workspace --all-targets -- -D warnings` 与 `cargo test --workspace` 必须保持全绿。
- `pnpm run test:task` 与 `pnpm run lint` 必须无警告全绿。
- 遵循 `AGENTS.md`：保持分层边界（transly-core 纯 Rust 零平台依赖，extension 纯瘦客户端零存储 Key）。

## Review Focus
- 术语库包含特殊符号（如 `$100`、`C++`、`\b`、括号）时必须纯字面量匹配替换，不产生正则语法崩溃。
- 长词与短词前缀重合时（`Deep Learning` vs `Deep`），长词必须确定性优先命中。
- 级联替换防卫：术语 A 替换为 B 之后，B 中的文字严禁被术语 C 再次二次替换。
- 行内标签占位：当翻译引擎未保留占位符时，页面必须优雅降级为原位替换，不得产生空节点或页面报错。
- 视口内节点集合为空时（如下滑到底部空白），切批规划必须平滑处理，不能丢弃离屏节点。

---

### Task 1: 术语库引入 Aho-Corasick 单趟扫描与边界识别

**Files:**
- Modify: `crates/transly-core/Cargo.toml`
- Modify: `crates/transly-core/src/glossary.rs`
- Test: `crates/transly-core/src/glossary.rs`

**Interfaces:**
- Consumes: `Glossary::insert`, `Glossary::insert_with_options`, `Glossary::apply`
- Produces: `Glossary::apply(&self, text: &str) -> String` (单趟非重叠扫描，O(L) 时间复杂度)

- [ ] **Step 1: 编写失败测试：验证级联防卫与复杂长词优先**
在 `crates/transly-core/src/glossary.rs` 增加测试 `test_glossary_cascade_prevention` 和 `test_glossary_aho_corasick_longest_match`。

- [ ] **Step 2: 运行测试验证失败**
Run: `cargo test -p transly-core -- test_glossary_cascade_prevention`

- [ ] **Step 3: 引入 `aho-corasick = "1.1"` 并重构 `Glossary` 实现**
更新 `crates/transly-core/Cargo.toml`，重构 `Glossary` 内部使用 `AhoCorasick` 构建单趟状态机与后验边界判定。

- [ ] **Step 4: 运行测试验证通过**
Run: `cargo test -p transly-core`
Expected: 63+ passed, 0 failed.

- [ ] **Step 5: 提交代码**
```bash
git add crates/transly-core/Cargo.toml crates/transly-core/src/glossary.rs
git commit -m "feat(core): upgrade glossary replacement to aho-corasick single-pass scan"
```

---

### Task 2: 浏览器扩展 Viewport-First 首屏优先切批划分

**Files:**
- Modify: `extension/src/content/index.ts`
- Modify: `extension/src/shared/batchPlan.ts`
- Test: `extension/src/shared/batchPlan.test.ts`

**Interfaces:**
- Consumes: `isNodeInViewport(node: Node): boolean`
- Produces: `planViewportBatches(entries: TextNodeEntry[]): { inViewport: TextNodeEntry[], offscreen: TextNodeEntry[] }`

- [ ] **Step 1: 编写视口批次划分单测**
在 `extension/src/shared/batchPlan.test.ts` 补充视口批次优先级排序与稳定性测试。

- [ ] **Step 2: 运行测试验证失败**
Run: `pnpm run test:task`

- [ ] **Step 3: 实现视口感知收集与优先队列调度**
在 `extension/src/shared/batchPlan.ts` 与 `extension/src/content/index.ts` 中接入 `isNodeInViewport`，优先执行首屏批次翻译。

- [ ] **Step 4: 运行测试验证通过**
Run: `pnpm run test:task`
Expected: 全部测试通过。

- [ ] **Step 5: 提交代码**
```bash
git add extension/src/shared/batchPlan.ts extension/src/shared/batchPlan.test.ts extension/src/content/index.ts
git commit -m "feat(extension): implement viewport-first priority batch scheduling"
```

---

### Task 3: 浏览器扩展行内标签富文本段落保真翻译与占位复原

**Files:**
- Create: `extension/src/shared/richText.ts`
- Create: `extension/src/shared/richText.test.ts`
- Modify: `extension/src/content/index.ts`

**Interfaces:**
- Consumes: `isLeafRichTextContainer(el: Element): boolean`
- Produces: `stashRichTextContainer(container: Element): { text: string, slots: Map<string, Text> }`
- Produces: `restoreRichTextContainer(translatedText: string, slots: Map<string, Text>): boolean`

- [ ] **Step 1: 编写富文本占位提取与复原单元测试**
在 `extension/src/shared/richText.test.ts` 测试带 `<a>`、`<code>`、`<strong>` 的段落提取、占位符生成与译文写回拆分。

- [ ] **Step 2: 运行测试验证失败**
Run: `pnpm run test:task`

- [ ] **Step 3: 实现 `richText.ts` 并在 content script 中接入**
在 `extension/src/shared/richText.ts` 实现段落富文本占位与回填逻辑，异常时安全回退至逐 Text 节点替换。

- [ ] **Step 4: 运行全量测试与构建校验**
Run: `pnpm run test:task && pnpm --filter transly-extension run build && pnpm run lint`
Expected: 全部通过且构建无报错。

- [ ] **Step 5: 提交代码**
```bash
git add extension/src/shared/richText.ts extension/src/shared/richText.test.ts extension/src/content/index.ts
git commit -m "feat(extension): preserve inline tags and syntax order in rich-text blocks"
```

---

### Task 4: 全流程回归验证与经验沉淀

**Files:**
- Modify: `Lessons Learned.md`
- Test: 全工作区校验门禁

- [ ] **Step 1: 运行全套 CI 本地检验**
Run:
- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `pnpm run lint`
- `pnpm run format:check`
- `pnpm run test:task`
- `pnpm run build`
- `pnpm --filter transly-extension run build`

- [ ] **Step 2: 更新经验教训与变更日志**
在 `Lessons Learned.md` 记录 Aho-Corasick 单趟词边界处理与 DOM 占位回填的实践总结，并在 `CHANGELOG.md` 记录特性增强。

- [ ] **Step 3: 最终提交**
```bash
git add Lessons Learned.md CHANGELOG.md
git commit -m "docs: document lessons learned from aho-corasick and rich-text translation"
```
