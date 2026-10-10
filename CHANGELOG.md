# 更新日志

本项目的所有重要变更均记录于此文件。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
并且本项目严格遵循 [语义化版本 2.0.0](https://semver.org/lang/zh-CN/)。

## [未发布]

### 优化与增强
- **LLM 翻译专业领域风格预设 (P2)**：在 `crates/transly-core` 的 `OpenAIEngine` 中引入 `TranslationStyle` 枚举，支持【通用直译】、【学术技术】（严格锁定专业术语与代码标识符）、【文学润色】（流畅地道）与【商务公文】四种 Prompt 风格模板并自适应调节采样温度；桌面端设置中心新增全局风格选择器。
- **本地离线模型 (Ollama) 一键预置向导 (P2)**：在设置中心的 OpenAI 自定义端点配置区域提供一键填入本地 Ollama 默认 API 端点（`http://localhost:11434/v1`）与默认模型（`qwen2.5:7b`），方便离线安全翻译用户零门槛直连本地模型。
- **SQLite 翻译缓存读写分离并发解耦 (P1)**：将 `crates/transly-core` 中的 `TranslationCache` 升级为读写双连接分离架构（`File` 模式下采用独立 `writer` 互斥写连接与 `reader` 只读并发连接）。在 SQLite WAL 模式下，整页翻译的大批量事务写盘与用户前台的高频历史查询/缓存命中完全解耦、互不阻塞，杜绝锁争用卡顿。
- **现代 Web Components 开放 Shadow DOM 穿透翻译 (P2)**：在 `extension/src/content/index.ts` 中增强 DOM 遍历器，递归探测并收集拥有开放模式（`open`）Shadow Root 的自定义组件内部有效文本，彻底解决 Lit/Stencil 等现代前端组件库封装内容被整页翻译遗漏的局限。
- **术语库 Aho-Corasick 单趟多模式扫描重构 (P0)**：在 `crates/transly-core` 中重构 `Glossary::apply` 算法，引入高性能 `aho-corasick` crate (v1.1)。由原先按词长遍历的逐词 `Regex::replace_all` 升级为基于 `MatchKind::LeftmostLongest` 的单趟自动机流式扫描，匹配后精准执行后验 ASCII 单词边界检测，匹配性能从 $\mathcal{O}(N \times L)$ 优化至 $\mathcal{O}(L)$。彻底解决先前术语 A 替换为 B 之后 B 中文字被后续术语再次二次替换的级联缺陷，并天然保障长词优先命中。
- **浏览器扩展整页翻译 Viewport-First 优先切批调度 (P1)**：在 `extension/src/content/index.ts` 与 `shared/batchPlan.ts` 中实现视口感知切批调度。在收集待翻译 Text 节点时计算其实际几何包围盒，将当前可视视口内的可见文本优先拆批置顶并立即发送请求，使得用户在点击翻译后数百毫秒内即可看到当前屏幕文字变成译文；离屏（Offscreen）未滚动到的段落则平滑在后台批次中保序排队，大幅缩减首屏翻译等待的感官延迟。
- **浏览器扩展富文本段落行内标签保真翻译 (P1)**：在 `extension/src/shared/richText.ts` 中实现富文本行内占位提取与译文安全还原算法。针对 `<p>`、`<li>`、`<h1>`-`<h6>` 等段落中内嵌的 `<a>`、`<code>`、`<strong>` 等行内标签，以整句为单元进行占位保护（`⟦T_N⟧`）后统一翻译；返回后自动拆解回填，彻底杜绝跨语种（如中英文语序倒装）时离散文本节点被切碎翻译导致的语序破版，并在占位符丢失时无缝安全回退至逐节点原位替换。

### 新增
- **浏览器扩展当前站点自动翻译偏好记忆 (E11)**：在扩展 Popup 底栏增加「为此站点记住自动翻译」记忆开关；针对用户信任的外文技术或学术网站（如 GitHub、Reddit、ArXiv），在页面加载完成后自动触发整页沉浸式翻译，彻底消除手动点击翻译步骤。
- **浏览器扩展选区划词即时微型悬浮图标 (E9)**：在网页中选中文本后，在光标右上角通过隔离式封闭 Shadow DOM 动态浮现微型 Transly 图标按钮，点击即可直接平滑展开只读译文气泡，告别右键二级菜单的繁琐操作。
- **浏览器扩展整页双语对照翻译模式 (E10)**：在浏览器扩展 Popup 中增加「仅译文（原地替换）」与「双语对照」双模式切换单选组；双语对照模式下保持原 DOM 文本节点内容不变，并在其后自动追加带有行内样式隔离的兄弟译文节点，还原原文时一键自动批量清理，兼顾沉浸式仅译文排版与中英文对照阅读学习诉求。
- **Markdown 与代码块智能占位保真 (E4)**：在 `crates/transly-core` 中新增 `preserve` 模块，纯 Rust 实现多行代码块（```` ```lang ... ``` ````）与行内代码（`` `...` ``）智能提取与 Unicode 专有括线（`⟦TRANSLY_CODE_N⟧`）占位替换；翻译完成后精确还原，彻底解决传统翻译引擎误翻变量名、破坏缩进及破版的技术痛点。
- **LLM 流式请求物理中断与资源回收 (E2)**：在平台层 `src-tauri` 中引入 `StreamTaskRegistry` 任务取消注册表与 `cancel_stream_task` 命令；前端 `useTaskRunner` 状态机在用户手动取消或快速输入新代际文本时，向后端发送物理取消信号，底层通过 `tokio::select!` 立即中止在途流式连接，杜绝网络空耗与用户 Token 浪费。
- **全局快捷键可视化智能录制器 (E6)**：在设置中心新增 `ShortcutRecorder` 组件，支持一键点击监听键盘按键组合（支持 Ctrl、Alt、Shift、Super 及其与字母、数字、F1-F12、空格的组合），自动格式化并拦截非法冲突输入，提供 Escape 撤销与清除支持，彻底替代原有手动打字输入的繁琐交互。
- **划词悬浮窗图钉（Pin）置顶与受控粘滞 (E7)**：悬浮窗标题栏新增图钉切换按钮，支持在“失焦自动收起”与“永久置顶常驻”之间一键切换；配合平台层 `set_float_sticky` 保持置顶粘滞状态，方便用户对照长篇译文编写代码或记录笔记。
- **桌面端全界面深色模式 (Dark Mode) 适配 (E5)**：Tailwind 接入 `darkMode: 'class'`，在设置中心提供「浅色模式 / 深色模式 / 跟随系统」三态切换与持久化记忆；全面适配主翻译窗口、划词悬浮窗、历史记录面板、设置中心模态框及快捷键录制组件的暗色配色与高对比度排版。
- **离线语言检测与智能双向语言路由 (P0)**：在 `crates/transly-core` 中新增 `detect_language` 与 `resolve_smart_target_lang` 模块，纯 Rust 基于 Unicode 字符区段（汉字给予 2.5 倍权重支持中英夹杂技术文本准确识别、假名、谚文、西里尔、阿拉伯、拉丁字母）实现微秒级零网络离线语言探测。主界面与任务调度状态机接入「中英智能双向对转」：当用户目标语言设定为中文时，输入中文自动对转翻译为英文，输入非中文保持翻译为中文；反之亦然。主界面源文本区同步显示「检测为: {lang}」与「自动转译为: {lang}」智能路由标识，彻底消除频繁手动切换语言下拉框的繁琐交互。
- **划词原地翻译并替换全局生产力工作流 (P0)**：在平台层 `src-tauri` 中新增 `trigger_selection_replacement` 逻辑与 `get_replace_shortcut` / `set_replace_shortcut` 命令，注册默认全局热键 `Alt+Shift+R`（支持用户在设置中心自定义持久化配置）。按下快捷键后自动模拟复制选区文本、智能推断目标语言并异步翻译，译文生成后自动模拟粘贴覆盖原选区并妥善恢复用户原有剪贴板历史；引入全局 `CLIPBOARD_SIMULATION_MUTEX` 互斥排队锁彻底杜绝与悬浮划词并发竞争，自适应轮询上限调优至 300ms 兼顾大段长选区，执行期间全程置 `sel_flag` 抑制剪贴板监听器自触发，为跨应用写作、聊天与代码注释翻译提供一键原地直达体验。
- **全局划词快捷键自定义支持 (P1)**：在平台层 `src-tauri` 中新增 `get_shortcut` 与 `set_shortcut` 命令，支持从系统凭据持久化读取与更新用户自定义全局快捷键（如 `Ctrl+Shift+T`，默认 `Alt+Shift+D`）；并在设置中心提供快捷键录入与保存交互，热重载注册全局热键，避免与其他系统或应用快捷键冲突。
- **LLM 引擎自定义 Base URL 端点与模型名称 (P1)**：在设置中心为 DeepSeek 与 OpenAI 引擎新增端点与模型自定义输入支持（命令层新增 `save_llm_config` 与 `get_llm_config`，密钥安全存储于平台凭据库中）。`EnginePool` 引擎池根据自定义配置动态装配 `OpenAIEngine` 实例并在保存后即时失效旧缓存，全面支持用户接入本地 Ollama、vLLM、OneAPI 及私有部署大模型。
- **Firefox (Gecko) 浏览器扩展专属构建与打包适配 (P2)**：在 `extension/scripts/build-firefox.mjs` 中实现自动化 Firefox 产物生成，自动适配 Firefox MV3 规范（`browser_specific_settings.gecko` 与 `background.scripts` 兼容），并在根目录提供 `pnpm run build:extension:firefox` 一键构建 `dist-firefox` 产物。
- **主输入框快速清空与实时字符计数统计 (P3)**：在主翻译界面的源文本操作底栏新增「清空内容」快速操作按钮与字符实时统计标签（`{n} 字符`），与取消任务状态平滑联动，提升日常大段文本翻译时的交互便捷度。
- **翻译历史模糊搜索与导出 (CSV / JSON) (P1)**：在 `crates/transly-core` 的 `TranslationCache` 中实现 `search` 模糊查询，在命令层导出 `search_translation_history`。前端 `HistoryPanel` 增加实时搜索输入框（支持原文与译文模糊匹配）以及一键导出为 `.json` / `.csv` 文件功能，便于用户将翻译记录整理归档或用作生词复习资料。
- **系统原生 Web Speech 零开销发音朗读 (TTS) (P2)**：基于现代浏览器与 WebView 内置的 `window.speechSynthesis` 原生 API，在桌面端主界面译文区、悬浮浮窗模式以及历史记录条目旁提供发音朗读按钮（小喇叭图标），实现原文与目标语言文本的零体积、零依赖、零额外网络开销的原生语音朗读。
- **翻译缓存批量写入单次显式事务化 (P1)**：在 `crates/transly-core` 的 `TranslationCache` 中引入 `set_batch` 接口，内部使用 `conn.transaction()?` 与单次预编译语句批量落盘；服务端（`src-tauri/src/server/mod.rs`）在整页批量翻译成功后统一通过 `set_batch` 提交事务，将单次批量翻译的 30 次隐式加锁与写盘减少为 1 次批量提交，大幅降低高并发与整页翻译下的 SQLite 锁争用与磁盘 I/O。
- **翻译引擎网络瞬态故障受控重试与指数退避 (P1)**：在 `crates/transly-core` 的 `TranslateEngine` 内部引入 `send_with_retry` 网络请求重试机制，对底层连接中断、超时及服务端 429/500/502/503/504 等瞬态故障实施至多 1 次指数退避重试（200~300ms 抖动延迟）；严格对 401/403 等鉴权与输入错误绝不重试，大幅提升弱网与整页翻译时的鲁棒性，减少局部缺译。
- **浏览器扩展整页翻译悬浮查看原文 Tooltip (P1)**：在 `extension/src/content/index.ts` 中基于 `originalTextMap` 增加无侵入的原文悬浮查看交互。原地替换文本节点后向父级元素标记 `data-transly-has-translation`，鼠标悬停时在封闭式 Shadow DOM 浮层中呈现对应原文字句，还原网页时自动彻底销毁，兼顾“整页仅译文保真排版”与“随时对照原文”的双重诉求。
- **i18n 语言包键值结构完全对称性自动化单元测试 (P2)**：新增 `src/task/i18n.test.ts`，递归遍历并深度比对桌面端（`zh-CN.json` 与 `en.json`）及浏览器插件（`_locales/zh_CN` 与 `_locales/en`）所有语言包的键路径与结构，在提交门禁与 CI 中杜绝遗漏翻译键或运行时 `undefined` 文案。
- **浏览器插件与桌面端一键自动安全握手配对 (P1)**：在本地 HTTP 服务（`127.0.0.1:39281`）中实现 `/pair/request` 与 `/pair/poll` 握手协议与 `PairingManager` 内存状态机（60 秒有效时效限制、单次读取一次性消费销毁）；在桌面端通过 Tauri 事件 `transly://pairing-requested` 广播配对申请，并在 React 前端弹出带 60s 倒计时动画的来源授权确认模态浮层（`approve_pairing` / `reject_pairing`）；浏览器插件端（`extension/`）在未配对或 Token 失效时提供【一键配对桌面端】交互，自动轮询并无缝写入 `chrome.storage.local`，彻底免除手动复制粘贴 64 位 Token 的繁琐操作，同时杜绝未授权扩展静默窃取风险。
- **术语库支持「忽略大小写」(Case-insensitive) 选项 (P2)**：在 `crates/transly-core` 的 `Glossary` 中引入 `insert_with_options` 与 `match_case` 开关，支持对 ASCII 单词与多词术语启用不区分大小写匹配（通过 `(?i)` 正则标志支持）；SQLite 持久化数据表通过 `ALTER TABLE glossary ADD COLUMN match_case ...` 自动向前迁移并保持平滑兼容；命令层与前端 DTO 对齐 `GlossaryTermDto`；设置面板新增「区分大小写」复选框与不区分大小写（`Aa`）标识。
- **浮窗与扩展端网络错误引导对齐 (P2)**：当用户使用默认 Google 网页翻译遇到网络无法直连时，浏览器扩展端（网页划词翻译气泡与 Popup 整页翻译面板）及桌面端浮窗同步提供醒目明确的可执行替代指引（“Google 翻译网络不可达，请在桌面端设置配置 DeepL / DeepSeek 后切换引擎”），中英文语言包同步对齐。
- **浏览器扩展整页翻译 MutationObserver 动态增量翻译与定时器防泄漏加固 (P2)**：在 `extension/src/content/index.ts` 中加固动态 DOM 监听器的生命周期管理，新增模块级 `observerDebounceTimer` 与 `observerPendingNodes` 清理机制，在整页翻译重新触发、代际变更及「还原原文」时彻底清空挂起节点并断开监听器，消除无限滚动页面潜在的内存与异步定时器泄漏风险。
- **LLM 引擎流式 (Streaming) 打字机实时输出 (P1)**：在 `crates/transly-core` 的 `TranslateEngine` trait 中引入 `supports_streaming` 与 `translate_stream` 规范，为 `OpenAIEngine`（OpenAI 与 DeepSeek）启用 SSE (`stream: true`) 流式解析协议，实时提取 Token chunk 并触发 `on_chunk` 回调。桌面端命令层新增 `translate_stream` 命令，通过 Tauri 事件总线实时向当前窗口广播 `translate-stream-chunk` 增量分块。前端 `useTaskRunner` 状态机支持事件流增量累加与代际隔离，主界面译文区新增打字机光标脉冲（`animate-pulse`）动效与换行保真；流式生成完成后统一执行全局术语库规则（`Glossary::apply`）修正并闭环写回 SQLite 缓存；缓存命中时秒级分发，零网络延迟。
- **划词模拟复制自适应短轮询与容错加固 (P2)**：在 `src-tauri/src/clipboard_gate.rs` 中实现 `poll_clipboard_change` 异步短轮询机制（15ms 周期，180ms 上限），替代原本写死硬等的 `sleep(80ms)`。内容发生变化时立即放行返回（高配机延迟直降 60ms+），超时则平滑兜底，全空内容安全过滤。并在快捷键划词恢复原剪贴板时增加系统锁占用异常日志隔离，消除静默失败与潜在崩溃风险。
- **扩展网页内划词翻译（右键菜单 + Shadow DOM 气泡）**：在任意网页选中文本后右键选择「Transly 翻译选中文本」，即可在选区旁弹出只读译文气泡，支持一键复制与 `Esc` / 点击外部关闭。气泡渲染在 **封闭式 Shadow DOM**（`mode: 'closed'` + `all: initial` + `adoptedStyleSheets`）中，宿主页面 CSS 无法污染、宿主脚本无法读取；宿主节点带 `data-no-translate` 且挂在 `documentElement` 上（而非 `body`），因此整页翻译的 TreeWalker 从源头就不会把气泡自身文案当作待翻译正文。脚本沿用既有的**主动注入**模式：新增 `contextMenus` 权限，仍不申请 `<all_urls>`、不常驻注入、不监听 `mouseup`，维持 `PRIVACY_POLICY.md` 的隐私承诺。翻译链路 100% 复用现有 `TRANSLATE_BATCH` 与 `/translate`，目标语言与引擎沿用 Popup 中的配置；选区锚点由页面侧读取 `window.getSelection()` 的 Range 矩形获得（右键菜单回调只有文本、没有几何信息），滚动/缩放时按记录时的滚动偏移回推坐标重新定位。
- **桌面端翻译历史面板**：把一直存在但用户不可见的 `translation_cache` 表变为可视化「最近翻译」。主区新增「翻译 / 历史」顶层 Tab；历史页支持倒序分页浏览（每页 50 条，可加载更多）、单条复制、单条删除与刷新，空态给出「开启持久化缓存后记录会出现在这里」的引导。`transly-core` 新增 `CacheEntry` DTO 与 `list_recent` / `delete_entry`；命令层新增 `get_translation_history` / `delete_history_entry`（均下沉 `spawn_blocking`，SQLite 同步 I/O 不占 async 工作线程）。`created_at` 以 `u32` 纪元秒导出，规避 specta 将 `i64` 映射为 JS `BigInt` 导致的 `JSON.stringify` 崩溃（Lessons Learned 第 1 条）。该表按 `cache_key` 去重，故语义为「近期不重复的翻译记录」，而非逐次操作日志。

### 修复
- **Tauri 2 窗口权限 ACL 缺失导致扩展自动配对时主窗口无法唤醒 (P1)**：在 `src-tauri/capabilities/default.json` 中补齐 `core:window:allow-show` 与 `core:window:allow-set-focus` 权限；主窗口常驻后台或关闭到托盘时，扩展一键配对请求可被可靠拉起置顶并聚焦。
- **悬浮窗手动 Pin 状态在翻译完成时被自动粘滞机制覆盖关闭 (P1)**：`useTaskRunner` 粘滞判定接入 `isPinned` 配置，确保在翻译完成恢复 `idle` 时不会单方面向后端重置粘滞标志，保护用户手动置顶浮窗的操作不被意外冲掉。
- **快捷键划词翻译遗漏光标位置计算导致浮窗弹出位置漂移 (P1)**：在 `trigger_selection_translation` 流程中补齐 `place_float_near_cursor` 调用，确保选区划词唤起浮窗时紧随光标所在显示器选区下方，并消除 `main.rs` 中的冗余重复实现。
- **术语库 Aho-Corasick 大小写折叠与后验过滤冲突导致模式被静默吞掉 (P1)**：重构 `Glossary::apply` 算法，改用 `MatchKind::Standard` 配合 `find_overlapping_iter` 提取全候选区间并在业务层贪婪排序匹配，彻底杜绝全局大小写折叠导致同形大小写敏感词或子模式被提前剪枝丢弃的隐患。
- **多引擎对比翻译模式 loading 状态聚合、请求竞态与取消支持补齐 (P2)**：在主界面对比翻译模式中引入 `isComparing` 聚合状态合并入 `isTranslating`，接入代际版本号隔离丢弃迟到响应，支持在途任务取消与防重复点击。
- **LLM 领域风格选择（TranslationStyle）前后端参数打通 (P1)**：在命令层新增 `save_llm_style` 与 `get_llm_style` 命令并持久化至系统安全凭据库，`EnginePool` 动态装配风格至 `OpenAIEngine`，打通前端设置中心与底层翻译引擎。
- **划词原地替换增加 Markdown 代码块保护与文本长度防线 (P2)**：在 `trigger_selection_replacement` 流程中接入 `transly_core::stash_code_blocks` 与 `restore_code_blocks`，防止代码段与变量名被误翻，并增加字符长度上限校验。
- **流式 LLM SSE 解析跨网络 Chunk 多字节 UTF-8 截断乱码修复 (P2)**：改用字节缓冲区累计字节并在换行符处按行对齐解码，彻底杜绝中文等多字节字符被 TCP 分包拆开时产生 `\u{FFFD}` 乱码。
- **FallbackEngine 流式降级补全 `on_chunk` 增量广播 (P2)**：流式主引擎报错降级时通过共享回调代理继续调用备用引擎流式接口，确保降级成功后打字机视图正常接收译文。
- **后台剪贴板监听在禁用状态下降频休眠，减少常驻空唤醒 (P3)**：在监听关闭时将轮询休眠时间从 500ms 降频至 1500ms，大幅减少后台空唤醒与 CPU/电量损耗。
- **翻译历史 CSV 导出增加电子表格公式注入防御 (P3)**：对以 `=,+,-,@,\t,\r` 开头的文本前置单引号转义，防御 CSV 电子表格公式注入攻击。
- **扩展构建把共享模块拆成独立 chunk，Popup 与划词气泡会静默失效 (P1)**：`extension` 的构建是单次多入口（`rollupOptions.input` 含 4 个入口），`src/i18n` 被多个入口共享后会被 Rollup 提升为独立 chunk，`popup.js` / `selection.js` 于是以 `import { t } from "./assets/index-*.js"` 开头。但 `chrome.scripting.executeScript` 按**经典脚本**注入、`popup.html` 也用经典 `<script src>`，二者都不支持 ESM 导入 —— 扩展能正常加载、控制台无任何报错，但 Popup 与划词气泡全部失效。此前只有 3 个入口且恰好无共享模块，该缺陷一直被掩盖。现改为 `scripts/build.mjs` 逐入口独立构建（每个产物自包含），并在构建后**断言产物不含 ESM 导入、`dist/assets/` 不存在**，让该类回归无法再次静默通过。

## [0.2.1] - 2026-10-08

### 修复
- **CI 首次真正运行暴露 Linux 平台专属编译失败 (P1)**：仓库 GitHub Actions 权限长期处于卡死状态，`push` / `tag` 事件从未触发任何工作流（CI 历史运行数为 0），导致 `.github/workflows/ci.yml` 实际上从未校验过任何提交。重置仓库 Actions 权限开关后 CI 首次运行，立即在 Linux 作业上暴露 `src-tauri/src/ocr.rs` 的缺陷：Windows 专属 import（`ClipboardExt` 与 `clamp_dimensions` / `rgba_to_bgra_in_place` / `build_candidate_languages`）被写在文件顶层，非 Windows 平台下这些符号不被任何代码使用，在 `cargo clippy -- -D warnings` 下直接以 `unused_imports` 编译失败。Windows 本地 clippy 恒通过，故该缺陷此前从未被发现。现将上述 import 用 `#[cfg(target_os = "windows")]` 门控，仅保留跨平台共用的 `OcrErrorDto`。
- **扩展在途批次绕过代际闸门，「还原原文」后译文被重新写回 (P1)**：`sendBatchWithSplit` 收到响应后立即调用 `applyBatchResponse`，而 `translateEntries` 的「响应写回前安全拦截」要等其返回后才执行；`applyBatchResponse` 的 `items` 主契约分支（当前唯一路径）内部**没有**代际判定，只有旧版 `translations` 兼容分支有。用户点击「还原原文」（`currentGeneration += 1`）或取消后，只要仍有批次在途，译文就会被原地写回页面——用户看到的是「还原失败」，与代码注释宣称的「严禁写入 DOM」直接矛盾。现把代际闸门下沉进 `applyBatchResponse` 入口统一判定，并对 `markBatchFailed` 的两处调用点加同样守卫，避免还原后失败集被污染导致「重试失败项」按钮凭空出现。
- **设置面板「立即清空」缓存无任何错误处理 (P3)**：`App.tsx` 的 `clearCache` 按钮只处理成功分支，既无 `else` 也无 `catch`，与同面板其他操作（保存密钥、轮换 Token、术语增删）的防御性写法不一致。IPC 抛错或返回 `error` 时用户点击后**毫无反馈**并产生未处理的 Promise 拒绝。现补齐 `error` 分支与 `catch`，并新增 `statusCacheClearFailed` 文案（中英同步）。
- **保存密钥 / 轮换 Token 等操作吞掉后端具体错误 (P3)**：`handleSaveKeys`、`handleRotateToken`、`handleDeleteKey`、`handleAddGlossary`、`handleDeleteGlossary` 的 `catch` 分支只写通用文案，丢弃后端返回的 `res.error`（如「API 密钥长度超出限制」「写入系统安全凭据失败」）。现统一优先透传后端原始消息，仅在其缺失时回落通用文案，与 `setCacheEnabled` 的既有行为对齐。
- **扩展批量上限与服务端量纲错配导致整批静默失败 (P0)**：扩展端按「项数 30」切批，服务端按「字符总量 50000」拒绝——两者量纲不同。30 个平均 1667 字的正文节点字符总量即越过上限，服务端返回 413，该批**全部**节点被标为失败且页面无任何提示。现把 `MAX_BATCH_ITEMS` / `MAX_BATCH_TOTAL_CHARS` / `MAX_ITEM_CHARS` 收敛到 `transly-core` 单一来源，服务端在 `/health` 公开，扩展端新增 `shared/batchPlan.ts` 的 `planBatchRanges` 按双上限切批；并加 413 对半拆批兜底（服务端收紧上限时无需两端同时升级）。新增 5 项 Rust 回归测试（含「项数未超但总量超限必须 413」的核心用例）与 6 项 TS 单测。
- **浮窗并发请求被覆盖丢失 (P0)**：`PendingFloatRequest` 是单槽 `Option`，而划词（快捷键）与剪贴板监听是两条独立链路共用同一槽位，后到的请求直接覆盖先到的。浮窗 JS 未就绪时（首次呼出、WebView 重建）先到的请求永久丢失，用户表现为「第一次划词没反应」。现改为有界 FIFO 队列 `PendingFloatRequests`（容量 16，溢出丢弃最旧项并打印告警），前端循环排空队列而非只拉一次。新增 2 项单测锁定 FIFO 顺序与容量边界。
- **构建期绑定导出路径越界 (P1)**：`main.rs` 在 `debug_assertions` 下执行 `.export(Typescript::default(), "../src/bindings.ts")`，相对路径被按**进程 cwd** 解析。从仓库根执行 `cargo test` / `cargo run` 时会在仓库外（实测 `D:/Project/src/bindings.ts`）凭空生成文件。现改为相对 `CARGO_MANIFEST_DIR` 解析，与 `bin/export_bindings.rs` 的写法对齐，并已实测确认仓库外不再生成。
- **校验错误构造路径不统一 (P2)**：`translate` 命令内有 3 处手工拼装 `TranslateErrorDto { kind: ErrorKind::Input, retryable: false }`，绕过 `classify_error`。新增校验项时极易漏改某一条。现 `validate_translation_input` / `validate_language_pair` 统一返回 `Error::Input`，调用方一律走 `TranslateErrorDto::from_error`；HTTP 侧语言校验也改为复用同一函数，消除「不支持该语言」的两份判定。
- **扩展 `failedItemsMap` 无界增长 (P2)**：强引用 Map 随 SPA 导航与多次还原无限增长，并钉住已脱离文档的 Text 节点。现容量封顶 2000，超出丢弃最旧项（新增 `rememberFailed`）。
- **扩展动态监听持有陈旧代际 (P2)**：`startDynamicObserver` 接收调用方传入的 `generation` 并在闭包内比较。但该监听是单例（重复调用直接 return），重试或新任务后监听器仍持旧代际，导致增量节点永久不再收集。现改为闭包内读取模块级 `currentGeneration`。
- **扩展进度快照不清理 (P3)**：`tabProgressMap` 按 tabId 累积且无清理路径，Service Worker 长生命周期下持续增长。现接入 `chrome.tabs.onRemoved` 与 `onUpdated(loading)`。
- **空 `catch` 吞掉可诊断错误 (P2)**：`App.tsx` 5 处 `.catch(() => {})` 改为 `console.warn`（读取术语库、3 个引擎密钥状态、引擎能力）；`useTaskRunner` 的 `setFloatSticky` 同样暴露。`Lessons Learned` 第 8 条已写明该规程（ACL 拒绝曾被静默数月），但本轮之前只落实了事件订阅一处。
- **`useTaskRunner.retry` 无历史请求时静默失效 (P3)**：`retry` 在 `lastRunRef` 为空时直接 `return`，按钮看起来失灵且无任何提示。现加 `console.warn` 说明原因。

### 变更
- **lefthook 提交门禁真正生效 (P2)**：`PROJECT_RULES.md` 声称「提交前通过 lefthook 自动校验」，但 lefthook 既不在依赖、也无 `prepare` 脚本，`.git/hooks` 只有 `.sample`——该规则从未生效。现新增 `lefthook` 依赖与 `prepare: lefthook install`，并在 `pnpm-workspace.yaml` 的 `allowBuilds` 放行其 postinstall（需下载平台二进制）。
- **文档与实现对齐 (P2)**：`PROJECT_RULES.md`、`extension/README.md`、`PRIVACY_POLICY.md` 中「聚合**可视区域**文本块」的表述与实现不符（实为全量 `document.body` 遍历，不做可视裁剪），已如实改写；隐私政策同时补明提取范围排除脚本/样式/代码块/输入控件/可编辑区域。`PROJECT_RULES.md` 新增「批量请求上限（单一契约来源）」一节。
- **清理死代码 (P3)**：移除仅被自身单测调用的 `TranslationResponse::is_fallback`、`BatchItemResult::is_success` / `engine`（改由测试直接匹配枚举变体）、`AppConfig::new`（只被单测调用）；`MIN_CLIPBOARD_CHARS` 收窄为模块私有。
- **`test:task` 覆盖扩展端纯函数 (P3)**：根 `test:task` 增加 `extension/src/shared/*.test.ts`；扩展 `tsconfig.json` 排除测试文件（其 `node:test` 依赖不在扩展运行时类型集内）。
- **缓存目录收敛到 Tauri 应用目录约定 (P2)**：原先手拼 `%APPDATA%\Transly`，与 `identifier` 约定（`data_dir/<identifier>`）不一致，macOS 上也不符合平台惯例。现由平台适配层解析为 `data_dir/com.transly.desktop`。**关键点：术语库与翻译缓存在同一个 SQLite 文件里，直接改路径等于丢弃用户自建术语**，因此新增 `storage.rs` 一次性迁移（含 `-wal` / `-shm` 旁路文件，幂等且不覆盖新库），并配 4 项单测；`transly-core` 相应改为 `open_default(data_dir)` 由宿主注入目录，保持零平台假设。
- **落地“禁止空 catch 吞掉 IPC 注册类调用”规程 (P2)**：`Lessons Learned` 第 8 条早已写明该规程，但修复 ACL 问题时并未执行。现 `useTaskRunner` 的事件订阅与 pending 拉取失败改为 `console.warn` 暴露（这正是当初 ACL 拒绝静默数月的根因）。
- **前端格式化与 React Hooks 规则接入 (P2)**：`PROJECT_RULES.md` 声称「ESLint + Prettier」，实际无 prettier 依赖、无配置、无格式化脚本，前端代码风格零强制（同文件出现 `catch(() => {})` 与 `catch(() => { })` 两种写法）。现新增 `.prettierrc.json` / `.prettierignore` 与 `format` / `format:check` 脚本，接入 `lefthook` 与 CI；同时接入 `eslint-plugin-react-hooks`（`rules-of-hooks` / `exhaustive-deps`）——`useTaskRunner` 的代际闸门与依赖数组是整套取消/重试语义的根基，此前无任何规则守护。
- **清理未使用图标与构建产物 (P3)**：删除 `src-tauri/icons/` 下 45 个未被 `tauri.conf.json` 引用的图标（Windows Store `Square*`/`StoreLogo`、`android/mipmap-*`、`ios/*`），其中 4 个仍是模板替换失败的 `{{EMAIL_*}}` 残留；`src-tauri/gen/schemas/` 改为 `.gitignore` 忽略（tauri-build 每次构建重新生成，此前跟踪了 320KB 产物）。
- **清理零调用死代码 (P3)**：移除仅被自身单测调用的 `rgba_to_bgra`（生产只用 `rgba_to_bgra_in_place`）、零调用的 `impl From<Error> for TranslateErrorDto`、无人读取的 `totalBatches`、仅同文件使用的 `translations` 导出；`nextState` 的 cancel 分支改用 `isTerminal()`，使该函数从死代码变为有生产消费者且与终态集合永不漂移；`EnginePool::with_secret_getter` 收窄为 `#[cfg(test)]`。
- **文档与 CI 与实际对齐 (P3)**：修正 `src-tauri/README.md` 中 Tauri 1 时代的 `custom-protocol` 特性说明（Tauri 2 已移除该特性）；`PROJECT_RULES.md` 删除「调用系统截图能力」（实际仅读剪贴板图片）与「支持多引擎聚合/对比」（实际只有主备降级）两处未实现表述，补充 `storage.rs` 与 capabilities 目录说明；CI Windows job 补齐 `cargo fmt --check` 与 `--release` clippy，与 Linux job 矩阵对齐；`en.json` 类型断言改为显式标注，使英文缺键在编译期报错而非被 `as` 静默掩盖。
- **移除零使用契约与死代码 (P2)**：删除从未被构造的 `Error::UnsupportedLanguage` 与 `Error::Timeout`（连带 `ErrorKind::Timeout`，其重试语义已由 `ErrorKind::Network` 与 `Engine` 覆盖）、无生产消费者的 `TaskState::is_terminal` / `is_active`、无人消费的 `TranslateOutcomeDto::cached` / `detected_source_lang` 与 `EngineCapabilities::supports_streaming`（无任何流式实现）、以及未被调用的 `useTaskRunner.reset`。
- **文档与目录事实对齐 (P2)**：`PROJECT_RULES.md` 目录结构此前描述了不存在的 `src/components`、`src/pages`、`src/hooks`、`src/stores`、`src-tauri/src/platform`、`src-tauri/src/error.rs`，已按实际文件重写；新增 `docs/README.md`、`src/task/README.md`、`extension/src/README.md`，并为 `docs/architecture-state-machines.md` 建立索引（此前全仓库无任何引用）。
- **缓存目录收敛到 Tauri 应用目录约定 (P2)**：原先手拼 `%APPDATA%\Transly`，与 `identifier` 约定（`data_dir/<identifier>`）不一致，macOS 上也不符合平台惯例。现由平台适配层解析为 `data_dir/com.transly.desktop`。**关键点：术语库与翻译缓存在同一个 SQLite 文件里，直接改路径等于丢弃用户自建术语**，因此新增 `storage.rs` 一次性迁移（含 `-wal` / `-shm` 旁路文件，幂等且不覆盖新库），并配 4 项单测；`transly-core` 相应改为 `open_default(data_dir)` 由宿主注入目录，保持零平台假设。
- **落地“禁止空 catch 吞掉 IPC 注册类调用”规程 (P2)**：`Lessons Learned` 第 8 条早已写明该规程，但修复 ACL 问题时并未执行。现 `useTaskRunner` 的事件订阅与 pending 拉取失败改为 `console.warn` 暴露（这正是当初 ACL 拒绝静默数月的根因）。
- **前端格式化与 React Hooks 规则接入 (P2)**：`PROJECT_RULES.md` 声称「ESLint + Prettier」，实际无 prettier 依赖、无配置、无格式化脚本，前端代码风格零强制（同文件出现 `catch(() => {})` 与 `catch(() => { })` 两种写法）。现新增 `.prettierrc.json` / `.prettierignore` 与 `format` / `format:check` 脚本，接入 `lefthook` 与 CI；同时接入 `eslint-plugin-react-hooks`（`rules-of-hooks` / `exhaustive-deps`）——`useTaskRunner` 的代际闸门与依赖数组是整套取消/重试语义的根基，此前无任何规则守护。
- **清理未使用图标与构建产物 (P3)**：删除 `src-tauri/icons/` 下 45 个未被 `tauri.conf.json` 引用的图标（Windows Store `Square*`/`StoreLogo`、`android/mipmap-*`、`ios/*`），其中 4 个仍是模板替换失败的 `{{EMAIL_*}}` 残留；`src-tauri/gen/schemas/` 改为 `.gitignore` 忽略（tauri-build 每次构建重新生成，此前跟踪了 320KB 产物）。
- **清理零调用死代码 (P3)**：移除仅被自身单测调用的 `rgba_to_bgra`（生产只用 `rgba_to_bgra_in_place`）、零调用的 `impl From<Error> for TranslateErrorDto`、无人读取的 `totalBatches`、仅同文件使用的 `translations` 导出；`nextState` 的 cancel 分支改用 `isTerminal()`，使该函数从死代码变为有生产消费者且与终态集合永不漂移；`EnginePool::with_secret_getter` 收窄为 `#[cfg(test)]`。
- **文档与 CI 与实际对齐 (P3)**：修正 `src-tauri/README.md` 中 Tauri 1 时代的 `custom-protocol` 特性说明（Tauri 2 已移除该特性）；`PROJECT_RULES.md` 删除「调用系统截图能力」（实际仅读剪贴板图片）与「支持多引擎聚合/对比」（实际只有主备降级）两处未实现表述，补充 `storage.rs` 与 capabilities 目录说明；CI Windows job 补齐 `cargo fmt --check` 与 `--release` clippy，与 Linux job 矩阵对齐；`en.json` 类型断言改为显式标注，使英文缺键在编译期报错而非被 `as` 静默掩盖。
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

### 变更
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
- **全平台自动化打包与 Release 流水线**：新增 `.github/workflows/release.yml`，在推送 `v*` 语义化标签时，利用 `tauri-apps/tauri-action` 矩阵自动化构建 Windows (`.exe` / `.msi`)、macOS (`.dmg`)、Linux (`.deb` / `.AppImage`) 安装包，并自动编译打包浏览器扩展 `.zip` 挂载至 GitHub Release 页面。

[未发布]: https://github.com/Moody-Sin/Transly/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/Moody-Sin/Transly/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/Moody-Sin/Transly/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/Moody-Sin/Transly/releases/tag/v0.1.0
