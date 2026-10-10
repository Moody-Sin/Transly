# 全面评审调研报告：优化与增强项

> 评审日期：2026-10-11 · 基线提交：`22cca9b`（分支 `dev`）
> 方法：5 路并行静态评审（core / 平台层 / 前端 / 插件 / 基建）+ 主控亲自执行全部 CI 命令取基线 + 对每一条 P0 论断回源码逐行复核。core 路的缺陷另由独立验证 crate 实跑复现。
> **凡标注「✔已复核」者，均由主控打开原文件确认过行号与代码，非二手转述。**
>
> **规模**：11 条 P0、22 条 P1、约 30 条 P2/P3。

---

## 一、工程基线（实测执行，非纸面核对）

| 检查项 | 命令 | 结果 |
|---|---|---|
| 格式 | `cargo fmt --all --check` | ✅ |
| 静态分析 | `cargo clippy --workspace --all-targets -D warnings` | ✅ |
| Rust 测试 | `cargo test --workspace` | ✅ 113 项 |
| ESLint | `pnpm run lint` | ✅ No issues found |
| 类型 | `tsc --noEmit` | ✅ |
| 前端/插件测试 | `pnpm run test:task` | ✅ 30 项 |
| 构建 | `pnpm run build` / `build:extension` | ✅ |
| 版本号同步 | package.json / tauri.conf / 两个 Cargo.toml | ✅ 均为 0.2.1 |
| 待办债务 | TODO / FIXME / `unimplemented!` | ✅ 0 处 |
| 仓库卫生 | `git ls-files` 查产物/密钥 | ✅ 干净，`pnpm audit` 无漏洞 |

**结论：工程底子远比文档吹得好。** CI 硬门禁（覆盖率 70%、IPC 类型同步校验、gitleaks、双平台 Rust 测试）都是真实实现。所有问题都藏在功能深层逻辑里，而非工程配置。

---

## 二、P0 — 必须立即处理（11 条）

### P0-1 本地 HTTP 服务无 `Host` 头校验 → DNS Rebinding 可窃取 Token ✔已复核

**这是全项目唯一可远程利用的漏洞。**

`src-tauri/src/server/mod.rs` 全文 587 行**没有任何** `Host` / `RequestHost` / `Authority` 校验（主控 grep 全文确认，唯一命中的 `host` 是第 136 行的 origin 解析）。唯一的跨源防护是 `CorsLayer`（第 208-223 行）。

而 CORS **不是服务端访问控制**。攻击链：

1. 攻击者注册 `evil.com`，A 记录 TTL=0 指向 `127.0.0.1`（DNS Rebinding）。
2. rebinding 后 `http://evil.com:39281` 与恶意页面**同源**，浏览器不发预检、不校验 CORS。
3. 直接 `POST /pair/request`（该路由在第 227 行，**不在** `protected` 内，零鉴权）→ 桌面端弹确认框。
4. 用户点「允许」→ 攻击者轮询 `/pair/poll` → **拿到 Bearer Token**。
5. 持 Token 可任意调用 `/translate`，消耗用户付费的 DeepL/OpenAI 配额。

第 109-129 行的 `is_allowed_origin` 写得再严也拦不住——请求根本不会带被拒绝的 Origin。

**修复**：加一层 `from_fn` 中间件，仅接受 `Host` ∈ {`127.0.0.1:port`, `localhost:port`, `[::1]:port`}，其余 421。成本约 10 行。

---

### P0-2 隐私政策的陈述与实现直接矛盾（法律/合规风险）✔已复核

`PRIVACY_POLICY.md:19` 承诺：

> 扩展**不监听鼠标选区、不常驻注入脚本**、不在后台读取页面内容；气泡脚本**仅在点击菜单的那一刻注入**。

实际（主控逐行打开确认）：

- `extension/src/selection/index.ts:460` — `document.addEventListener('mouseup', ...)`
- `extension/src/selection/index.ts:546` — `document.addEventListener('selectionchange', ...)`
- `extension/src/background/index.ts:78-89` `ensureSelectionScriptInjected` 只注入不移除，故上述监听在该页整个生命周期常驻。

更讽刺的是 `selection/index.ts:5-6` 的**自己的注释**写着「不做常驻注入、**不监听 mouseup**，与 PRIVACY_POLICY 一致」——注释与同文件第 460 行代码互相矛盾。

后果：用户只要在任意页面用过一次划词翻译，此后每次选词都被捕获。这是本次审计中唯一面向外部、具法律效力的失实陈述，Chrome Web Store 审核会看隐私政策。

**修复**：二选一，不要两者都留 —— 要么改政策如实描述，要么移除 460/546 的常驻监听。

---

### P0-3 术语库对非 ASCII 词条完全放弃单词边界 → 中文术语大面积误伤 ✔已复核

`crates/transly-core/src/glossary.rs:39-49`（主控已打开确认）：

```rust
let starts_with_ascii = src.chars().next().map(|c| c.is_ascii_alphanumeric() || c == '_').unwrap_or(false);
let ends_with_ascii   = src.chars().next_back().map(|c| c.is_ascii_alphanumeric() || c == '_').unwrap_or(false);
let word_boundary = starts_with_ascii && ends_with_ascii;   // ← 合取
```

**全部中日文词条**首尾均为非 ASCII 汉字 → 两个条件皆 false → 边界检查整体关闭。子代理构建独立验证 crate 实测复现：

| 输入 | 实测结果 | 期望 |
|---|---|---|
| 术语 `机器 → 装置`，译「机器学习和机器视觉」 | 「装置学习和装置视觉」 | 「装置学习和机器视觉」 |
| 术语 `café → 咖啡`，译「a caféteria menu」 | 「a 咖啡teria menu」 | 「a caféteria menu」 |

对中文用户这是**最高频场景**——录中文术语是术语库的主要用途。现有测试 `glossary.rs:249` 只覆盖纯 ASCII 词条 `"cat"`，整个非 ASCII 分支零覆盖，这正是 8 个单测全绿而缺陷存在的原因。

**修复**：非 ASCII 边界改用 `char::is_alphanumeric()`（已覆盖 CJK 与拉丁附加），而非直接置 false。

---

### P0-4 划词原地替换跨阶段释放全局锁 → 覆盖并销毁用户新复制的内容 ✔已复核

`src-tauri/src/commands/mod.rs:556-708`，主控已打开确认时序：

| 行 | 动作 |
|---|---|
| 568 | 拿 `CLIPBOARD_SIMULATION_MUTEX` |
| 569 | `sel_flag = true` |
| 581-583 | 模拟 Ctrl+C |
| **599** | **`sel_flag = false`（剪贴板里仍是原文）** |
| **600-602** | **注释明写「离开本作用域自动释放 _guard 锁」** |
| 612-677 | **无锁**执行网络翻译（可达数秒） |
| 681-708 | 重新加锁 → 写译文 → Ctrl+V → sleep(350ms) → **无条件写回 `old_clipboard`** |

**故障场景**：用户按 `Alt+Shift+R`，翻译耗时 2 秒。这 2 秒内用户在另一应用复制了「world」。阶段三开始时：(a) `write_text(final_text)` 覆盖「world」；(b) Ctrl+V 发给**当前焦点窗口**（可能是另一个应用），译文被粘进无关文档；(c) 第 701-705 行写回 `old_clipboard`（「hello」），**用户刚复制的「world」被彻底销毁**。

**修复**：写回 `old_clipboard` 前先 `read_text()` 比对，若已被外部改写则跳过恢复。

---

### P0-5 划词抑制窗口只覆盖 Ctrl+C → 同一次划词被翻译两次 ✔已复核

同一函数，第 599 行 `sel_flag.store(false)` 早于第 684 行的译文写入。第 566 行注释宣称抑制窗口可「防止同一段文本翻译两次、浮窗弹两次」——**实际未覆盖后续翻译耗时**。

`main.rs:283` 的去重也救不了：监听器自身的 `last_clipboard` 尚未记录这段选区文本，会把它当新内容放行并广播一次浮窗翻译。更糟的是这次误触发会把 `last_clipboard` 更新成该选区文本，导致用户**随后手动复制同一段文本时不再触发**。

对比：`trigger_selection_translation`（第 748-826 行）的 `sel_flag` 一直保持到第 824 行恢复剪贴板之后，是正确的。两条链路标准不一致。

---

### P0-6 取消先于注册：已取消的任务仍被投递并真实计费 ✔已复核

`src/task/useTaskRunner.ts:81-163`，主控已打开确认：

```ts
const gen = ++genRef.current                              // :83
const detectRes = await commands.detectTextLanguage(text) // :96  ← 一次 IPC 往返
// …中间无任何代际复查…
const reqPromise = commands.translateStream(gen, ...)     // :123-128 直接投递
if (applyIfCurrent(gen, genRef.current, true) !== true)   // :132 只挡响应
```

代际闸门只校验**响应回流**，从不校验**请求投递**。`translateStream` 尚未发出时后端注册表里还没有这个 `task_id`，所以 `cancelStreamTask` 是 no-op。

**故障场景**：t=500ms 防抖触发 `execute`（gen=2）进入 detect 往返 → t=550ms 用户继续输入 → `cancelStreamTask(2)` 无效 → gen 递增 → t=600ms detect 返回，**不复查代际**，直接发出 `translateStream(2, ...)`，后端完整跑一次 LLM → 响应被丢弃 UI 无感，但 **API 已计费，且这条用户从未看到的译文被写进了缓存和历史**（`mod.rs:259-266`）。

**修复**：在 `await detectTextLanguage` 之后、构造 `reqPromise` 之前补 `if (gen !== genRef.current) return`。

---

### P0-7 `run()` 对空串提前返回 → 防抖不清、任务不取消 ✔已复核

`useTaskRunner.ts:167`：

```ts
if (!text.trim()) return        // ← 在 clearDebounce() 之前直接返回
clearDebounce()                  // :169
commands.cancelStreamTask(oldTaskId) // :172
```

`App.tsx:878-889` 的 `onChange` 对**每一次按键**调用 `run()`，包括删除到空。**故障场景**：输入「hello world」→ 500ms 防抖待触发或任务在途 → 全选 + Delete → `run('')` 直接 return → 上一次按键留下的防抖定时器**不会被清除**，仍用「hello world」发起翻译；在途任务也不取消 → **输入框已清空，右侧却填上了译文**。

`handleClearSource`（`App.tsx:568-573`）显式调了 `runner.cancel()`，所以 ✕ 按钮路径是对的，**唯独键盘删除路径漏了**，两条清空路径行为不一致。

---

### P0-8 `task_id` 每窗口独立计数，取消注册表却是全局的 → 跨窗口误杀 ✔已复核

`src-tauri/src/commands/mod.rs:282-285`：

```rust
pub struct StreamTaskRegistry(
    pub Arc<std::sync::Mutex<std::collections::HashMap<u32, oneshot::Sender<()>>>>,
);
```

单张全局 `HashMap<u32, Sender>`，**不含窗口维度**；`cancel_stream_task(task_id: u32)` 也不带窗口。值得注意的是 `translate_stream` 第 155 行已经接收了 `window: tauri::Window` 参数，但注册表未按窗口分区。

前端每个窗口各自 `genRef = useRef(0)`（`useTaskRunner.ts:59`）。**故障场景**：主窗口正在跑 LLM 流式任务（`task_id=2`）时，剪贴板监听让浮窗完成第 1 次翻译；浮窗第 2 次划词取到 `oldTaskId=2` → 杀掉主窗口正在跑的流 → 主窗口报「流式任务已被前端主动取消」，用户看到的是毫无线索的「内部错误」。

**修复**：`task_id` 改为 `(window_label, task_id)` 复合键。

---

### P0-9 `FallbackEngine::translate_batch_tolerant` 数组越界 panic（core，已实证）

`crates/transly-core/src/engine/fallback.rs:126,134`：

```rust
for (idx, res) in primary_results.into_iter().enumerate() {
    match self.fallback.translate(&requests[idx]).await {   // :134
```

`idx` 枚举自 `primary_results`，索引的却是另一条切片 `requests`，两者长度**无任何校验关系**。子代理构建独立验证 crate 实跑复现：

```
请求 3 项、primary 返回 6 项 →
panicked at fallback.rs:134: index out of bounds: the len is 3 but the index is 3
```

生产可达：`engine_pool.rs:105,142` 正是把真实 DeepL/OpenAI 引擎包进 FallbackEngine，而 DeepL 的 `translate_batch`（`deepl.rs:239-246`）跨 chunk 直接 `for t in data.translations { all_responses.push(..) }`，不对每 chunk 返回条数做校验。触发概率低，但一旦 LLM 网关异常返回超长数组即在 Tauri 命令上下文中 panic 掉 tokio worker。

**同源缺陷**：请求 5 项、引擎返回 1 项时，`server/mod.rs:469` 因 `results.len() != missing_requests.len()` 直接返回 500，**已成功项一并丢弃**——恰好抵消了 CHANGELOG 里「批量翻译引擎逐项容错隔离」想解决的问题。

---

### P0-10 占位符还原可被内容自我污染 → 用户原文被静默改写（core，已实证）

`crates/transly-core/src/preserve.rs:94-97`：

```rust
for (idx, original) in stash.iter().enumerate() {
    let placeholder = format!("⟦TRANSLY_CODE_{idx}⟧");
    result = result.replace(&placeholder, original);
}
```

占位符是可预测的纯文本，原文未经任何转义或加盐。子代理实测复现：

| 输入 | 实测还原 | 期望 |
|---|---|---|
| ``a `⟦TRANSLY_CODE_1⟧` b `keepme` `` | ``a ``keepme`` b `keepme``` | ``a `⟦TRANSLY_CODE_1⟧` b `keepme``` |

还原中 `⟦TRANSLY_CODE_0⟧` 被展开成 `⟦TRANSLY_CODE_1⟧`，随即被下一轮当成真占位符替换。**技术文档讲解占位符机制本身即可触发**，用户原始内容被静默改写且无任何提示。

---

### P0-11 `FallbackEngine::translate_stream` 降级导致 UI 文本重复拼接（core，已实证）

`crates/transly-core/src/engine/fallback.rs:43-68`：primary 与 fallback **共享同一个 `on_chunk` 回调**。primary 已推送部分 chunk 后失败（LLM 流式最常见的断连/超时），fallback 译文会追加在残缺输出之后。

子代理实测：primary 推送 `"PARTIAL-A "`、`"PARTIAL-B"` 后报错，fallback 正常返回——

```
最终 response.text = "完整译文"
UI 实际渲染         = "PARTIAL-A PARTIAL-B完整译文"
```

返回的 text 与 UI 渲染不一致：用户看乱码，复制与落库的却是干净译文。修复方向是降级前清空已推送内容，或为 fallback 换独立回调并向上层回报「已重试」信号。

---

## 三、P1 — 本迭代修复

| # | 问题 | 位置 | 说明 |
|---|---|---|---|
| P1-1 | **CORS 白名单只验格式不验身份** | `server/mod.rs:109-129` | `chrome-extension://` + 32 个 a-p 字符即放行，任意已安装扩展都能过。应钉死具体扩展 ID（Chrome ID 是构建期常量、公开可查） |
| P1-2 | **还原原文不穿透 Shadow DOM** | `content/index.ts:91-121` | 翻译侧（`:372-381`）递归穿透 open shadow root，还原侧的 TreeWalker 不穿透。Web Components 页面上**还原永久失效**，只能刷新页面 |
| P1-3 | **「为此站点自动翻译」永不触发** | `background/index.ts:459-460` | manifest 只申请 `activeTab`，未申请 `tabs`。用户正常打开/刷新站点时 `tab.url` 恒为 `undefined`，整个 `if` 块短路 |
| P1-4 | **`set_if_current` 在 async 线程跑阻塞 SQLite** | `commands/mod.rs:116,267,657` + `server/mod.rs:503` | 该函数是同步的，抢全局 `sync_gate` 后同步写库，连接还设了 `busy_timeout=5000` → 单次可占用一个 tokio worker 5 秒。与同文件 `:485` 已正确 `spawn_blocking` 的写法自相矛盾 |
| P1-5 | **术语库 CSV 往返即损坏** | `App.tsx:1462-1508` | 导出加引号，导入按 `,` 裸切。术语 `foo,bar` → 往返后变成 `src='"foo'`、`tgt='bar'`。同仓库 `HistoryPanel.tsx:102-109` 有正确的 RFC4180 实现 |
| P1-6 | **空 Token 时鉴权 fail-open** | `server/mod.rs:146-160` | `constant_time_eq(b"", b"")` 为 true，当期望 Token 为空串时 `Authorization: Bearer ` 即通过。当前靠 `main.rs:70-76` 恰好规避，属偶然正确 |
| P1-7 | **快捷键注册失败被 `let _ =` 丢弃** | `main.rs:224,244` | `Alt+Shift+D` 极易被占用，失败后无任何 UI 提示，用户表现为「划词莫名其妙不工作」 |
| P1-8 | **storage 迁移按后缀独立判断** | `storage.rs:46-54` | 三个后缀各自判断 `!to.exists()`。新目录已有 `cache.db` 但无 `-wal` 时，旧 WAL 会被搬进新库 → SQLite WAL 不校验盐值 → 静默数据损坏 |
| P1-9 | **流式取消把「sender 被 drop」当成「用户取消」** | `commands/mod.rs:241-261` | `_ = cancel_rx =>` 匹配**任何**完成含 `Err`。`task_id` 重复时 `insert` 覆盖旧 sender，旧 sender 析构即让旧任务以 `Err` 完成 |
| P1-10 | **`clear_cache` 未递增 generation** | `commands/mod.rs:393-404` | 用户点「清空历史」，在途请求持有旧 `start_gen` 仍会把记录写回 |
| P1-11 | **富文本段落模块是死代码** ✔已复核 | `extension/src/shared/richText.ts` | 主控 grep 全 `extension/src` 确认：`processRichTextParagraph` / `isLeafRichTextContainer` 除定义处与 import 外**零调用点**。模块头注释声称的「段落占位符保护」从未生效，其测试测的是生产路径不存在的函数 |
| P1-12 | **lefthook 的 Rust 门禁被架空** ✔已复核 | `lefthook.yml:5,7,9` | 三条 cargo 命令全部以 `\|\| true` 结尾，退出码被无条件吞掉，Rust 本地门禁 100% 不可能失败。讽刺的是 CHANGELOG 自己写着「lefthook 提交门禁真正生效」 |
| P1-13 | **CHANGELOG 0.2.1 段整块逐字重复** ✔已复核 | `CHANGELOG.md:106-120` | 主控逐行确认 6 条连续条目各出现两次；同一版本段内 `### 修复` / `### 变更` / `### 新增` 分类标题也各重复一次 → 0.2.1 实际发布说明不可读 |
| P1-14 | **HistoryPanel 搜索乱序覆盖** | `HistoryPanel.tsx:27-56` | `load()` 无代际闸门，`clearTimeout` 挡不住在途 IPC。快速输入时后发先至会让搜索框显示「cat」而列表是「ca」的结果 |
| P1-15 | **多引擎对比模式可卡死在「翻译中」** | `App.tsx:825,958-964` | 键盘清空不清 loading + 切回单引擎时 `clearTimeout` 杀掉待触发的清空回调 → 按钮永久 disabled |
| P1-16 | **OCR 无条件覆盖用户输入** | `App.tsx:591-595` | 写了半篇长文后误点「识别剪贴板图片」，文本被静默销毁且无撤销 |

**transly-core 层（子代理以独立验证 crate 实跑复现，非静态推断）**

| # | 问题 | 位置 | 说明 |
|---|---|---|---|
| P1-17 | **智能对转覆盖用户显式选择的目标语言** ✔已复核 | `detector.rs:154-162` | 目标为 `ja`/`fr`/`de` 等「其他语言」时，若检测到的源语言与目标相同则强制改为 `zh-CN`。用户显式选日语，日文输入被改译成中文。**这是提交 22cca9b「修复多语言对转偏转」的遗漏分支**——那次只处理了 `target=en` 的拉丁语系路径 |
| P1-18 | **`cache.search` 未转义 LIKE 通配符** | `cache.rs:416` | 实测：表内存在 `alpha` 时 `search("al_ha")` 会误命中 `alXha`；`search("100%pct")` 中 `%` 匹配了空格。搜技术文档时 snake_case 标识符极常见，误命中稳定复现。需 `ESCAPE '\'` + 逐字符转义 |
| P1-19 | **未闭合代码围栏吞掉整篇文档** | `preserve.rs:37` | 实测：`"Intro.\n```rust\nfn main(){}\n\nConclusion."` 中 `Conclusion` 从此不参与翻译，永久丢失。AI 生成/流式截断的 Markdown 极易产生未闭合围栏 |
| P1-20 | **引擎客户端超时配置被静默丢弃** | `deepl.rs:38`、`google.rs:15`、`openai.rs:107` | `Client::builder().timeout(15s).build().unwrap_or_default()`——reqwest 源码明确注释默认**无超时**。TLS 初始化失败时退化为完全无超时的默认 Client，比配置值更危险且无日志 |
| P1-21 | **`validate_engine_id` 白名单与实际 ID 空间不一致** | `validation.rs:43` | 白名单只有 `google\|deepl\|openai\|deepseek`，但 `OpenAIEngine` 生成的 ID 是 `format!("openai:{model}")`。实测 `validate_engine_id("deepseek:deepseek-chat")` 返回 false，而 `engine_pool.rs:137` 正是设置这种 ID。目前靠前端只传别名侥幸未炸 |
| P1-22 | **DeepL 未知语言码静默回落中文** | `deepl.rs:76` | `_ => "ZH"`。`vi` 等码会被静默翻译成中文且无任何提示 |

---

## 四、core 层性能问题（P2）

| # | 问题 | 位置 | 实测 |
|---|---|---|---|
| P2-0 | **`Glossary::apply` 每次调用重建 Aho-Corasick 自动机** | `glossary.rs:83-90` | 500 词条单次 apply 平均 **4.72ms**。30 项整页翻译即累计 142ms 纯浪费，且发生在 tokio worker 上。应缓存编译结果 |
| P2-0b | `restore_code_blocks` 每次调用重新编译正则 | `preserve.rs:101` | 应用 `LazyLock`/`OnceLock` 静态化 |
| P2-0c | SSE 行缓冲 `drain` 为 O(n²) | `openai.rs:299-302` | 长流下累计搬移；`from_utf8_lossy` 遇被切断的多字节序列会静默插入 U+FFFD（reqwest chunk 边界不保证落在字符边界） |
| P2-0d | `google.rs` 的 `encoded_len` 低估真实编码长度 | `google.rs:94-96` | 所有 ASCII 一律记 1，但空格要编码成 `%20`（3 字符）。实测 4800 字符实际编码 6400。当前被 `chunk_paragraph` 兜住未造成越界，属风险 |

---

## 五、P2 — 下一迭代（平台层 / 前端 / 插件 / 基建）

**架构与性能**
- `collectTextNodes` 对每个文本节点强制同步布局（`content/index.ts:266-282`）→ 5000 节点页面主线程冻结数百毫秒
- MutationObserver 纯防抖无 max-wait（`content/index.ts:438-475`）→ 高频变更页面上增量翻译**永久饥饿**，且 pending Set 持续强引用已脱离文档的节点
- 浮窗白跑约 10 次 IPC：挂载 effect 无 `isFloatWindow` 短路（`App.tsx:229-298`），结果写进约 20 个永不渲染的 state
- 扩展端无瞬态重试：服务端 429（并发令牌耗尽）/500 直接整批判失败，最多 30 个节点丢失

**正确性**
- 字符统计口径不一致：扩展用 UTF-16 `.length`，Rust 用码点 `.chars().count()`（`batchPlan.ts:40` vs `engine/mod.rs:180`）→ 6000 个 emoji 被前端误判「过长」
- `/verify_token` 探针把「非 401」和「网络异常」一律判为已配对（`background/index.ts:386-395`）→ 桌面端 500/404 时 UI 仍显示「已连接」并解锁翻译按钮
- `saveLlmConfig` 错误被吞（`App.tsx:501-512`），用户看到「密钥已保存」但端点/模型根本没落盘
- `setCacheEnabled` / `setClipboardMonitor` 缺 try/catch（`App.tsx:403-423`），IPC 抛错时回滚逻辑永不执行
- `revokeObjectURL` 紧跟 `click()`（`App.tsx:1471`、`HistoryPanel.tsx:88,124`）→ Chromium 下下载可能被取消
- 剪贴板写入无 `.catch`（`App.tsx:477,577`、`HistoryPanel.tsx:59`）→ 失败仍显示「已复制」

**平台层**
- `main.rs` 有 5 处阻塞 keyring/SQLite 调用未下沉（`:60,86,100,211,216`），`setup` 在主线程执行
- OCR 下采样前无条件全量拷贝原像素（`ocr.rs:79-92`）→ 超大位图可致内存耗尽，缺 `width*height*4` 上限校验
- `storage.rs:12-24` 用 `%APPDATA%`（漫游）而非 `%LOCALAPPDATA%`，翻译历史与术语库会被域策略同步到其他机器
- 引擎原始错误体原样回传 HTTP 客户端（`openai.rs:216-220` + `server/mod.rs:492-496`）→ OpenAI 网关 401 会回显密钥前缀
- 浮窗宽度大于显示器时 `place_float_near_cursor` 把窗口推到屏外（`commands/mod.rs:743`）

**插件**
- `manifest.json:5` 版本号不从 git tag 注入 → Chrome Web Store 拒绝重复版本上传（**发布阻断**）
- Popup 语言下拉硬编码中文语种名、manifest `default_title` 硬编码中文
- `build-firefox.mjs` 的 `fs.cpSync` 不清理过期文件，`gecko.id` 仍是 `{{EMAIL_...}}` 占位符

**基建**
- CI 缺 PROJECT_RULES 承诺的「禁止提交文件检查」（9 项清单里唯一纯空头的一项）
- 全部 `pnpm install` 未加 `--frozen-lockfile`（`:70,49,82`）——项目认真跟踪了 lockfile 却用宽松解析
- macOS 零 CI 覆盖，却是发布目标（`release.yml:22` 出 `macos-arm64`）；0.2.1 首个修复恰是「Linux 专属编译失败」
- `format:check` glob 不含根配置文件与 `scripts/*.mjs`，实测 `tailwind.config.js`、`check-commit-msg.mjs` 不合规却 CI 全绿
- `docs/README.md` 自订「每新增一篇必须登记」，实际 16 篇只登记 1 篇
- 无 `dependabot.yml`；`lucide-react` 落后 3 个大版本、`vite` 落后 2 个
- `enigo 0.3.0`（2019 年）拉入 `windows 0.58` 重复编译

---

## 六、测试策略问题

CLAUDE.md 明确「优先使用端到端测试」，但现状是**测错了地方**：

| 模块 | 行数 | 测试覆盖 |
|---|---|---|
| `task/taskState.ts` | 59 | ✅ 扎实，全仓质量最高 |
| `task/generation.ts` | 4 | ✅ 但只测纯函数 |
| `task/useTaskRunner.ts` | **305** | ❌ **零测试** |
| `App.tsx` | **1638** | ❌ **零测试** |
| `HistoryPanel.tsx` | — | ❌ 零测试 |
| `ShortcutRecorder.normalizeKey` | 52 | ❌ 零测试（纯函数，最易测） |
| 术语库 CSV 往返 | — | ❌ 零测试（纯字符串逻辑） |

**三个 P0 竞态缺陷（P0-6/7/8）全部落在 `useTaskRunner.ts` 这块空白里。** 现有测试验证的是「代际闸门这个纯函数正确」，而 `generation.test.ts:20-28` 用手写的 `let current = 0; current += 1` 模拟计数器，**恰好绕开了真实代码里 `run` +1 / `execute` +1 的双递增**——也就是绕开了 P0-6 的成因。

### core 层的同类规律：缺口集中在「边界组合」而非「函数覆盖」

`transly-core` 73 个测试全绿、clippy 零告警，但 P0-3/9/10/11 四个确证缺陷全部未被拦截。原因高度一致：

| 已有测试覆盖的 | 漏掉的边界 |
|---|---|
| `test_glossary_boundary_check` 只用纯 ASCII 词条 `"cat"` | 整个非 ASCII 分支 → 漏掉 P0-3 |
| `fallback.rs` 只测 `translate` / `translate_stream` | `translate_batch_tolerant` 的下标契约 → 漏掉 P0-9 |
| `test_unclosed_backtick_handled_gracefully` 只覆盖单个反引号 | 未闭合三反引号 → 漏掉 P1-19 |
| `deepl.rs` 只测语言归一化与空 Key | 批量响应长度校验 → 正是 P0-9 的根因 |
| `test_resolve_smart_target_lang_*` 只覆盖 `target=en` | `target=ja/fr/de` 分支 → 漏掉 P1-17 |
| `openai.rs` **零 HTTP 路径测试** | 流式解析、SSE 分帧、`[DONE]`、多字节边界全未覆盖 |

**建议优先补的 6 个回归用例**：非 ASCII 词条边界、fallback 批量下标一致性、未闭合三反引号、占位符自引用、`search` 通配符转义、DeepL 批量响应长度校验。

**建议**：优先为 `useTaskRunner` 建立真实 hook 级端到端测试（驱动真实 Tauri 窗口，快速输入 + 取消，断言不产生孤儿请求、不产生幽灵译文），而不是继续给纯函数加用例。

---

## 七、建议修复顺序

| 批次 | 内容 | 理由 |
|---|---|---|
| **第 0 批** | P1-12 删掉 `lefthook.yml` 的 3 个 `\|\| true` | 3 个字符的改动。门禁失效会让后续所有修复的质量无法保证，必须最先恢复 |
| **第 1 批** | P0-1 Host 校验 + P1-1 钉死扩展 ID | 唯一可远程利用路径，修复成本约 30 行 |
| **第 1 批** | P0-2 隐私政策/代码对齐 | 唯一具法律效力的失实陈述 |
| **第 1 批** | P0-3 术语库非 ASCII 边界 | 中文用户最高频场景，修复约 1 行（`is_ascii_alphanumeric` → `is_alphanumeric`），收益/成本比最高 |
| **第 2 批** | P0-4 / P0-5 剪贴板链路 | 造成**用户数据丢失**，线上必然复现 |
| **第 2 批** | P0-6 / P0-7 / P0-8 前端竞态 | 造成真实计费与跨窗口误杀 |
| **第 3 批** | P0-9 / P0-10 / P0-11 core 缺陷 | panic 风险 + 用户内容静默改写 + UI 与落库不一致 |
| **第 3 批** | P1-2 / P1-3 / P1-11 插件功能失效 | 旗舰功能静默失效 |
| **第 3 批** | P1-13 CHANGELOG 去重 | 0.2.1 发布说明当前不可读 |
| **第 4 批** | 其余 P1 + P2 | 按批次推进 |

**特别提醒两点**：

1. **P1-12（`|| true`）应排在最前**——它是其余所有修复的质量保障，而修复成本只有删 3 个词。
2. **P1-17 是一次不完整修复的遗留**——提交 22cca9b 声称修复「多语言对转偏转」，实际只覆盖了 `target=en` 分支，`target=ja/fr/de` 仍然偏转。这提醒我们：修复「语言路由」这类分支密集的逻辑时，应对**全部目标语言**做表驱动测试，而非只测修复的那一条。

---

## 八、做得好的地方（供维护参考）

- **`task/taskState.ts` 的纯函数状态机 + 逐事件测试**是全仓质量最高的一块，正确范式。
- **代际闸门思路本身是对的**，且已在 `i18n.test.ts` 这类键对称测试中得到同类方法的印证——问题在于它没被用在真正会出错的 hook 上。
- **CORS 未使用 `Any` 通配符**，方法与预检头精确列举，服务严格绑定 `127.0.0.1`，Token 用 `subtle` 做恒定时间比较——安全意识在位，缺的是 Host 校验这一块。
- **`capabilities/default.json` 最小权限评估通过**：`core:event:default` + `allow-show` + `allow-set-focus` 三项与前端实际用到的 API 精确匹配，剪贴板与快捷键插件未授予任何前端权限。
- **扩展译文写入无 XSS**：正文走 `node.nodeValue`，唯一 HTML 解析点是 tooltip 且已转义。
- **`batchPlan.ts` 双上限切批**正确修复了「30 项 × 1667 字整批 413」的量纲错配。
- **构建期断言产物无 ESM 导入**（`scripts/build.mjs:42-57`）把极难排查的回归变成了构建失败。
- **批量上限契约单一来源**严格遵守：core 定义，服务端 `/health` 公开，扩展端运行时读取零硬编码。
