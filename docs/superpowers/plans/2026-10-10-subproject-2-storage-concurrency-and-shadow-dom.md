# 子项目 2：存储并发读写分离与现代网页 Shadow DOM 穿透实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现 `TranslationCache` 读写连接分离提升多批次并发性能，并实现浏览器扩展穿透开放模式 Shadow DOM 的正文收集。

**Architecture:**
1. Rust Core：`TranslationCache` 内部拆分为 `writer: Mutex<Connection>` 和 `reader: Mutex<Connection>`，在 WAL 模式下读写完全解耦并发，互不阻塞。
2. Extension：`collectTextNodes` 在遇到具备开放模式 `shadowRoot` 的元素时，递归遍历其内部文本节点，纳入统一切批调度。

**Tech Stack:** Rust (rusqlite, WAL), TypeScript (Web Components ShadowRoot).

**Spec:** `docs/superpowers/specs/2026-10-10-subproject-2-storage-concurrency-and-shadow-dom-design.md`

## Global Constraints
- 遵守简体中文约定式提交：`feat(...)`, `docs(...)`。
- CI 全绿：`cargo fmt`, `cargo clippy`, `cargo test`, `pnpm run lint`, `pnpm run format:check`, `pnpm run test:task`, `pnpm run build`。

---

### Task 1: TranslationCache 读写分离架构改造 (TDD)

**Files:**
- Modify: `crates/transly-core/src/cache.rs`
- Test: `crates/transly-core/src/cache.rs`

- [ ] **Step 1: 编写并发读写解耦单测**
编写并发测试：在持有写事务时，读连接能够并发成功读取已有条目，无锁等待超时。

- [ ] **Step 2: 改造 `TranslationCache` 双连接架构**
拆分 `writer` 与 `reader`，区分写入路径与查询路径。

- [ ] **Step 3: 运行测试验证全绿**
Run: `cargo test -p transly-core -- cache`

- [ ] **Step 4: 提交代码**
```bash
git add crates/transly-core/src/cache.rs
git commit -m "feat(core): 实现翻译缓存 sqlite 读写连接分离并发解耦"
```

---

### Task 2: 浏览器扩展开放 Shadow DOM 穿透遍历 (TDD)

**Files:**
- Modify: `extension/src/content/index.ts`
- Test: `src/task/e2e.test.ts` 或对应 task 单测

- [ ] **Step 1: 编写 Shadow DOM 收集测试用例**
- [ ] **Step 2: 在 `collectTextNodes` 中支持递归探测 `element.shadowRoot`**
- [ ] **Step 3: 运行扩展单测与桌面端测试**
- [ ] **Step 4: 提交代码**
```bash
git add extension/src/content/index.ts src/task/e2e.test.ts
git commit -m "feat(extension): 支持开放模式 shadow dom 内部文本穿透收集"
```

---

### Task 3: 全量构建与端到端回归验证

- [ ] **Step 1: 全套构建门禁检验**
- [ ] **Step 2: 更新变更日志与总结**
- [ ] **Step 3: 提交文档**
```bash
git add CHANGELOG.md docs/superpowers/
git commit -m "docs: 记录读写分离与 shadow dom 穿透实施计划与日志"
```
