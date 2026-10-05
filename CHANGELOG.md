# 更新日志

本项目的所有重要变更均记录于此文件。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
并且本项目严格遵循 [语义化版本 2.0.0](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### 修复
- **同步门禁锁消除缓存判断与写入竞态 (P1)**：在 `CacheController` 中引入 `sync_gate` 互斥门禁，提供原子 `set_if_current` 与 `disable_and_clear` 保证判断与写入位于同一原子临界区，并在 `server/mod.rs` 补充并发关闭与写回阻断强断言回归测试，彻底关闭清空与写入交错漏洞。
- **清空缓存错误严格冒泡与事务保护 (P2)**：`set_cache_enabled` 在关闭缓存清空历史失败时，强制向前端暴露错误，杜绝静默失败与假成功。
- **防范并发翻译关闭缓存后的写回竞争 (P1)**：在写入缓存的临界点再次读取原子开关 `cache_enabled_flag`，彻底消除长耗时外部引擎翻译返回后重新将数据写回已关闭缓存的竞争窗口。
- **完善扩展连接与 Token 鉴权状态反馈 (P2)**：扩展 Popup 增加深度鉴权校验，准确细分“桌面端未运行”、“未配对 Token”、“Token 已失效，请重新配对”以及“已连接且配对有效”四种明确状态，杜绝无效 Token 误显示已连接。
- **强化清空缓存错误处理与 i18n 完整国际化 (P2/P3)**：`set_cache_enabled` 在清空历史缓存失败时向前端准确反馈并支持重试；补全 Token 轮换与缓存管理文案的英文国际化字典。

### 安全
- **本地服务 Token 动态轮换与即时撤销 (P2)**：实现 `rotate_server_token` 命令与内存 `RwLock<String>` 动态更新，前端设置中心提供“重新生成 Token”操作；旧 Token 立即返回 401 Unauthorized 失效，生成后新 Token 持久化进原生系统凭据库，彻底消除长期静态 Token 暴露风险。
- **验证缓存清空绝对保留术语库独立性 (P2)**：增加单元测试 `test_clear_all_cache_retains_glossary`，强断言证明清空全部翻译历史时，用户自定义术语库 100% 完整保留不受任何影响。

### 修复
- **隔离全局缓存开关并发竞争并消灭排队内存堆积 (P1)**：在内存状态中维护原子布尔 `cache_enabled_flag`，彻底消除运行时与并行测试中频繁同步读写外部系统凭据库的竞争冲突；并发控制采用 `try_acquire()` 拒绝超额请求（返回 429 Too Many Requests），防止排队堆积与内存膨胀。
- **关闭缓存时同步清空历史磁盘数据 (P2)**：用户在设置中关闭“持久化翻译缓存”时，自动触发清空已有 `translation_cache` 磁盘表（保留术语库），切实保障用户隐私不留存。
- **本地服务全局并发受控限制 (P1)**：在本地 HTTP 服务状态中集成 `tokio::sync::Semaphore`，将同时处理的批量翻译任务严格限制在至多 10 批，防范网页动态 DOM 频繁变更造成的并发过载。
- **杜绝未知引擎静默降级 (P2)**：在 `translate_batch_handler` 与桌面 IPC 中实现 `validate_engine_id` 强校验，对未知或未受支持的引擎代码直接返回 400 错误，严防静默降级导致用户误用。
- **收紧语言代码校验白名单 (P2)**：将语言代码规则收敛为标准受支持白名单枚举校验，阻断潜在非法格式注入。
- **强化语言参数白名单与批量请求防线 (P1)**：在 `transly-core` 建立 `validation` 模块统一语言代码（BCP-47 规范）与字符合法性校验；本地 HTTP 服务增加 2MB 请求体上限、单次批量 50,000 总字符防线，全面拦截长语言代码注入与 DoS。
- **限制 API Key 凭据存储目标引擎枚举 (P2)**：在 `save_api_key` 与 `delete_api_key` 中严格限制目标仅允许受支持的 `deepl`、`openai`、`deepseek`，杜绝写入任意键名。
- **代码格式化与缓存关闭路径自动化测试补齐**：执行 `cargo fmt --all` 消除格式漂移；为本地 HTTP 批量服务补充 `test_translate_cache_disabled_path` 测试，强断言在 `cache_enabled == false` 下绝对不向本地 SQLite 写入新条目。
- **扩展批量翻译严格遵循缓存隐私开关 (P1)**：在本地 HTTP 服务的 `translate_batch_handler` 中集成 `cache_enabled` 共享判定，关闭持久化翻译缓存后彻底停止查询与写入 SQLite 磁盘库，确保桌面端与浏览器扩展两路隐私策略完全一致。

### 安全
- **Linux Secret Service 启用加密传输 (P1)**：为 Linux `keyring` 配置 `features = ["sync-secret-service", "crypto-rust"]`，确保 Linux 桌面凭据通信符合安全加密规范。
- **凭据持久化单测强断言 (P2)**：在 `secrets.rs` 增加强制写入、读取与 Token 连续性双向强断言单测，杜绝异常被静默跳过。
- **提供关闭持久化翻译缓存开关 (P2)**：在 IPC 与前端设置中心提供“持久化翻译缓存”开关，关闭后彻底停止写入 SQLite 磁盘库，充分保障用户隐私数据不落盘。
- **修剪 AppConfig 配置模型 (P2)**：移除未接入的虚假抽象字段，将 `local_server_port` 与 `cache_enabled` 真实打通至运行链路。

### 安全
- **Linux 接入 Secret Service 原生持久化后端 (P1)**：切换 `keyring` Linux feature 为 `sync-secret-service`，在 CI/CD 中安装 `libdbus-1-dev` 与 `gnome-keyring`，实现跨平台凭据（Windows / macOS / Linux）生命周期永久持久化对齐。
- **收紧生产 CSP 端口白名单 (P2)**：彻底移除生产环境中的 `http://localhost:5173` Vite 连接端口，生产构建仅允许连接本地翻译服务 `http://127.0.0.1:39281`。

### 优化
- **术语库同长度稳定排序与消除级联 (P2)**：排序规则重构为 `b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(b.0))`，实现跨平台确定性替换，补充 `test_glossary_same_length_deterministic` 单元测试。
- **剪贴板开关持久化错误传播 (P2)**：将 `set_clipboard_monitor` 调整为 `Result<(), String>`，凭据存储失败时向前端准确反馈并回滚 UI 开关。
- **接入 AppConfig 配置模型至应用启动 (P2)**：消除未使用的配置抽象，在桌面启动与服务绑定中深度集成 `AppConfig`。

### 修复
- **启用 Keyring 平台原生凭据后端 (P0)**：在 `Cargo.toml` 中按操作系统细化配置：Windows (`windows-native`)、macOS (`apple-native`)、Linux (`linux-native`)，解决原本回退至临时内存 Mock 导致的密钥无法跨进程/跨重启持久化严重缺陷；强化 `set_secret` 错误传播与凭据往返单元测试。
- **防止术语库特殊字符被正则错误展开 (P1)**：在 `Glossary::apply` 中使用 `regex::NoExpand` 包装目标文本，确保含 `$`、反斜杠的译文（如 `USD → $100`）按纯字面量替换，杜绝捕获组语法误伤。
- **提供本地 SQLite 翻译缓存一键清空支持 (P1)**：在 `transly-core` 与 Tauri IPC 中增加 `clear_all_cache` 命令，并在前端设置中心提供可视化的“清空翻译缓存”操作，提升用户对本地存储的隐私掌控力。
- **消除前端事件死监听 (P2)**：移除前端冗余的 `quick-translate` 事件监听，统一收敛为悬浮窗事件通道 `float-translate`。

### 修复
- **杜绝固定 Token 降级隐患 (P0)**：当系统原生凭据管理器不可用或故障时，明确拒绝启动本地 HTTP 扩展服务，彻底删除 `transly-default-token` 固定回退硬编码。
- **Release 模式 Clippy 校验与 CI 门禁闭环 (P1)**：将 `specta_typescript::Typescript` 导入约束在 `#[cfg(debug_assertions)]`，在 CI 流水线中补充 `cargo clippy --release` 校验。
- **收紧 CORS 跨域与生产环境限制 (P1)**：限制 CORS Origin 扩展协议并仅在 debug 构建开放本地回环端口调试。
- **前端 TypedError 状态全量防御检查 (P1)**：在删除密钥、添加术语、删除术语等操作中严格校验 `res.status === 'ok'`，失败时准确暴露错误并保留上下文。
- **扩展祖先链忽略规则与深度过滤 (P2)**：在 `shouldTranslateElement` 中使用 `closest` 沿祖先链深度递归检测 `.notranslate`、`data-no-translate` 与忽略标签，防止嵌套节点被误翻。
- **批量翻译数量一致性与输入尺寸统一限制 (P2)**：验证引擎批量返回数量必须与请求完全一致，避免残缺空数据写回；为桌面 IPC 与 OCR 接口补充严格的长度与体积上限校验。

### 修复
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
- **扩展 Origin 专属白名单与 ID 格式校验**：在本地 HTTP 服务中实现 `is_allowed_origin` 校验器，严格限制 Chrome 扩展为标准字母数字 ID 格式（`chrome-extension://[a-z0-9]+`）、Firefox 扩展为合法 UUID（`moz-extension://[a-zA-Z0-9-]+`），生产环境拒绝空扩展名与非法伪装域名。
- **密码学安全随机 Token 生成**：使用 `rand::rngs::OsRng` 生成 32 字节 (256-bit) 强随机安全令牌，替换原本的时间戳+PID 拼接方式，彻底消除被探测伪造风险。
- **强化 Tauri 应用程序 CSP 防线**：在 `tauri.conf.json` 中配置严格的 Content Security Policy (`default-src 'self'; script-src 'self'; connect-src 'self' ...`)，阻断潜在的 XSS 攻击。
- **CI 引入 Gitleaks 密钥泄露扫描**：在 `.github/workflows/ci.yml` 中配置 Gitleaks 自动化扫描步骤，确保代码库无任何明文 Key 泄漏。

### 优化
- **术语库正则表达式预编译缓存**：在 `Glossary::insert` 时完成 ASCII 单词边界正则的预编译并缓存在 `TermEntry` 中，消除每次文本翻译时的重复动态编译开销。
- **DeepL 批量请求语言对同构性校验**：在 `DeepLEngine::translate_batch` 中增加全量语言一致性判定，在请求混杂多语种时安全降级为逐项翻译，避免取首项导致的批量乱译。
- **悬浮小窗文案全面国际化**：补齐浮窗所有静态文本的 i18n 多语言字典支持，消除硬编码。
- **纳入 Cargo.lock 并完善排除规则**：按应用类型仓库规范取消对 `Cargo.lock` 的忽略，确保跨平台与 CI 重现构建的一致性；明确排除 `extension/dist/`。
- **规范项目 AGENTS.md 准则**：将项目的核心分层规则、平台边界与安全底线沉淀至根目录 `AGENTS.md`。

### CI/CD
- **全平台自动化打包与 Release 流水线**：新增 `.github/workflows/release.yml`，在推送 `v*` 语义化标签时，利用 `tauri-apps/tauri-action` 矩阵自动化构建 Windows (`.exe` / `.msi`)、macOS (`.dmg`)、Linux (`.deb` / `.AppImage`) 安装包，并自动编译打包浏览器扩展 `.zip` 挂载至 GitHub Release 页面。

### 新增
- **轻量划词悬浮浮窗（Mini Floating Window）**：在 Tauri 窗口架构中建立独立的 `float` 迷你无边框置顶气泡小窗（`380x240`，磨砂半透明阴影），按下 `Alt+Shift+D` 划词时仅唤起轻量悬浮窗展示极简译文并支持一键复制，失去焦点（失焦）自动智能隐藏收起，不遮挡阅读视线。
- **系统剪贴板监听自动翻译（可选常驻模式）**：新增剪贴板后台动态检测调度器与 `set_clipboard_monitor` 命令；在设置中心增加常驻开关，开启后用户复制外文时自动弹出迷你悬浮窗展现译文。
- **快捷键触发自动模拟复制选中文本**：利用 `enigo` 在按下 `Alt+Shift+D` 全局快捷键时，自动模拟系统级 `Ctrl+C` (Windows/Linux) 或 `Cmd+C` (macOS) 按键事件将屏幕选区内容拷入剪贴板，随后无缝唤醒主窗口并即时翻译，无需用户预先手动复制。
- **系统原生轻量 OCR 引擎抽象与命令支持**：在 `transly-core` 中定义 `OcrEngine` trait 与 `SystemOcrEngine` 原生轻量实现；在桌面端暴露 `ocr_image` 强类型命令与 Base64 解码管道，为多模态识图翻译奠定架构基础。
- **术语库 SQLite 持久化存储与可视化管理**：在 `TranslationCache` 中建立独立 `glossary` 表，桌面端启动时自动回载自定义术语库规则；通过 Specta 暴露出 `get_glossary_terms` 与 `delete_glossary_term` 强类型接口，并在前端设置中心提供术语条目的实时增删与持久化列表。
- **全局快捷键划词取词与即时翻译唤醒**：接入 `tauri-plugin-global-shortcut` 与 `tauri-plugin-clipboard-manager`，用户选中文本按下全局快捷键（`Alt+Shift+D`）即可自动从系统剪贴板读取最新选区文本、唤醒主窗口并即时完成翻译展示。

### 优化
- **Google 网页翻译长文本分块防溢出**：对单次传入超过 1200 字符的长段落自动实施换行/分段切片合并，彻底规避 GET URL 超长（HTTP 414 URI Too Long）与触发接口风控阻断。
- **术语库单词边界精准匹配**：在 `Glossary::apply` 中引入正规表达式单词边界识别（`\b(term)\b`），对英文 ASCII 单词强制边界匹配，彻底杜绝例如 `IT` 误替换 `limit` / `Wait` 的子串误伤问题。
- **LLM 批量翻译受控并发加速**：在 `OpenAIEngine::translate_batch` 引入 `futures::future::join_all` 受控分批并发处理（并发度 4），大幅缩短浏览器整页多节点批量翻译响应耗时。
- **SQLite 缓存大表容量修剪算法优化**：重构 `trim_to_max_items` 淘汰算法，使用基于时间游标与 rowid 偏移的高性能删除替代耗时的 `NOT IN` 全表子查询。

### 优化
- **全局引擎连接池复用 (EnginePool)**：在 `src-tauri` 中引入 `EnginePool`，缓存已初始化的长连接实例并提供读写锁，杜绝单次翻译重复握手与新建网络连接；密钥更新时自动使对应实例失效。
- **本地 HTTP 服务与浏览器插件引擎传参支持**：`TranslateBatchRequest` 支持 `engine` 字段，浏览器扩展 Popup 支持引擎选择并持久化，整页翻译与增量监听按指定引擎流转。
- **前端国际化 (i18n) 资源拆分与中英切换**：创建 `src/i18n/` 模块，将硬编码中文文案抽取至 `zh-CN.json` 与 `en.json`，并支持主界面顶部一键切换中英文。

### 新增
- **接入 DeepL 与 OpenAI / DeepSeek 高质量翻译引擎**：在 `transly-core` 中新增 `DeepLEngine`（支持 Free 与 Pro 接口、语言代码标准化）与 `OpenAIEngine`（兼容 OpenAI 规范与 DeepSeek 专用预设）；实现动态安全解析与 `FallbackEngine` 容灾回退。
- **桌面端设置中心与多引擎交互面板**：前端支持设置弹窗，可直接管理 DeepL / DeepSeek 密钥安全存入系统原生凭据库（非明文落盘）；支持用户动态切换翻译引擎与添加全局术语库规则。
- **IPC Specta 强类型契约同步**：更新 `commands::translate` 接口并重新导出生成 `bindings.ts`，确保前端引擎选择与后端执行一致。

### 优化
- **SQLite 缓存自动衰减与容量保护**：在 `transly-core` 中实现 `cleanup_older_than_days` 与 `trim_to_max_items` 淘汰策略，并在桌面端启动时自动执行 90 天过期与 100,000 条上限维护，防止本地数据库无序膨胀。

### 新增
- **术语库深度接入翻译流水线**：在 `transly-core` 与 `src-tauri` IPC / 本地 HTTP 批量翻译端注入全局 `Glossary` 术语库，实现译文自动匹配与精准纠正；支持桌面命令动态维护术语。
- **主备引擎降级容灾机制**：实现 `FallbackEngine` 包装器，支持在主引擎不可用或网络异常时自动切换备用引擎，提升服务可用性。
- **前端规范与自动化校验闭环**：配置最新 ESLint Flat Config，将 `pnpm run lint` 整合至 `package.json`、`lefthook` 预提交钩子与 GitHub Actions CI 流水线。

### 安全
- **本地服务 CORS 来源策略加固**：收紧本地 HTTP 服务的 CORS 策略，严格限制仅允许浏览器扩展（`chrome-extension://`、`moz-extension://`）与本地开发回环请求，杜绝任意第三方恶意网页探测。

### 优化
- **系统托盘与后台常驻机制**：Tauri 2 原生集成托盘图标、左键单击激活、右键上下文菜单（“显示主窗口” / “退出 Transly”）；拦截主窗口关闭事件转为后台隐藏，确保本地 HTTP 翻译服务不中断。
- **浏览器插件 MutationObserver 动态增量翻译**：首屏整页翻译完成后，自动挂载 DOM 变动监听器，针对 SPA 单页应用及瀑布流加载内容执行防抖增量收集与原地无感翻译；还原原文时自动注销监听。

### 优化
- **引擎实例与连接池复用**：将 `TranslateEngine` 注入 Tauri 全局状态，消除重复创建 `reqwest::Client` 的握手损耗。
- **插件 Token 配对闭环**：桌面端支持一键复制插件配对 Token，插件 Popup 支持持久化保存 Token 与实时连通性探测。
- **语言检测增强**：扩展中（CJK）、日（平/片假名）、韩（Hangul）、英多语言特征字符判定。
- **CI 契约同步校验**：在 GitHub Actions CI 中增加 Specta 自动绑定代码比对检查（`git diff --exit-code src/bindings.ts`）。

### 新增
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
