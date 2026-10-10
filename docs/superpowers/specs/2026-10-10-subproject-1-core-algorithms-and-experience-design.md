# 技术规范：子项目 1 - 核心算法与整页体验加固

- **版本**: 1.0.0
- **状态**: Approved
- **日期**: 2026-10-10
- **涉及子系统**: `crates/transly-core` (Rust), `extension` (TypeScript)

---

## 1. 目标与背景

当前 Transly 在长文或整页翻译中存在以下三个体验与性能瓶颈：
1. **术语库循环替换瓶颈**：随着词条量增加，逐词 `Regex::replace_all` 呈现 $\mathcal{O}(N \times L)$ 耗时增长，且存在“后执行的术语可能破坏先前替换好的译文”的级联二次替换缺陷。
2. **整页翻译断句破坏语义**：离散 Text 节点收集把 `Click <a>here</a> to start` 割裂为 3 个独立短语单独翻译，导致在不同语序语言之间回填时语法混乱。
3. **首屏翻译等待感知明显**：整页翻译按文档树顺序从上到下执行，长页面的首屏可视区域可能被排至中后批次，拖慢首屏呈现感知速度。

本规范旨在通过外科手术式设计彻底攻克上述三项问题。

---

## 2. 详细技术方案

### 2.1 术语库 Aho-Corasick 算法单趟扫描 (`crates/transly-core/src/glossary.rs`)

#### 2.1.1 核心机制
- 引入官方 `aho-corasick` crate (v1.1+)。
- 构建统一自动机，配置：
  - `MatchKind::LeftmostLongest`：最长模式优先匹配（例如 `Deep Learning` 必先于 `Deep` 匹配）。
- 建立映射查找表 `Vec<TermMeta>`：
  - 存储目标词汇、是否区分大小写、是否要求单词边界（ASCII 词自动标记边界需求）。
- 在单趟扫描命中模式时，执行边界检测：
  - 若需要词边界校验，检查当前匹配区段的左侧前驱字符与右侧后继字符：若任一侧为 `alphanumeric | '_'`，则判定为词内部，跳过该匹配。
- 采用非重叠单趟重写生成最终文本：彻底解决级联重复替换问题。

#### 2.1.2 质量契约与回退兼容
- 保留现存测试用例（大小写敏感/不敏感、同长字典序、特殊符号如 `$` 纯字面量不展开）。
- 保证无状态与线程安全（`Glossary` 内部实现 `Send + Sync`）。

---

### 2.2 浏览器整页 Viewport-First 优先切批调度 (`extension/src/content/index.ts`)

#### 2.2.1 核心机制
- 在 `collectTextNodes` 或切批规划阶段，通过节点所在元素计算其几何包围盒：
  ```ts
  function isNodeInViewport(node: Node): boolean {
    const el = node.parentElement;
    if (!el) return false;
    const rect = el.getBoundingClientRect();
    return rect.top < window.innerHeight && rect.bottom > 0 && rect.width > 0 && rect.height > 0;
  }
  ```
- 将收集到的候选节点分成两组：
  1. `inViewportEntries`: 视口内节点（高优先级）；
  2. `offscreenEntries`: 视口外节点（后台优先级）。
- 切批与调度执行：
  - 先规划并发出 `inViewportEntries` 批次；
  - 接着发出 `offscreenEntries` 批次；
  - 维持 `currentTaskTotal` 和 `currentTaskCompleted` 全局统一计数，弹窗进度条不出现倒退或断裂。

---

### 2.3 行内标签富文本段落保真翻译 (`extension/src/content/index.ts`, `extension/src/shared/`)

#### 2.3.1 识别叶子富文本容器 (Leaf Rich-Text Container)
- 属于常见段落/标题容器：`P`, `H1`-`H6`, `LI`, `BLOCKQUOTE`, `TD`, `TH`。
- 其子孙节点仅包含内联元素（如 `SPAN`, `A`, `STRONG`, `EM`, `B`, `I`, `CODE` 等）与 `Text` 节点，不包含嵌套的块级容器。
- 文本长度达到短语级别且内部包含多于 1 个文本片段。

#### 2.3.2 占位保护与反向拆解
1. **聚合与占位生成**：
   - 将该容器内的文本序列拼装为单一句子，其中的行内子元素使用占位符 `⟦T_0⟧`、`⟦T_1⟧` 占位；
   - 发送给桌面端后端统一翻译。
2. **结构化还原与写回**：
   - 收到译文后，根据占位符切分译文字符串；
   - 占位符内部对应的译文精准写回至对应的行内元素文本，占位符两侧的译文写回至相邻 Text 节点；
   - 若遇到翻译引擎吞掉占位符的异常情况，无缝回退为传统的单节点原位替换，绝不破坏页面 DOM 结构。

---

## 3. 测试与验证策略

1. **Rust Core 单元测试**：
   - 编写 `glossary.rs` 单测：覆盖超大词库无级联替换、边界检测（中文、英文、符号混排）。
   - 运行 `cargo test --workspace` 与 `cargo clippy --workspace --all-targets -- -D warnings`。
2. **前端与扩展测试**：
   - 在 `src/task/*.test.ts` / `extension/src/shared/*.test.ts` 中补充切批与行内占位还原算法单测。
   - 运行 `pnpm run test:task` 与 `pnpm --filter transly-extension run build` 确保类型与语法健全。
3. **经验库联动**：
   - 若在实施过程中发现新边界，记录至 `Lessons Learned.md`。
