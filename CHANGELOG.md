# 更新日志

本项目的所有重要变更均记录于此文件。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
并且本项目严格遵循 [语义化版本 2.0.0](https://semver.org/lang/zh-CN/)。

## [未发布]

### 修复
- **浮窗划词/剪贴板翻译静默失效 (P0)**：`src-tauri` 此前未声明任何 Tauri 2 capability，运行时 ACL 因此为空。应用自有 command 不经 ACL 校验（这就是主窗口翻译、设置、扩展服务全部正常的原因），但**插件命令**一律被拒：`@tauri-apps/api/event` 的 `listen` 实际调用 `plugin:event|listen`，被运行时以 `event.listen not allowed. Permissions associated with this command: core:event:allow-listen, core:event:default` 拒绝，而 `useTaskRunner` 的 `.catch(() => {})` 吞掉了该异常，导致 `float` 浮窗永远收不到 `float-translate` 事件、只显示占位文案。现新增 `src-tauri/capabilities/default.json`，仅为 `main` / `float` 两个窗口放行最小权限集 `core:event:default`（listen/unlisten），并已在真实运行的应用上用 CDP 探针复验放行。
- **中文长文本翻译必然失败 (P1)**：`GoogleWebEngine` 的长文本切块只按空白边界聚合，无空格的中文段落会被当成一个「单词」——`split_whitespace` 返回的唯一片段直接跳过 800 字符阈值，5,000 字中文段落会生成 URL 编码后约 4.5 万字符的单次 GET 请求（`urlencoding` 将每个非 ASCII 字节膨胀为 `%XX`，中文 1 字 = 9 字符），请求必然失败。现改为纯函数 `chunk_paragraph`：优先空白边界，无空白片段按字符窗口硬切，并以「字符数 800」与「编码后长度 4,000」双重上限取先到者；同时新增 4 项切块单测（中文硬切、西文丢词校验、长 token 硬切、短文本单块）。另修正切块拼接逻辑：仅在西文块之间补空格，避免中文译文句中被插入空格。
- **划词/剪贴板事件全窗口广播导致重复翻译 (P2)**：宿主用 `float_win.emit("float-translate", …)` 投递浮窗请求，而 Tauri 的 `Emitter::emit` 语义是「发给所有窗口」——主窗口因此也执行了一次翻译，并把划词结果写进输出区（输入框为空却出现译文），同一段选择文本被翻译两次。现改为 `emit_to("float", …)` 定向投递，并把前端的事件订阅与 pending 拉取收窄到浮窗（`isFloatWindow` 守卫），主窗口退出划词链路。
- **三个命令在 async 体内直接做同步阻塞 I/O (P2)**：`add_glossary_term`（SQLite 写入）、`delete_glossary_term`（DELETE + 全表回读）、`rotate_server_token`（keyring 写入）此前在 `async fn` 体内直接调用同步 API，违反「命令体内出现 keyring / SQLite 一律 `spawn_blocking`」的既定规程（同文件的 `get_glossary_terms` / `clear_cache` / `set_cache_enabled` 均已下沉）。现全部下沉至阻塞线程池，并保留原有的错误上下文区分。
- **默认引擎不可用时报错无指导 (P2)**：默认引擎 `google_web` 在部分网络（实测本机 `translate.googleapis.com` 超时）不可达，而 UI 仅提示「网络连接失败」。现当错误类别为 `network` 且当前引擎为 Google 时，在主窗口与浮窗额外给出可执行引导（配置 DeepL / DeepSeek 密钥后切换引擎），中英语言包同步补齐。
- **扩展长会话内存持续增长 (P2)**：`currentTaskEntries` 在增量批次路径上使用 `push` 追加，SPA / 无限滚动页面下只增不减，并强引用已脱离文档的 Text 节点（取消翻译时还会遍历它）。现改为用当前批次整体替换（增量批次本身即独立任务），把数组规模收敛到单批次。
- **浮窗固定出现在系统默认位置 (P3)**：划词翻译的预期是译文出现在光标附近。现新增 `place_float_near_cursor`：按光标物理坐标水平居中、垂直置于光标下方，并夹取到当前显示器可视范围内；取不到光标或显示器信息时保持系统默认位置，不阻断浮窗展示。
- **扩展徽标未按标签页隔离 (P3)**：`chrome.action.setBadgeText` 未携带 `tabId` 时会写全局徽标，多标签同时翻译时互相覆盖，与已有的「每 Tab 进度快照」设计矛盾。现统一经 `setBadge(tabId, ...)` 写入。
- **扩展弹出面板文案硬编码 (P3)**：popup 的按钮、状态、进度文案此前全部硬编码中文。现接入 Chrome 原生 i18n：新增 `_locales/zh_CN|en/messages.json`、`manifest.json` 使用 `__MSG_*__`（含 `default_locale`）、`popup.html` 改用 `data-i18n*` 注入、`popup/index.ts` 统一走 `t()`；`scripts/postbuild.mjs` 同步复制 `_locales/` 到产物。
- **门禁锁与缓存兜底 `unwrap()` (P3)**：`CacheController` 三处 `sync_gate.lock().unwrap()` 在持锁期间 panic 后会永久中毒，导致后续全部缓存写入 panic；现改为 `unwrap_or_else(|e| e.into_inner())`（门禁锁不守护任何数据，取回中毒锁是安全的）。`TranslationCache::default_shared` 的最后兜底改为带说明的 `expect`，符合项目「已证明不可能失败并注释说明」规则。
- **dev server 被 `target/` 监视拖崩 (P3)**：Windows 上 Vite 监视 `target/` 下的二进制会抛 `EBUSY`（实测可直接终止 dev server 并导致页面白屏），现于 `vite.config.ts` 忽略 `**/target/**`。
- **同步 Tauri command 阻塞主线程 (P0)**：`is_api_key_configured`、`get_clipboard_monitor`、`get_server_token`、`set_clipboard_monitor`、`set_cache_enabled`、`get_glossary_terms`、`clear_cache` 共 7 个命令此前为同步 `fn`，会按 `tauri::command` 语义在**主线程**执行 keyring 与 SQLite 阻塞 I/O。启动时前端 `useEffect` 会串行触发 5 次 Windows 凭据管理器读取，缓存膨胀时 `DELETE` 全表会让 UI 硬卡。现全部改为 `async fn` 并下沉至 `spawn_blocking`。
- **测试真调外部 API (P0)**：`src-tauri` 服务端集成测试此前使用 `EnginePool::new()`，其默认引擎为 `GoogleWebEngine`，缓存未命中时会真实外呼 `translate.googleapis.com`，且引擎失败仍返回 200 使用例恒通过、零信号。新增 `EnginePool::with_default_engine` 与 `testing::StubEngine`，测试不再触网。
- **测试读写真实系统凭据库 (P0)**：`secrets.rs` 单测此前直接读写开发者与 CI 机器的 Windows Credential Manager / macOS Keychain。现拆出纯函数 `generate_token()` 做无副作用断言，真实往返测试改为需显式设 `TRANSLY_KEYRING_TEST=1` 才执行。
- **扩展增量翻译持续闪烁徽标且进度永不收敛 (P1)**：任务终态（`TRANSLATION_FINISHED` / `_PARTIAL_FAILED` / `_FAILED`）此前在 `translateEntries` 内部广播，而动态增量监听会为每个 pending root 各调一次该函数，无限滚动页面下徽标反复闪烁、`currentTaskTotal` 只增不减导致百分比不收敛。现终态统一上提至任务级入口广播一次，并在上报后清空失败集，避免污染下一轮“部分失败”判定与“重试失败项”按钮可用性。
- **扩展 payload 翻倍 (P2)**：`TRANSLATE_BATCH` 同时发送 `items` 与 `texts`，而服务端只消费 `items`。现仅发送 `items`（服务端保留 `texts` 兼容分支供旧版客户端）。
- **popup 死代码降级分支 (P2)**：`chrome.tabs.query({})` 在未申请 `tabs` 权限时不会返回 `url`，原“查找最近网页标签”回退分支永远命中不了，已移除。
- **Bearer Token 非恒定时间比较 (P2)**：本地 HTTP 服务鉴权由 `==` 字符串比较改为 `subtle::ConstantTimeEq`，消除计时侧信道；抽出 `bearer_matches` 并补齐单元测试。
- **EnginePool 失效归一化不对称 (P2)**：`invalidate` 使用 `to_lowercase()` 而 `get_engine` 使用 `trim().to_lowercase()`，会漏删带空白输入的缓存条目导致旧密钥被继续复用。
- **英文界面残留中文文案 (P1)**：`App.tsx` 中密钥管理、引擎选项、语言选项、状态提示等 20 余处硬编码中文（违反项目规则「禁止硬编码文案」），已全部收敛至 `src/i18n/`。
- **安装包含未替换脚手架占位符 (P1)**：`tauri.conf.json` 的 `bundle.icon` 引用了模板变量替换失败留下的 `icons/128x128@2x.png`，已从配置与仓库中移除。
- **扩展全站常驻注入 (P2)**：移除 `content_scripts` 的 `<all_urls>` 常驻声明，改为用户点击翻译时经 `chrome.scripting.executeScript` **主动注入**，与 `PRIVACY_POLICY.md` 中「仅在主动触发时提取 DOM 文本」的承诺对齐，同时收敛权限面（新增 `scripting`，不再需要全站宿主权限）。

### 变更
- **移除零使用契约与死代码 (P2)**：删除从未被构造的 `Error::UnsupportedLanguage` 与 `Error::Timeout`（连带 `ErrorKind::Timeout`，其重试语义已由 `ErrorKind::Network` 与 `Engine` 覆盖）、无生产消费者的 `TaskState::is_terminal` / `is_active`、无人消费的 `TranslateOutcomeDto::cached` / `detected_source_lang` 与 `EngineCapabilities::supports_streaming`（无任何流式实现）、以及未被调用的 `useTaskRunner.reset`。
- **文档与目录事实对齐 (P2)**：`PROJECT_RULES.md` 目录结构此前描述了不存在的 `src/components`、`src/pages`、`src/hooks`、`src/stores`、`src-tauri/src/platform`、`src-tauri/src/error.rs`，已按实际文件重写；新增 `docs/README.md`、`src/task/README.md`、`extension/src/README.md`，并为 `docs/architecture-state-machines.md` 建立索引（此前全仓库无任何引用）。

## [0.2.0] - 2026-10-06

### 新增
- **剪贴板图片 OCR 与即时翻译闭环 (阶段四)**：主输入框底栏集成「识别剪贴板图片」入口；Windows 平台基于系统 `Windows.Media.Ocr` 原生轻量驱动，支持先验定向与多语言去重轮询，识别文本自动注入输入框并以 `debounceMs: 0` 立即触发翻译；全流程内存零落盘阅后即焚，识别错误与翻译错误严格分层。
- **桌面四类入口统一任务工作流**：主窗口输入框、划词、剪贴板监听与浮窗共用统一任务状态机（Idle/Running/Succeeded/Failed/Cancelled），共享代际隔离的取消语义、结构化错误分类与可重试判定、同一缓存门禁策略。
- **浏览器扩展多语言选择与原地 DOM 增强**：扩展 Popup 增加目标语言选择下拉（中文、英语、日语、韩语、法语、德语），自动持久化用户选择；原地 DOM 替换深度过滤 `contenteditable` 与 Monaco/CodeMirror 编辑器容器，保障页面编辑安全。
- **划词与剪贴板来源区分**：浮窗事件携带来源标识与请求 ID，修复模拟复制导致的重复翻译与重复弹窗；相同剪贴板内容不再重复触发。
- **输入框即时翻译**：主窗口输入框接入 500ms 防抖自动翻译，新输入立即作废上一代在途结果。

### 修复
- **解决 Fallback 降级缓存永久 Miss (P1)**：在核心库提供统一的 `lookup_cache_with_fallback`，支持在主引擎具备容灾降级时自动命中备用引擎历史缓存。
- **GoogleWebEngine 批量翻译并发加速 (P1)**：覆盖 `GoogleWebEngine::translate_batch` 实现分批并发（并发度 4），彻底消除单条 10s 串行等待延时。
- **落实各引擎批量分片与服务端防线 (P2)**：`DeepLEngine::translate_batch` 引入 50 项分片机制，本地 HTTP 服务上限收敛至 50 项，与所有引擎契约完全对齐。
- **浮窗取消态解除粘滞 (P1)**：从 `needsSticky` 判定中移除 `cancelled` 状态，用户主动取消后浮窗失焦可正常智能隐藏，避免永久钉屏。
- **消除引擎连接池持锁阻塞 I/O (P1)**：重构 `EnginePool::get_engine`，使用 `spawn_blocking` 在读写锁外读取系统凭据管理器 `secrets::get_secret`，消除对 Tokio 异步运行时的阻塞与并发假死隐患。
- **未配置密钥时 FallbackEngine 明确报错**：`DeepLEngine` 与 `OpenAIEngine` 在密钥为空时直接触发 `MissingCredentials`，禁止 Fallback 静默由 Google 接管，真实向上暴露结构化 Auth 错误并触发 UI 配置提示。
- **清理死结构与无用依赖 (P2)**：移除零使用的 `detect.rs` 语言检测模块、`cache.rs` trim_to_max_items 死逻辑、`ocr.rs` 冗余错误分支匹配；清理 `Cargo.toml` 中未使用的 `tempfile` 与 `base64` 依赖；优化 `lefthook.yml` 为快速 `tsc --noEmit`。
- **无副作用的扩展连接探测 (P2)**：在本地 HTTP 服务新增 `/verify_token` 轻量 GET 接口，扩展后台探测不再发送带副作用的伪造翻译请求并污染本地缓存。
- **消除划词/剪贴板 400ms 固定无用等待 (P2)**：利用已有 `PendingFloatRequest` 兜底暂存机制直接广播事件，删除 `sleep(400ms)`，划词取词响应提速 400ms。
- **补全 Chrome 商店发布所需隐私政策**：根目录新增标准 `PRIVACY_POLICY.md`，明确阐述零数据收集与本地优先安全承诺。
- **沉淀核心踩坑经验**：根目录建立 `Lessons Learned.md`，完整记录 Specta BigInt、Tauri TypeId 冲突、WebView 异步就绪、异步锁内阻塞 I/O 及 Fallback 缓存对称性五大技术硬坑。
- **纠正 OCR 能力对外表述 (P0)**：README 与更新日志如实标注当前 Windows 原生硬件加速 OCR 能力与 `ocr_clipboard_image` 命令。
- **统一 Node.js 运行环境要求 (P0)**：将 README.md 开发要求与 package.json 对齐为 `>= 22.6`。
- **前端任务重试守卫失效 (P1)**：`useTaskRunner.retry` 此前无条件重新执行，`canRetry` 判定未参与拦截；现仅在失败且错误可重试时执行。
- **质量门禁补齐 (P1)**：前端任务单测 `pnpm run test:task` 纳入 CI 与 lefthook 预提交；CI 与 Release 的 Node 版本由 20 升至 22，满足 `--experimental-strip-types` 运行时要求。
- **接入 Rust 覆盖率门禁**：CI 使用 `cargo llvm-cov` 校验 `transly-core` 行覆盖率不低于 70%，落实项目规则中的覆盖率要求。
- **桌面翻译错误可分类与可重试**：桌面 IPC 由裸字符串错误升级为结构化错误（错误类别 + 是否可重试），UI 按类别展示提示；缺密钥、输入非法等重试无意义的场景不再提供重试入口。
- **浮窗错误态不失焦隐藏**：翻译中与错误态下浮窗保持显示，错误提示与重试按钮不再因失焦自动收起而无法点击。
- **缓存写入错误全链路透传与可观测性 (P2)**：重构 `CacheController::set_if_current` 返回值为 `Result<bool>`，区分“版本有效写入成功”、“请求跨越关闭清空边界拒绝写入”以及“底层存储/SQLite写入异常”三种状态；桌面端与扩展服务捕获异常并输出带上下文的诊断日志，杜绝错误静默丢失。

## [0.1.0] - 2025-05-18

### 新增
- **Transly 桌面端与沉浸式扩展首次正式发布**：基于 Rust + Tauri 2 构建的轻量极速桌面翻译客户端，以及基于 Manifest V3 的原地 DOM 替换网页沉浸式翻译扩展。
- **本地回环安全通信协议**：桌面端自动生成 256 位强随机 Bearer Token 并在 `127.0.0.1:39281` 监听，配合严格 CORS 白名单，保障扩展与桌面端安全联动。
- **Token 动态轮换与即时撤销**：桌面端支持一键轮换 Token 并自动持久化至系统安全凭据管理器，旧 Token 立即 401 失效阻断。
- **持久化本地 SQLite 缓存**：支持毫秒级重复请求极速命中；提供缓存隐私开关，关闭时自动清空历史缓存并保障在途与后续请求零写入。
- **独立全局术语库**：用户自定义术语库与缓存解耦，支持同长度稳定优先级排序与正则字面量安全替换，清空缓存时 100% 完整保留。
- **轻量悬浮翻译与 OCR 引擎抽象**：Alt+Shift+D 划词/复制触发轻量无边框悬浮窗；定义 `OcrEngine` 抽象与 `ocr_image` 命令管道，平台原生识别实现于后续版本交付。

- **轻量划词悬浮浮窗（Mini Floating Window）**：在 Tauri 窗口架构中建立独立的 `float` 迷你无边框置顶气泡小窗（`380x240`，磨砂半透明阴影），按下 `Alt+Shift+D` 划词时仅唤起轻量悬浮窗展示极简译文并支持一键复制，失去焦点（失焦）自动智能隐藏收起，不遮挡阅读视线。
- **系统剪贴板监听自动翻译（可选常驻模式）**：新增剪贴板后台动态检测调度器与 `set_clipboard_monitor` 命令；在设置中心增加常驻开关，开启后用户复制外文时自动弹出迷你悬浮窗展现译文。
- **快捷键触发自动模拟复制选中文本**：利用 `enigo` 在按下 `Alt+Shift+D` 全局快捷键时，自动模拟系统级 `Ctrl+C` (Windows/Linux) 或 `Cmd+C` (macOS) 按键事件将屏幕选区内容拷入剪贴板，随后无缝唤醒主窗口并即时翻译，无需用户预先手动复制。
- **OCR 引擎抽象与命令管道**：在 `transly-core` 中定义 `OcrEngine` trait 与 `SystemOcrEngine` 骨架；在桌面端暴露 `ocr_image` 强类型命令与 Base64 解码管道，为后续多模态识图翻译奠定架构基础（平台原生识别实现尚未交付）。
- **术语库 SQLite 持久化存储与可视化管理**：在 `TranslationCache` 中建立独立 `glossary` 表，桌面端启动时自动回载自定义术语库规则；通过 Specta 暴露出 `get_glossary_terms` 与 `delete_glossary_term` 强类型接口，并在前端设置中心提供术语条目的实时增删与持久化列表。
- **全局快捷键划词取词与即时翻译唤醒**：接入 `tauri-plugin-global-shortcut` 与 `tauri-plugin-clipboard-manager`，用户选中文本按下全局快捷键（`Alt+Shift+D`）即可自动从系统剪贴板读取最新选区文本、唤醒主窗口并即时完成翻译展示。

- **接入 DeepL 与 OpenAI / DeepSeek 高质量翻译引擎**：在 `transly-core` 中新增 `DeepLEngine`（支持 Free 与 Pro 接口、语言代码标准化）与 `OpenAIEngine`（兼容 OpenAI 规范与 DeepSeek 专用预设）；实现动态安全解析与 `FallbackEngine` 容灾回退。
- **桌面端设置中心与多引擎交互面板**：前端支持设置弹窗，可直接管理 DeepL / DeepSeek 密钥安全存入系统原生凭据库（非明文落盘）；支持用户动态切换翻译引擎与添加全局术语库规则。
- **IPC Specta 强类型契约同步**：更新 `commands::translate` 接口并重新导出生成 `bindings.ts`，确保前端引擎选择与后端执行一致。

- **术语库深度接入翻译流水线**：在 `transly-core` 与 `src-tauri` IPC / 本地 HTTP 批量翻译端注入全局 `Glossary` 术语库，实现译文自动匹配与精准纠正；支持桌面命令动态维护术语。
- **主备引擎降级容灾机制**：实现 `FallbackEngine` 包装器，支持在主引擎不可用或网络异常时自动切换备用引擎，提升服务可用性。
- **前端规范与自动化校验闭环**：配置最新 ESLint Flat Config，将 `pnpm run lint` 整合至 `package.json`、`lefthook` 预提交钩子与 GitHub Actions CI 流水线。

- 集成 `tauri-specta` 自动强类型 IPC 绑定生成器，消除手写 invoke 字符串与类型断裂风险。
- 实现浏览器插件 DOM 文本节点 TreeWalker 智能提取与分批并发翻译原地替换。
- 浏览器插件增加 Popup 工具栏交互弹窗与整页一键翻译、还原原文支持。
- 串联 SQLite 持久化缓存至桌面端 IPC 命令与本地 HTTP 服务，支持自动查存与批量异步回写。
- 引入 `GoogleWebEngine` 免费网页翻译实现，接入核心翻译抽象层。
- 本地扩展通信 HTTP 服务新增 Bearer Token 鉴权中间件与 `/translate` 批量接口。
- 浏览器插件后台通信脚本接入 Token 自动携带机制与批量翻译转发。
- 桌面端前端 UI 接入 Tauri 2 核心 IPC 调用通道与错误反馈。
- 初始化项目工程架构、规范文档与规则。
- 确立桌面端（Tauri 2 + React）与浏览器插件（整页仅译文模式）协作技术方案。

### 修复
- **同步门禁锁消除缓存判断与写入竞态 (P1)**：在 `CacheController` 中引入 `sync_gate` 互斥门禁，提供原子 `set_if_current` 与 `disable_and_clear` 保证判断与写入位于同一原子临界区，并在 `server/mod.rs` 补充并发关闭与写回阻断强断言回归测试，彻底关闭清空与写入交错漏洞。
- **清空缓存错误严格冒泡与事务保护 (P2)**：`set_cache_enabled` 在关闭缓存清空历史失败时，强制向前端暴露错误，杜绝静默失败与假成功。
- **防范并发翻译关闭缓存后的写回竞争 (P1)**：在写入缓存的临界点再次读取原子开关 `cache_enabled_flag`，彻底消除长耗时外部引擎翻译返回后重新将数据写回已关闭缓存的竞争窗口。
- **完善扩展连接与 Token 鉴权状态反馈 (P2)**：扩展 Popup 增加深度鉴权校验，准确细分“桌面端未运行”、“未配对 Token”、“Token 已失效，请重新配对”以及“已连接且配对有效”四种明确状态，杜绝无效 Token 误显示已连接。
- **强化清空缓存错误处理与 i18n 完整国际化 (P2/P3)**：`set_cache_enabled` 在清空历史缓存失败时向前端准确反馈并重试；补全 Token 轮换与缓存管理文案的英文国际化字典。
- **隔离全局缓存开关并发竞争并消灭排队内存堆积 (P1)**：在内存状态中维护原子布尔 `cache_enabled_flag`，彻底消除运行时与并行测试中频繁同步读写外部系统凭据库的竞争冲突；并发控制采用 `try_acquire()` 拒绝超额请求（返回 429 Too Many Requests），防止排队堆积与内存膨胀。
- **关闭缓存时同步清空历史磁盘数据 (P2)**：用户在设置中关闭“持久化翻译缓存”时，自动触发清空已有 `translation_cache` 磁盘表（保留术语库），切实保障用户隐私不留存。
- **本地服务全局并发受控限制 (P1)**：在本地 HTTP 服务状态中集成 `tokio::sync::Semaphore`，将同时处理的批量翻译任务严格限制在至多 10 批，防范网页动态 DOM 频繁变更造成的并发过载。
- **杜绝未知引擎静默降级 (P2)**：在 `translate_batch_handler` 与桌面 IPC 中实现 `validate_engine_id` 强校验，对未知或未受支持的引擎代码直接返回 400 错误，严防静默降级导致用户误用。
- **收紧语言代码校验白名单 (P2)**：将语言代码规则收敛为标准受支持白名单枚举校验，阻断潜在非法格式注入。
- **强化语言参数白名单与批量请求防线 (P1)**：在 `transly-core` 建立 `validation` 模块统一语言代码（BCP-47 规范）与字符合法性校验；本地 HTTP 服务增加 2MB 请求体上限、单次批量 50,000 总字符防线，全面拦截长语言代码注入与 DoS。
- **限制 API Key 凭据存储目标引擎枚举 (P2)**：在 `save_api_key` 与 `delete_api_key` 中严格限制目标仅允许受支持的 `deepl`、`openai`、`deepseek`，杜绝写入任意键名。
- **扩展批量翻译严格遵循缓存隐私开关 (P1)**：在本地 HTTP 服务的 `translate_batch_handler` 中集成 `cache_enabled` 共享判定，关闭持久化翻译缓存后彻底停止查询与写入 SQLite 磁盘库，确保桌面端与浏览器扩展两路隐私策略完全一致。

- **启用 Keyring 平台原生凭据后端 (P0)**：在 `Cargo.toml` 中按操作系统细化配置：Windows (`windows-native`)、macOS (`apple-native`)、Linux (`linux-native`)，解决原本回退至临时内存 Mock 导致的密钥无法跨进程/跨重启持久化严重缺陷；强化 `set_secret` 错误传播与凭据往返单元测试。
- **防止术语库特殊字符被正则错误展开 (P1)**：在 `Glossary::apply` 中使用 `regex::NoExpand` 包装目标文本，确保含 `$`、反斜杠的译文（如 `USD → $100`）按纯字面量替换，杜绝捕获组语法误伤。
- **提供本地 SQLite 翻译缓存一键清空支持 (P1)**：在 `transly-core` 与 Tauri IPC 中增加 `clear_all_cache` 命令，并在前端设置中心提供可视化的“清空翻译缓存”操作，提升用户对本地存储的隐私掌控力。
- **消除前端事件死监听 (P2)**：移除前端冗余的 `quick-translate` 事件监听，统一收敛为悬浮窗事件通道 `float-translate`。

- **杜绝固定 Token 降级隐患 (P0)**：当系统原生凭据管理器不可用或故障时，明确拒绝启动本地 HTTP 扩展服务，彻底删除 `transly-default-token` 固定回退硬编码。
- **Release 模式 Clippy 校验与 CI 门禁闭环 (P1)**：将 `specta_typescript::Typescript` 导入约束在 `#[cfg(debug_assertions)]`，在 CI 流水线中补充 `cargo clippy --release` 校验。
- **收紧 CORS 跨域与生产环境限制 (P1)**：限制 CORS Origin 扩展协议并仅在 debug 构建开放本地回环端口调试。
- **前端 TypedError 状态全量防御检查 (P1)**：在删除密钥、添加术语、删除术语等操作中严格校验 `res.status === 'ok'`，失败时准确暴露错误并保留上下文。
- **扩展祖先链忽略规则与深度过滤 (P2)**：在 `shouldTranslateElement` 中使用 `closest` 沿祖先链深度递归检测 `.notranslate`、`data-no-translate` 与忽略标签，防止嵌套节点被误翻。
- **批量翻译数量一致性与输入尺寸统一限制 (P2)**：验证引擎批量返回数量必须与请求完全一致，避免残缺空数据写回；为桌面 IPC 与 OCR 接口补充严格的长度与体积上限校验。

- **修复桌面端启动时 Runtime 缺失崩溃 (P0)**：将本地 HTTP 服务、快捷键监听与剪贴板轮询全部移至 Tauri `setup` 钩子并在 `tauri::async_runtime::spawn` 作用域中安全唤醒。
- **配置 Tauri 默认 Binary 与优化 Bundle ID (P0)**：在 `src-tauri/Cargo.toml` 中明确声明 `default-run = "transly"`，将 `identifier` 规范为 `com.transly.desktop`，消除 Release 构建失败与系统警告。
- **跨平台扩展构建产物补齐 manifest (P0)**：添加跨平台 Node 构建后脚本 `scripts/postbuild.mjs`，在扩展打包时自动拷贝 `manifest.json` 与 `popup.html` 到 `extension/dist/`。
- **杜绝前端接触明文 API 密钥 (P1)**：将 `get_api_key` 命令重构为布尔值判定 `is_api_key_configured`，前端只展示“已配置”状态与重置覆盖入口，密钥绝不进入 WebView 内存与 DOM 树。
- **隔离大模型缓存标识与 Fallback 缓存污染 (P1)**：为 `OpenAIEngine` 动态生成包含模型的专用引擎 ID (`openai:gpt-4o-mini`, `deepseek:deepseek-chat`)，消除不同供应商译文串用；发生 Fallback 降级时仅按实际响应引擎写回缓存。
- **WeakMap 原文追踪与增量监听强化 (P1)**：浏览器插件废除有缺陷的父元素属性方案，改用 `WeakMap<Text, string>` 与 `WeakSet` 在 Text 节点粒度实现无损原地还原；MutationObserver 开启 `characterData: true` 监听并优化根节点收集。
- **快捷键剪贴板无损备份恢复与持久化 (P1)**：在模拟按键发送前备份旧剪贴板并在翻译投递后完整还原；将剪贴板常驻开关持久化至系统安全存储。
- **收紧 CORS、CSP 与批量翻译防护 (P1)**：严密校验 Origin 域名与端口，禁止前缀绕过；收敛 WebView CSP 连接源至本地服务；限制单次批量条数与文本长度，防范越界与 DoS。
- **消除前端假译文伪装 (P1)**：在 IPC 调用失败时真实暴露错误原因并清空译文，杜绝把错误当成翻译结果显示。
- **缓存 Key 分隔符碰撞与术语库长词优先修复 (P2)**：缓存 Key 编码改用字段长度前缀，防止冒号分隔符冲突；术语库替换规则强制按源词长度降序执行，防止短词破坏长复合术语。

### 安全
- **系统原生凭据跨平台托管 (P0)**：API 密钥与内部鉴权 Token 全面接入操作系统原生安全存储（Windows Credential Manager / macOS Keychain / Linux Secret Service），严禁明文落盘。
- **Linux Secret Service 启用加密传输 (P1)**：为 Linux `keyring` 配置 `features = ["sync-secret-service", "crypto-rust"]`，确保 Linux 桌面凭据通信符合安全加密规范。
- **收紧生产 CSP 与 CORS 白名单 (P2)**：移除开发端口连接源，限制 CORS 仅允许标准扩展 Origin，拦截外部恶意网页嗅探与请求伪造。

- **扩展 Origin 专属白名单与 ID 格式校验**：在本地 HTTP 服务中实现 `is_allowed_origin` 校验器，严格限制 Chrome 扩展为标准字母数字 ID 格式（`chrome-extension://[a-z0-9]+`）、Firefox 扩展为合法 UUID（`moz-extension://[a-zA-Z0-9-]+`），生产环境拒绝空扩展名与非法伪装域名。
- **密码学安全随机 Token 生成**：使用 `rand::rngs::OsRng` 生成 32 字节 (256-bit) 强随机安全令牌，替换原本的时间戳+PID 拼接方式，彻底消除被探测伪造风险。
- **强化 Tauri 应用程序 CSP 防线**：在 `tauri.conf.json` 中配置严格的 Content Security Policy (`default-src 'self'; script-src 'self'; connect-src 'self' ...`)，阻断潜在的 XSS 攻击。
- **CI 引入 Gitleaks 密钥泄露扫描**：在 `.github/workflows/ci.yml` 中配置 Gitleaks 自动化扫描步骤，确保代码库无任何明文 Key 泄漏。

- **本地服务 CORS 来源策略加固**：收紧本地 HTTP 服务的 CORS 策略，严格限制仅允许浏览器扩展（`chrome-extension://`、`moz-extension://`）与本地开发回环请求，杜绝任意第三方恶意网页探测。

### 优化
- **术语库正则表达式预编译缓存**：在 `Glossary::insert` 时完成 ASCII 单词边界正则的预编译并缓存在 `TermEntry` 中，消除每次文本翻译时的重复动态编译开销。
- **DeepL 批量请求语言对同构性校验**：在 `DeepLEngine::translate_batch` 中增加全量语言一致性判定，在请求混杂多语种时安全降级为逐项翻译，避免取首项导致的批量乱译。
- **悬浮小窗文案全面国际化**：补齐浮窗所有静态文本的 i18n 多语言字典支持，消除硬编码。
- **纳入 Cargo.lock 并完善排除规则**：按应用类型仓库规范取消对 `Cargo.lock` 的忽略，确保跨平台与 CI 重现构建的一致性；明确排除 `extension/dist/`。
- **规范项目 AGENTS.md 准则**：将项目的核心分层规则、平台边界与安全底线沉淀至根目录 `AGENTS.md`。

- **Google 网页翻译长文本分块防溢出**：对单次传入超过 1200 字符的长段落自动实施换行/分段切片合并，彻底规避 GET URL 超长（HTTP 414 URI Too Long）与触发接口风控阻断。
- **术语库单词边界精准匹配**：在 `Glossary::apply` 中引入正规表达式单词边界识别（`\b(term)\b`），对英文 ASCII 单词强制边界匹配，彻底杜绝例如 `IT` 误替换 `limit` / `Wait` 的子串误伤问题。
- **LLM 批量翻译受控并发加速**：在 `OpenAIEngine::translate_batch` 引入 `futures::future::join_all` 受控分批并发处理（并发度 4），大幅缩短浏览器整页多节点批量翻译响应耗时。
- **SQLite 缓存大表容量修剪算法优化**：重构 `trim_to_max_items` 淘汰算法，使用基于时间游标与 rowid 偏移的高性能删除替代耗时的 `NOT IN` 全表子查询。

- **全局引擎连接池复用 (EnginePool)**：在 `src-tauri` 中引入 `EnginePool`，缓存已初始化的长连接实例并提供读写锁，杜绝单次翻译重复握手与新建网络连接；密钥更新时自动使对应实例失效。
- **本地 HTTP 服务与浏览器插件引擎传参支持**：`TranslateBatchRequest` 支持 `engine` 字段，浏览器扩展 Popup 支持引擎选择并持久化，整页翻译与增量监听按指定引擎流转。
- **前端国际化 (i18n) 资源拆分与中英切换**：创建 `src/i18n/` 模块，将硬编码中文文案抽取至 `zh-CN.json` 与 `en.json`，并支持主界面顶部一键切换中英文。

- **SQLite 缓存自动衰减与容量保护**：在 `transly-core` 中实现 `cleanup_older_than_days` 与 `trim_to_max_items` 淘汰策略，并在桌面端启动时自动执行 90 天过期与 100,000 条上限维护，防止本地数据库无序膨胀。

- **系统托盘与后台常驻机制**：Tauri 2 原生集成托盘图标、左键单击激活、右键上下文菜单（“显示主窗口” / “退出 Transly”）；拦截主窗口关闭事件转为后台隐藏，确保本地 HTTP 翻译服务不中断。
- **浏览器插件 MutationObserver 动态增量翻译**：首屏整页翻译完成后，自动挂载 DOM 变动监听器，针对 SPA 单页应用及瀑布流加载内容执行防抖增量收集与原地无感翻译；还原原文时自动注销监听。

- **引擎实例与连接池复用**：将 `TranslateEngine` 注入 Tauri 全局状态，消除重复创建 `reqwest::Client` 的握手损耗。
- **插件 Token 配对闭环**：桌面端支持一键复制插件配对 Token，插件 Popup 支持持久化保存 Token 与实时连通性探测。
- **语言检测增强**：扩展中（CJK）、日（平/片假名）、韩（Hangul）、英多语言特征字符判定。
- **CI 契约同步校验**：在 GitHub Actions CI 中增加 Specta 自动绑定代码比对检查（`git diff --exit-code src/bindings.ts`）。

### CI/CD
- **全平台自动化打包与 Release 流水线**：新增 `.github/workflows/release.yml`，在推送 `v*` 语义化标签时，利用 `tauri-apps/tauri-action` 矩阵自动化构建 Windows (`.exe` / `.msi`)、macOS (`.dmg`)、Linux (`.deb` / `.AppImage`) 安装包，并自动编译打包浏览器扩展 `.zip` 挂载至 GitHub Release 页面。
