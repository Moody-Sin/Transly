# 技术规范：子项目 2 - 存储并发读写分离与现代网页 Shadow DOM 穿透

- **版本**: 1.0.0
- **状态**: Approved
- **日期**: 2026-10-10
- **涉及子系统**: `crates/transly-core/src/cache.rs` (Rust), `extension/src/content/index.ts` (TypeScript)

---

## 1. 目标与背景

1. **SQLite 并发读写冲突消除**：
   - 当前 `TranslationCache` 采用单个 `conn: Mutex<Connection>`。在整页批量翻译（如 30 个并发或连续切批）发生 `set_batch` 写入事务时，所有高频的 `get` 读操作都会在 Rust 锁层面被强行串行阻塞，未能发挥 SQLite WAL 模式原生并发潜力。
   - 解决方案：改造为**读写分离双连接**结构（`writer: Mutex<Connection>` + `reader: Mutex<Connection>`）。写连接专供 `set` / `set_batch` / `clear` / 结构化维护；读连接专供 `get` / `list_recent` / `search` / `get_all_glossary_terms`。彻底消除读写互斥锁争用。

2. **现代 Web Components 开放 Shadow DOM 穿透翻译**：
   - 许多现代站点（如 Lit, Stencil, Web Components 封装的组件库）将正文包裹在自定义元素的 `#shadow-root (open)` 中。
   - 当前 `TreeWalker` 只扫描宿主 `document.body` 顶层节点，导致自定义组件内的文本被彻底遗漏。
   - 解决方案：在 `collectTextNodes` 中增加对开放模式 `element.shadowRoot` 的探测与递归遍历，确保现代前端组件库内容无缝原地翻译。
