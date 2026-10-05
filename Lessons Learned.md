# 经验教训 (Lessons Learned)

本文件依据全局 `AGENTS.md` 规范建立，用于记录项目中踩过的重大技术硬坑、平台边界限制及架构设计教训，避免后续迭代与维护者重复踩坑。

---

## 1. tauri-specta / Specta 前后端强类型契约与版本锁定

### 现象与问题
- `tauri-specta` 与底层 `specta` 的版本耦合极度敏感。直接使用语义化范围 `^2.0` 会导致拉取到不兼容的破坏性小版本（RC 变动），引发 `collect_commands!` 宏展开报错或 TypeScript 类型生成格式崩塌。
- `specta` 默认会将 Rust `u64` / `i64` 导出为 JavaScript 原生 `BigInt`。在前端通过 Tauri IPC 通信传输并在 React 渲染或 `JSON.stringify` 序列化时，会直接抛出 `TypeError: Do not know how to serialize a BigInt`，导致前端整页崩溃。

### 经验与规程
1. **严格精确锁定 RC 版本**：在 `Cargo.toml` 中显式指定 `specta = "=2.0.0-rc.25"` 与 `tauri-specta = { version = "2.0.0-rc.25" }`，禁止使用无上限的模糊版本。
2. **禁止导出大整数至前端**：对时间戳、字符数、偏移量等字段，在 IPC DTO 中一律转换为 `u32` / `i32`，或直接使用 ISO-8601 字符串格式，彻底消除 `BigInt` 序列化炸裂隐患。
3. **保持命令导出单测对齐**：必须保留 `src/bin/export_bindings.rs` 导出的绑定文件比对机制，CI 中执行 `git diff --exit-code src/bindings.ts` 拦截前后端类型漂移。

---

## 2. Tauri 2 运行时状态（State）与 TypeId 冲突 Panic

### 现象与问题
- 在 Tauri 2 的 `Builder::default().manage(...)` 中，同一个类型的状态只能注册一次。
- 若在不同的模块（如 `setup` 钩子、外部初始化函数）中重复调用 `.manage(Arc::new(AtomicBool::new(false)))`，Tauri 底层会因为在 `TypeMap` 中检测到重复的 `TypeId` 直接触发线程 `panic`，且报错信息往往非常晦涩（指向容器反射内部）。

### 经验与规程
1. **使用专用包装元组结构体 (Newtype Pattern)**：对于基础类型（如 `Arc<AtomicBool>`、`Arc<Mutex<String>>`），严禁直接裸传 `manage`，必须使用带有明确业务含义的强类型包装结构体：
   ```rust
   pub struct ClipboardMonitorEnabled(pub Arc<AtomicBool>);
   pub struct FloatStickyFlag(pub Arc<AtomicBool>);
   ```
2. **统一管理入口**：所有全局共享状态集中在 `main.rs` 的主构建链上显式挂载，不在分散的分支逻辑中随意调用 `manage`。

---

## 3. WebView 异步初始化与事件（Emit）静默丢失

### 现象与问题
- 快捷键划词或剪贴板变化触发浮窗显示时，如果浮窗处于 `visible: false` 状态，Tauri 仅完成底层原生窗口创建，内部的 WebView 页面与前端 JavaScript 尚未完成初次加载与事件监听器挂载。
- 此时若直接通过 `app.emit_to("float", "event", payload)` 广播事件，该事件会在操作系统消息循环中直接被前端“错过”，导致浮窗弹出后永远是一片空白，且没有任何报错。

### 经验与规程
1. **严禁依赖 `sleep(400ms)` 式的脆弱等待**：固定延时无法适应不同机器性能（低配机器 400ms 仍然不够用，高配机器每次白白卡死 400ms）。
2. **“先暂存再拉取”的双重兜底架构 (Take-Pending Pattern)**：
   - 后端在发送广播前，在 `PendingFloatRequest` 共享槽位中暂存最新的请求。
   - 前端浮窗组件在 `useEffect` 挂载就绪后，主动调用一次 `take_pending_float_request` IPC 命令把暂存数据捞回并清空。
   - 配合事件去重（`request_id` 去重），实现 100% 可靠交付且零无效延时。

---

## 4. 异步锁内阻塞 I/O 导致系统死锁与引擎卡顿

### 现象与问题
- 在 `EnginePool` 获取翻译引擎实例时，如果在持有 `RwLock::write().await` 的作用域内直接调用 `secrets::get_secret(key)`，会直接调用底层的操作系统凭据管理器（Windows Credential Manager / macOS Keychain）。
- 平台凭据管理器属于完全同步且耗时可能长达数百毫秒的阻塞 I/O。在异步上下文中持有写锁做阻塞 I/O，不仅违反了 Tauri 项目规则（“Tauri command 内禁止阻塞调用”），还会瞬间阻塞所有并发翻译请求的读锁获取，导致整个桌面端与扩展服务严重假死。

### 经验与规程
1. **读写锁与 I/O 严格分离**：
   - 先以只读锁快查内存缓存。
   - 缓存未命中时，**在无锁状态下**通过 `tokio::task::spawn_blocking` 在独立系统线程池中执行平台凭据读取与引擎初始化。
   - 初始化完成后，再瞬时获取写锁并更新 Map（带双重校验）。
2. 保持 Tokio 异步运行时与平台同步 API 的严格隔离，绝不在锁临界区跨越阻塞调用。

---

## 5. 容灾降级（Fallback）场景下的缓存 Key 污染与永久 Miss

### 现象与问题
- 包装引擎 `FallbackEngine` 将 `DeepLEngine` 作为 primary，将 `GoogleWebEngine` 作为 fallback。此时包装引擎对外暴露的 `id()` 是 `"deepl"`。
- 当 primary 失败自动降级到备用引擎时，翻译成功返回的 `res.actual_engine()` 是 `"google_web"`。
- 若写入缓存使用 `actual_engine`（写入了 `"google_web"` 的 key），而后续查询缓存时使用的是 `active_engine.id()`（查询的是 `"deepl"` 的 key），会导致用户下次翻译完全相同的文本时，缓存永久无法命中！

### 经验与规程
1. **缓存查询策略感知降级链**：在查询缓存时，若主引擎自身具备降级能力且未命中，允许进一步匹配降级引擎的历史缓存记录，并在返回时准确标注 `used_engine`。
2. **写回与查存契约必须对称**：任何包含主备切换的缓存设计，必须在单测中专门针对降级路径覆盖“降级成功 $\to$ 写入缓存 $\to$ 二次调用 $\to$ 缓存命中”的完整往返测试。
