# 更新日志

本项目的所有重要变更均记录于此文件。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
并且本项目严格遵循 [语义化版本 2.0.0](https://semver.org/lang/zh-CN/)。

## [0.5.0] - 2026-10-20

### 优化与增强
- **文档翻译全链路流式进度广播与主动打断取消机制 (`document/mod.rs`, `commands/document.rs`, `DocumentPanel.tsx`, `bindings.ts`)**：
  - **实时分片进度百分比通知**：在 `transly-core` 底层引入 `DocumentProgressCallback`，切批翻译中每完成一个 Batch 即刻向前端广播 `document-translate-progress` 事件，前端 `DocumentPanel` 实时呈现平滑进度条、百分比与已完成/总块数，彻底告别大文档翻译“漫长盲等”的假死体验；
  - **任务取消与快速打断**：底层接入取消信号监听通道，单批循环前置检查取消标志；前端提供“取消翻译”按钮并支持 `Esc` 快捷键极速取消，瞬时中断在途网络请求并停止后续大模型 Token 消耗；
  - **状态聚合与 specta 签名瘦身**：将 `commands/document.rs` 内部庞大的系统依赖聚合为 `TranslationDependencies` 注入，规避 `specta` 最多 10 个参数的硬限制，大幅提高后端架构内聚度。
- **架构重构与单测解耦瘦身 (`commands/mod.rs`, `commands/tests.rs`)**：
  - 将 `src-tauri/src/commands/mod.rs` 中逾 400 行庞大的单元测试与语义测试整洁抽离至 `commands/tests.rs`，消除主入口文件过度膨胀，大幅改善代码可读性与团队协作维护效率。
- **SQLite 历史缓存降序索引优化 (`cache.rs`)**：
  - 将 `translation_cache` 的索引优化为显式降序 `idx_cache_created_at ON translation_cache(created_at DESC)`，使 `list_recent`、`search` 以及后台定时修剪 `trim_to_max_items` 直接利用 B-tree 覆盖扫描，消除海量记录下的临时排序表开销。
- **macOS 平台划词替换辅助功能权限预检与诊断 (`selection.rs`)**：
  - 在 macOS 键盘模拟流程中前置探测 `AXIsProcessTrusted()` 辅助功能权限，未授权时向前端抛出友好告警与系统设置引导，杜绝用户因权限缺失导致模拟按键静默失败、误以为功能损坏。
- **浏览器扩展网络重试随机抖动 (Jitter) 防惊群 (`background/index.ts`)**：
  - 将扩展后台单次固定 400ms 重试升级为 250ms ~ 600ms 动态随机抖动退避，有效消除整页多批次并发在桌面端重启或休眠唤醒时同时重试引发的惊群效应。
- **截图覆盖工具栏响应式与极小选区安全适配 (`ScreenshotOverlay.tsx`)**：
  - 为跟随悬浮工具栏增加 `max-w-[calc(100vw-16px)]` 与单行防溢出适配，防止在低分辨率显示器或极窄选区下产生横向截断。
- **截图翻译全链路缺陷修复与真机体验优化 (`windows.rs`, `ScreenshotOverlay.tsx`, `screenshot.rs`, `ocr/mod.rs`, `screenshotLayout.ts`)**：
  - **选区智能跟随悬浮工具栏 (对标有道翻译体验，`ScreenshotOverlay.tsx`, `screenshotLayout.ts`)**：工具栏由死板固定在屏幕正上方，升级为智能跟随框选区域。默认紧贴选区右下方，靠近屏幕底部时自适应翻转至上方，全屏或贴边时智能安全内贴，彻底根治 2K/4K 大屏上框选后视线与鼠标需大幅跨屏位移的严重操作痛点；
  - **空格键 (`Space`) 极速切换原译文 (`ScreenshotOverlay.tsx`)**：按 `Space` 单键即刻在“原文”与“译文”之间切换对比，配合按键徽标直观指引，实现单手键盘流对比；
  - **新增一键「重新框选」入口 (`ScreenshotOverlay.tsx`)**：跟随工具栏集成一键重新框选按钮，点击后清空选区与文字块并恢复待选状态，无需手动退出重开；
  - **修复 Windows OCR 候选语言短路丢失中文缺陷 (`windows.rs`)**：WinRT OCR 候选语言遍历时，当首个引擎（`en-US`）在中文图像上识别返回空结果时，不再提前 `return Ok(words)`，而是自适应向后尝试 `system` 与 `zh-Hans-CN` 语言包，彻底根治截中文报“未能从图片中识别到有效文字”的高危缺陷；
  - **扩展多语言 OCR 候选矩阵 (`ocr/mod.rs`)**：在 `build_candidate_languages` 中扩充日文 (`ja-JP`) 与韩文 (`ko-KR`) 候选配置，大幅提升异种语言画面的文字召回率；
  - **修复译文块点击冒泡销毁画板缺陷 (`ScreenshotOverlay.tsx`)**：为译文块容器添加 `onPointerDown={(e) => e.stopPropagation()}`，彻底杜绝用户点击或选中译文时误触底层拖拽画布导致已有翻译瞬间清空的问题；
  - **新增鼠标右键一键取消退出截图 (`ScreenshotOverlay.tsx`)**：全屏画布与文字块任意位置单击鼠标右键立即取消并安全收起覆盖窗，对齐主流桌面截图工具交互习惯；
  - **单块快速复制与双击即拷交互 (`ScreenshotOverlay.tsx`)**：文字块支持双击快速复制、悬停便捷复制按钮以及绿色反馈标记，支持文字划选 (`select-text`)，免去只允许全局拼接复制的痛点；
  - **自适应悬停展开与排版优化 (`ScreenshotOverlay.tsx`)**：为译文块增加 `break-words` 断词保护，小框长文本悬停时提权使用 `hover:!h-auto` 配合行内 `minHeight` 自适应垂直展开完整文本，同时彻底杜绝了因 `hover:min-h-full` 误将视口当作 100% 导致悬停瞬间拉伸撑满整屏的样式缺陷；
  - **截图会话联动持久化首选目标语言偏好 (`commands/screenshot.rs`)**：`load_prefs` 优先读取用户在主窗口保存的 `preferred_target_lang` 偏好并严格排除非法 `auto` 值，告别每次截图强制回退中文的体验断层。
- **全方位使用体验与交互质量提升 (`App.tsx`, `HistoryPanel.tsx`, `DocumentPanel.tsx`, `FloatWindow.tsx`, `speech.ts`)**：
  - **TTS 语音朗读交互与抗 GC 假死加固 (`speech.ts`, `App.tsx`, `FloatWindow.tsx`)**：
    - 朗读期间发音按钮展示动态高亮波形与“停止朗读”状态指示，再次点击立即打断当前发音；
    - 在模块作用域持有 `activeUtterance` 全局强引用并在终态释放，彻底消除 Chromium / WebKit 垃圾回收提前销毁 Utterance 导致 `onend` 静默丢失、界面假死停在“播放中”的缺陷；
    - 每次触发新发音前显式先调用 `stopSpeaking()`，杜绝音频队列堆叠与串音。
  - **翻译历史防误删与检索体验重塑 (`HistoryPanel.tsx`)**：
    - **删除二次确认机制**：点击删除后原地展开高辨识度确认与取消按钮，待确认状态下保持长显，解决鼠标滑离被容器隐藏的问题；配套 `cancelDelete` 闭环重置与 3.5s 超时自动恢复，彻底根治手抖误删记录灾难；
    - **搜索关键词安全高亮**：实现正则表达式安全转义的 `HighlightMatch` 组件，在原文与译文中精准标出检索匹配词，规范化使用 `rounded-sm`；
    - **一键清空搜索词**：搜索输入框动态呈现清除图标，点击瞬时复原完整历史列表。
  - **设置中心可观察性与录入提效 (`App.tsx`)**：
    - **API Key 明密文显示切换**：DeepL、DeepSeek、OpenAI 密码框右侧集成眼睛（`Eye` / `EyeOff`）图标，补充 `autoComplete="off" spellCheck={false}`，方便直观核对长密钥并兼顾表单安全；
    - **实时连通性「测试连接」**：在各外部翻译引擎和网络代理配置旁增设测试按钮；测试前自动同步持久化输入框最新配置，测试成功后即时检索并删除测试条目（`Hello`），实现**零脏数据落盘**并就地反馈真实延迟与状态；增加 `isMountedRef` 避免弹窗关闭后的异步状态告警；
    - **术语库回车快捷添加与 IME 保护**：输入源词与译词后支持直接按 `Enter` 录入并聚焦，增加 `e.nativeEvent.isComposing` 状态判断，彻底防止中文/日文拼音选词回车误添加；过滤无匹配时呈现居中空状态。
  - **文档翻译双栏并排对照模式 (`DocumentPanel.tsx`)**：
    - 预览区域新增“单栏查看 / 双栏对照”模式切换，原文与译文可分栏独立滚动与字数对照，显著提升长篇文档校对审阅效率。
  - **主翻译工作区细节打磨 (`App.tsx`)**：
    - 输入框聚焦时支持按 `Esc` 键极速清空；字符数超过 4000 时以琥珀色高亮提示，辅助感知大文本拆批风险。
  - **多语言国际化 100% 对称 (`zh-CN.json`, `en.json`)**：
    - 补齐所有新增功能的中英文文案与插值占位符，保持双语字典结构严格对齐。
- **交互与使用体验全面深度优化 (`App.tsx`, `HistoryPanel.tsx`, `DocumentPanel.tsx`, `FloatWindow.tsx`)**：
  - **双向语言快速对调与一键反向对译 (`App.tsx`)**：输入栏箭头升级为交互式对调语言按钮，译文卡片新增「作为原文反向对译」按钮，一键将当前译文转为原文并反转目标语言立即发起翻译，极大提升润色与双语核对效率；
  - **全界面快捷键流体验 (`App.tsx`)**：主输入框支持 `Ctrl+Enter` / `Cmd+Enter` 立即触发翻译，与文档翻译面板操作逻辑保持统一；
  - **历史记录一键导入翻译框 (`HistoryPanel.tsx`)**：历史记录卡片悬浮新增一键导入按钮，点击后自动填入主输入框、同步目标语言并切换至翻译 Tab 立即运行，免去手动复制粘贴的繁琐步骤；
  - **全界面暗黑模式 (Dark Mode) 对比度与质感细节统一**：
    - 主窗口顶层非活动 Tab 补齐 `dark:bg-slate-800 dark:text-slate-400 dark:hover:bg-slate-700`，根除深色模式下 Tab 呈现刺眼浅灰块的割裂感；
    - 历史记录卡片补齐 `dark:border-slate-800 dark:bg-slate-800/40` 与 `dark:text-slate-400`；
    - 迷你悬浮窗底部朗读与复制按钮补齐深色主题背景与文字颜色，杜绝暗黑模式下的反白眩光；
    - 译文卡片边框、重试按钮、错误警告条与设置中心凭据标签全量完成暗黑模式颜色规范化；
    - 截图覆盖窗单块重试按钮补充暗黑模式高对比度样式。
  - **文档面板与设置中心国际化纯净化 (`DocumentPanel.tsx`, `zh-CN.json`, `en.json`)**：
    - 清除文档面板中「重新选择」、「未翻译」、「点击上方“开始翻译”按钮生成译文」以及字符单位等硬编码中文；
    - 清除设置中心离线引擎状态「● 就绪」、「● 模型已装载」等硬编码，中英文多语言环境均 100% 优雅呈现。

### 新增
- **文档与字幕文件翻译 (`transly-core/src/document/`, `commands/document.rs`, `DocumentPanel.tsx`)**：
  - 核心解析与保真重构：纯 Rust 实现 `.txt`（段落切分与空行定界保真）、`.md`（YAML Frontmatter 免翻保护、代码块/公式占位保护）、`.srt`（序号行与时间轴 100% 锁定保真，仅回填字幕译文）、`.json`（递归提取叶子值翻译，严格保护 Key 键名与嵌套结构）、`.html`（仅提取正文节点，100% 保留 HTML 标签、属性与代码）；
  - 批量流水线切批调度：复用 `plan_batch_ranges` 与 `translate_batch_with_cache`，严格满足 `MAX_BATCH_ITEMS`（30）与 `MAX_BATCH_TOTAL_CHARS`（50,000）上限；单项错误局部降级回退至原文，绝不损坏文档结构；
  - 桌面端全新「文档翻译」工作台：支持拖拽上传与文件选择、格式与尺寸自动识别、翻译进度与耗时感知、双栏/Tab 原文与译文对照预览，支持一键复制与一键流式导出对应格式译文文件。
- **浏览器扩展体验深化：网页输入框原地替换助手与精细化站点规则 (`extension/`)**：
  - **输入框原地翻译替换 (`selection/index.ts`, `background/index.ts`)**：支持通过快捷键 `Alt+Shift+R` 或右键「Transly 原地翻译替换」，将网页内 `input` 与 `textarea` 中选中的文本或完整内容原地翻译为目标语言；采用原型属性 setter 绕过 React/Vue 受控属性劫持并派发原生 `input`/`change` 事件，确保表单与框架状态机 100% 同步；
  - **站点级规则精细化配置 (`popup/index.ts`, `siteRules.ts`, `content/index.ts`)**：Popup 扩展折叠式站点规则卡片，支持为特定域名单独配置排除 CSS 选择器（如 `.code, pre, .comments`，防爆语法校验）、站点专属目标语言绑定（如特定外文站点固定直译），以及站点级输入助手禁用开关；
  - **构建产物 IIFE 隔离与防重声明 (`build.mjs`)**：将非 background 脚本（`content`、`selection`、`popup`）构建格式收口为独立 `iife` 并命名空间隔离，消除多次执行脚本时的顶层作用域标识符冲突。
- **嵌入式离线混合翻译引擎与本地模型扩展 (`transly-core/src/engine/offline/`, `src-tauri/`)**：
  - **内置高频与开发词库矩阵 (`lexicon.rs`)**：重构 `OfflineEngine` 为混合离线架构，内置百余项覆盖计算机、软件工程、界面交互、礼貌用语与高频词汇的 Aho-Corasick 双向自动机；支持 CJK 与西文词边界严格校验（防止词内误伤）及中英文标点智能双向对转；
  - **本地模型自动探测与扩展契约 (`model.rs`, `storage.rs`)**：自动扫描 `<app_data_dir>/models/` 目录，探测并自适应支持 `.onnx` / `.bin` / `.gguf` 外部离线模型文件，在未放置外部模型时安全降级为内置词库模式，绝不崩溃报错；
  - **设置中心离线引擎状态面板 (`App.tsx`)**：设置对话框新增「纯本地离线引擎与模型」卡片，实时展示内置词库就绪状态、外部模型装载状态以及本地模型扫描目录路径。
- **跨平台系统能力加固：Linux 桌面环境上下文感知与退化屏幕几何守护 (`geom.rs`, `screenshot.rs`, `screencap/linux.rs`)**：
  - **Linux Wayland 桌面环境上下文提示**：在检测到 Wayland 时提取 `XDG_CURRENT_DESKTOP`（如 GNOME / KDE Plasma / Sway / Hyprland），向用户呈现带有桌面环境标识的精准降级指引；
  - **Linux OCR 缺失前置报错与指引**：在未安装系统级 `tesseract` 工具时，能力探测接口前置置 `supported: false` 并展示确切的 apt 安装命令，禁用截图按钮并悬浮提示，杜绝用户点击后才抛出截屏失败；
  - **显示器几何退化尺寸守护 (`find_monitor_for_point`)**：增加对无效/休眠退化显示器尺寸（宽或高为 0）的拦截，防止多屏断开瞬间的伪矩形导致光标映射异常。

### 修复
- **截图翻译端到端真机交互与识别聚合全链路缺陷修复**：
  - **Windows OCR 4K/大图尺寸下采样后坐标严重缩小偏移 (`windows.rs`)**：WinRT OCR 在图片超过 2600 像素执行下采样后，词框像素坐标未按比例还原回原图空间，导致 4K 屏幕上 OCR 框整体缩至 67.7% 并向左上角严重偏移。现返回前统一按缩放比例还原，保证与原图像素契约严格对齐；
  - **CJK 语义单字（如“门”、“开”）被误杀过滤 (`blocks.rs`)**：`filter_blocks` 调整单字符过滤条件，明确区分西文字母与 CJK 表意字符，保留合法的单个汉字/假名/谚文 OCR 块，彻底解决单字截图报“未能识别到有效文字”的问题；
  - **高分屏/DPI缩放屏幕上译文字号算大导致截断 (`ScreenshotOverlay.tsx`)**：`fitFontSize` 尺寸计算由物理像素改用视口 CSS 逻辑像素，解决 Retina 与 Windows 150% 缩放下译文因过大而被 `overflow-hidden` 裁切丢失的问题；
  - **单块重试按钮冒泡导致所有文字块被清空销毁 (`ScreenshotOverlay.tsx`)**：重试按钮增加 `pointerdown` 阻止冒泡，并优化画板非拖拽点击时不误清空已有 blocks；
  - **还原原文模式下复制与发送到主窗口失效 (`ScreenshotOverlay.tsx`)**：`getSelectedText` 响应 `showTranslation` 状态，并在翻译失败或原文查看模式下正确提取 OCR 原文，避免发送空字符串；
  - **Linux 与 macOS OCR 语言映射缺失中英双向支持 (`tsv.rs`, `macos.rs`)**：修正 Tesseract 与 Vision 语言映射参数，中英双向场景统一加载双语模型，解决目标语言为英文时截图中文完全无法识别（乱码输出为 "PICA"）的严重缺陷；
  - **水平多栏排版同行横向串联 (`blocks.rs`)**：引入 `LINE_MERGE_MAX_X_GAP_RATIO`（2.5），在词聚合成行时按水平间距自动断开独立分栏片段，保留分栏排版语序；
  - **主窗口截图与剪贴板识别按钮暗黑模式样式补齐 (`App.tsx`)**：补充 `dark:border-slate-700 dark:bg-slate-800 dark:text-slate-300` 样式。
- **文档与文案诚实性校正 (`README.md`, `zh-CN.json`, `en.json`, `docs/superpowers/plans/`)**：
  - 移除 macOS 归档包「应用内自动更新用」的错误描述（项目尚未接入 `tauri-plugin-updater`）；
  - 离线模型相关文案由「已加载 / 已装载」校正为「检测到模型文件」，如实反映当前版本仅扫描模型文件、不执行推理的能力边界；
  - 为 4 份已交付的计划文档补充状态说明，避免复选框全部未勾选造成进度误判。


## [0.4.1] - 2026-10-18

### 优化与增强
- **批量流水线单事务原子写入优化 (`batch_pipeline.rs`)**：`translate_batch_with_cache` 将原本在循环中逐条单事务写入缓存的模式，重构为收集全部成功项后单次调用 `cache.set_batch` 原子批量写入，极大降低磁盘 I/O 争用与锁开销；同时移除 `translate_screenshot_blocks` 中的 `spawn_blocking + block_on` 嵌套反模式，直接异步执行。
- **划词原地替换首选目标语言配置与跨端联动 (`selection.rs`)**：新增 `get_preferred_target_lang` 与 `save_preferred_target_lang` IPC 命令，划词原地替换（`Alt+Shift+R`）自动遵循用户在主窗口或设置中持久化的目标语言偏好，不再硬编码为中文。
- **截图覆盖窗重选区域目标语言动态同步 (`overlay/mod.rs`, `commands/screenshot.rs`, `ScreenshotOverlay.tsx`)**：`recognize_screenshot_selection` 支持接收当前界面目标语言参数并同步更新会话内部状态，解决用户在覆盖窗切换目标语言后重新框选时 OCR 语言提示回退至初始中文的问题。
- **浏览器扩展富文本倒装语序容错与优雅降级 (`richText.ts`, `content/index.ts`)**：新增 `cleanCompositeText` 辅助函数；在双语对照模式下，针对日文/德文等倒装语序导致占位符顺序打乱的段落，优雅降级为在复合段落末尾追加完整双语译文，既维持了原地替换模式下不破坏 DOM 物理结构的 fail-closed 防线，又避免了双语对照模式下大面积弃翻报错。
- **悬浮窗与截图覆盖窗补齐完整语言列表 (`FloatWindow.tsx`, `ScreenshotOverlay.tsx`)**：将悬浮窗与截图覆盖窗的语言下拉选项由原本写死的 3 种扩充为与主窗口对齐的 6 种主流语言（`zh-CN`, `en`, `ja`, `ko`, `fr`, `de`）。
- **主窗口当前引擎与目标语言持久化 (`App.tsx`)**：在 `localStorage` 中持久化 `transly_main_target_lang` 与 `transly_main_engine`，并在切换时自动同步后端 `savePreferredTargetLang`，重启应用无需重新手动选择。
- **系统原生托盘菜单文案支持国际化 (`src-tauri/src/main.rs`)**：构建系统托盘时自动读取当前语言偏好，当用户偏好为英文环境时，托盘菜单项与 Tooltip 自适应呈现英文文案。

### 修复
- **全局快捷键重复注销导致静默抢占他人按键缺陷 (`commands/mod.rs`)**：`set_shortcut` 与 `set_replace_shortcut` 移除 `if old_shortcut != clean { let _ = app.global_shortcut().unregister(clean); }`，严格遵循 `LESSONS.md` 第 24 条教训，仅注销自身旧键 `old_shortcut`；当目标新键已被其他功能或系统占用时，如实让 `on_shortcut` 抛出 `AlreadyRegistered` 错误并回滚通知用户，杜绝静默抢占。
- **大模型流式翻译打断取消时占位符未还原泄漏至界面 (`useTaskRunner.ts`, `outcome.ts`)**：新增 `sanitizeCancelledStreamText` 纯函数，对流式打字中途取消（`cancel()`）时的残缺文本进行安全清洗，将内部专有括线占位符（如 `⟦TRANSLY_CODE_0⟧`）替换为友好的 `[代码]` 提示并剔除残缺标记，彻底消除内部标记泄露；新增单元测试覆盖。
- **框选截图翻译（三平台）**：`Alt+Shift+S` / 托盘「截图翻译」/ 主窗口按钮三入口触发，屏幕冻结为覆盖窗后拖拽框选任意区域，自动 OCR 识别并**按原坐标叠加译文**（保排版），可一键「还原原文」；覆盖窗内可直接切换引擎与目标语言即时重译、复制全部译文、把识别/翻译结果发送到主窗口精修重译。
  - **三平台自研屏幕捕获**（统一入口 `capture_monitor`）：Windows GDI `BitBlt`、macOS `CGDisplayCreateImage`、Linux X11 `xcb`。**刻意不引入 `xcap`** —— 其 Linux 依赖链强制拉入 pipewire/libspa（非 optional），而 `libspa 0.10.1` 无条件引用 pipewire 1.0 才引入的字段，在 Ubuntu 22.04 上编译直接失败；这些依赖只服务于 Wayland，而 Wayland 本就不在支持范围内。
  - **三平台 OCR**（统一入口 `recognize_words`）：Windows `Windows.Media.Ocr`、macOS Vision、Linux 外呼 `tesseract`（`- - tsv` 管道，全流程内存零落盘）。
  - **归一化坐标契约 `NormRect`**（原点左上，分量 ∈ `[0,1]`）为全链路唯一坐标语言：截图的物理像素与覆盖窗的 CSS 逻辑像素在任何 DPI 下都不相等，归一化后前端只需 `left: x*100%`，全链路零缩放系数，杜绝混合 DPI 下的漂移。
  - **冻结帧走自定义 URI 协议 `translyshot`**，绝不走 IPC：4K 帧约 33 MB，走 IPC 需 base64 再 `JSON.parse`，JS 堆峰值可达图像 3 倍；`session_id` 不匹配即 404，防陈旧窗口读到新会话图。
  - **降级探测**：`get_screenshot_capability` 提前告知环境是否支持（Wayland / 缺 tesseract），不支持时按钮禁用并显示**可执行**的原因，而不是让用户按了才发现不能用。
  - **新增第三个全局快捷键** `Alt+Shift+S`（支持录制与冲突检测）；`ShortcutStatusSlot` 扩展为三个独立槽位，任一快捷键冲突只影响对应设置卡片。
  - **Linux 剪贴板图片 OCR 顺带可用**：原先非 Windows 平台返回 `unsupported_platform`，而该文案在 Linux 上是「请改用『识别剪贴板图片』」——若剪贴板路径本身不可用，这句指引就是错的。现三平台统一走同一套 OCR 实现。

### 安全
- **升级 `rustls` 至 0.23.46 修复 4 项 TLS 层已知漏洞**：依赖审计（`cargo audit`）暴露 `rustls 0.23.45` 命中 RUSTSEC-2026-0346（客户端会话 `Debug` 实现泄漏 TLS 密钥，中危）、RUSTSEC-2026-0347（ECH 拒绝后缓冲的应用数据仍发往外层服务器）、RUSTSEC-2026-0348（Brotli 证书解压在认证完成前最多可分配 1 GiB）、RUSTSEC-2026-0349（未认证的 ServerHello 可将后续连接降级到经典密钥交换）。四者均为 reqwest 传输路径（`hyper-rustls` / `tokio-rustls`）的上游依赖，翻译请求与本地服务均受其影响。升到 0.23.46 后 `cargo audit` 漏洞数归零（仅剩 3 条 unmaintained/unsound 警告，暂不影响）。

### 修复
- **配对全局限流的稳态容纳量超过会话表容量，可被持续请求占满（配对拒绝服务）**：全局限流只约束了瞬时突发（10 秒窗口内 3 个新会话），未约束「有耐心的攻击者」跨窗口的持续速率。长期稳态下同时在世的会话数趋近 `burst / window × TTL` = 3/10×60 = **18**，而 `MAX_PENDING_SESSIONS` 仅 16 —— 攻击者按窗口节奏稳定请求约 **50 秒**即可占满全部槽位并长期保持，此后正常浏览器扩展的每次配对都收到 429。原注释声称「灌满 16 个会话至少需要约 50 秒，其间早先的会话已过期」并不成立：首个会话要到 60 秒才过期，50 秒时容量已打满。现有测试只覆盖同一窗口内的密集请求，未覆盖跨窗口的持续请求。现将窗口调整为 15 秒（稳态 12 < 16，对真实用户与 10 秒窗口无感知差别），并新增守护该不变式的常量算术断言 `test_pairing_global_throttle_steady_state_below_capacity`；该测试在旧参数下实测 FAILED（报出 18 > 16），修正后通过。
- **历史导出 CSV 时间戳错乱，debug 构建直接 panic（数据损坏）**：`format_timestamp_iso` 手写民用历法算法时把闰年修正除数抄错——正确值是 `1460 / 36524 / 146096`（4 年一闰、100 年不闰、400 年再闰），实际写成了 `1024 / 1461 / 142401`（后者与 `146097/4` 形近，属典型手抄串行）。后果分两档：`yoe` 偏大 1 后 `doy = doe - (365*yoe + yoe/4 - yoe/100)` 触发 u32 下溢，debug/测试构建在 1970–2037 年间共 **1672 天**直接 panic；release 构建则产出 `1970-28071671-3435973839T00:00:00Z` 这类畸形字段，**1674 天**的日期错误，用户导出 CSV 后按时间排序与归档全部错乱。原测试只断言了 `format_timestamp_iso(0)` 与 `(-100)`，这两点恰好是错误实现下也正确的值，属 LESSONS 第 11 条所述的「迎合实现的假测试」。现修正除数并补两个回归测试：表驱动真实日期对照（期望值由独立可信来源生成，含闰世纪与普通闰年 2 月）与连续 60 年逐日扫描。按 LESSONS 第 11 条要求，已将实现临时改回缺陷版确认两个测试均 FAILED（命中 `cache.rs:916` 下溢 panic）后再恢复。
- **扩展「重试失败项」丢弃 `compositeInfo`，导致占位符泄漏与段落破版**：`retryFailedTranslations` 用解构 `({ node, text, id })` 重建条目，把富文本段落赖以还原的 `compositeInfo` 丢掉了。富文本失败项的 `text` 是含 `⟦T_i⟧` 占位符的**复合文本**而非用户可见原文，重试时因 `compositeInfo` 缺失而走普通写回分支，把整段译文连同未还原的占位符一起塞进该段落第一个文本节点，而 `<a>`/`<strong>` 内的原文文本节点保持不动——用户看到「Unicode 私用括号占位符泄漏 + 整句译文与残留原文碎片混排」。首轮翻译路径不受影响（它持有完整条目），属重试路径独有缺陷。现改为整体浅拷贝保留全部字段。
- **扩展 E2E 在真实服务不可用时静默降级为 Mock 桩，仍报「真实环境 100% 通过」**：`test-extension-e2e.mjs` 找不到或拉不起 `transly_test_server` 时会回退到 Mock 桩，而结尾横幅无论走哪条路径都打印「全部 9 项浏览器插件真实环境端到端测试 100% 通过」。该路径下真实 Rust 服务的 Bearer 鉴权、CORS 白名单与切批校验完全未被执行，报告却与真实通过毫无区别——CI 已先编译真实服务，一旦降级即为异常，属假绿风险。现按实际后端来源拼接横幅（「真实环境」/「Mock 桩降级」）并在降级时用 `console.warn` 显著告警。
- **划词原地替换在无选区时误粘贴旧剪贴板（数据破坏）**：`poll_clipboard_change` 的超时兜底会返回「最后一次读到的非空文本」，而该函数被只读的划词浮窗与**会模拟 Ctrl+V 覆盖光标处**的原地替换共用。未选中文本时误触 `Alt+Shift+R`，几分钟前复制的旧内容会被当作选区文本翻译并强行粘贴，改写用户文档。现引入 `UnchangedFallback::{ReturnLastSeen, Abort}`，破坏性写路径一律 fail-closed。
- **术语库双向词条反转改写正确译文**：`Glossary::apply` 方向无关，源端预替换与译文端后替换各执行一次时，若词库同时存在 `apple→苹果` 与 `苹果→Apple`，英译中会产出 `我喜欢Apple`。新增 `Glossary::apply_to_translation`，在译文端跳过「已是某术语目标词」的片段；源端 4 处仍用 `apply`，译文端 7 处改用 `apply_to_translation`；判定依据是「命中词条自身的源词是否为某术语的目标词」（大小写敏感），而非「命中片段是否属于全部目标词并集」——后者会把引擎未翻译的源术语一并跳过。
- **SPA 动态插入的块级元素富文本聚合失效**：`querySelectorAll` 只检索子孙节点、永不包含元素自身，React/Vue 增量插入的孤立 `<p>`/`<li>` 会漏掉占位聚合，被 TreeWalker 砸成碎文本独立翻译导致语序破损。现显式补上根元素自身的 `matches` 判定。
- **富文本占位符语序被引擎调换时错配回填**：`restoreRichTextComposite` 以线性递增的 `slotIndex` 消费 `baseIds`，该前提仅在占位符顺序未变时成立。德语/日语倒装下引擎调换占位符顺序，会把前半段译文填进后半段 DOM 槽位。现检测到顺序被打乱即返回 `null`，触发既有的安全回退（保留原文 DOM）。
- **历史导出全程独占全局读连接导致应用假死**：导出虽为流式逐行写入，但整个循环包在 `with_reader` 闭包内，全程持有唯一的读连接互斥锁；导出十万条期间缓存查询、划词查词与插件整页翻译全部排队阻塞。现导出改用独立只读连接（WAL 模式下与主连接互不干扰）。
- **LLM 聚合批量失败后无差别降级导致请求放大**：聚合失败一律回退逐项并发（并发度 5，每项内部还各自退避重试），把 1 次请求放大为最多 61 次。鉴权失效/欠费时纯属浪费，429 限流时会加剧限流并可能导致 API Key 或 IP 被封禁。现按 `AggregateFailure::{Fatal, Format}` 区分：仅内容层格式异常（模型未按 JSON 输出）才降级，致命错误直接向整批返回。
- **配对接口 `/pair/*` 可被本地进程实施拒绝服务**：会话表满时按 `created_at` 淘汰最早会话，本机任意进程连发 16 个不同 `client_id` 即可挤掉正常浏览器扩展正在等待的配对会话（扩展轮询立即收到 Expired），且每次请求都会弹出配对窗口构成通知轰炸。现改为满员直接拒绝（`429`）并提供同 `client_id` 节流，同一客户端重复请求复用既有会话而非自我作废。
- **流式任务取消通道失效时被误判为错误**：`translate_stream` 中 `cancel_rx` 返回 `Err(RecvError)`（同键 sender 被覆盖/丢弃）属注册表内部事件，旧实现却直接返回「流式任务的取消通道已失效」，与相邻注释声明的语义自相矛盾，会把本可正常完成的翻译变成伪错误。现该情形改为永久挂起，任务照常返回真实译文。
- **主工作台切换目标语言/引擎不触发重译**：`onChange` 仅调用 `setState`，已有文本时切换语言或引擎不会重新翻译；且主翻译按钮未传 `forceTargetLang`，用户显式选择的「中译中」润色会被智能对转逻辑改写成英文。
- **划词悬浮窗在翻译中切换引擎被静默丢弃**：`!isTranslating` 拦截使等待云端 LLM 时改用 Google 的操作完全失效，旧引擎结果照旧覆盖。`runner.run` 内部已递增代际并取消旧任务，该拦截并无必要。
- **旧版兼容路径把「译文与原文相同」误判为失败**：专有名词、品牌名、代码标识符（Python/Linux 等）译后本就与原文一致，旧判定将它们记入失败集，导致整页翻译成功却显示 WARN 与「部分失败」。
- **`validation.rs` 硬编码单条文本上限**：`10_000` 改为引用 `crate::MAX_ITEM_CHARS`，恢复项目「批量上限单一契约来源」规则。
- **划词悬浮微标与术语搜索框硬编码中文**：微标 `'译'` 与术语库搜索占位符未走 i18n，英文环境下仍显示中文；同时界面语言偏好未持久化、悬浮窗语言写死为 `zh-CN`，现统一存入 `localStorage` 并跨窗口同步。
- **配对弹窗、OCR 确认弹窗与历史面板缺失暗色模式**：补全 `dark:*` 类名，消除暗色模式下的刺眼白底与对比度失衡。
- **Toast 与服务状态轮询定时器未在卸载时清理**：补上 cleanup，避免组件销毁后仍触发 `setState` 与残留 IPC 轮询。

### 变更
- **E2E 断言由「静默跳过」改为显式失败**：`test-desktop-ui-e2e.mjs` 中原有 10 余处 `if (await x.isVisible())` 守卫，选择器漂移或组件未挂载时只会静默跳过并照常打印通过。移除守卫后立即暴露出术语库用例的选择器自诞生起就从未匹配成功（placeholder 实际为「源术语」而非「原文」、按钮文案为「添加术语」而非「添加规则」），即该用例此前从未真正执行；同时修正 Mock 数据 `created_at` 由 ISO 字符串改为契约要求的 Unix 秒级整数（原值会渲染成 `Invalid Date`）并新增显式守护断言。
- **拆分 `commands/mod.rs`**：将划词与悬浮窗相关平台能力（含三阶段剪贴板竞态处理的 `trigger_selection_replacement`、`trigger_selection_translation` 与 `place_float_near_cursor`）移入 `commands/selection.rs`，主文件由 2345 行降至 1967 行；符号经 `pub use` 重导出，命令注册路径与 `main.rs` 引用保持不变。
- **README 补齐全部发布产物与破坏性变更升级提示**: 将安装章节重构为按平台（Windows / macOS / Linux）分列的表格，补齐此前遗漏的 `.rpm`（Fedora 系）、`.msi`、`Transly_aarch64.app.tar.gz`（应用内更新）与 Firefox 专属扩展包，使 README 声明与实际 Release 产物逐一对应；在「已知限制与注意事项」中新增「商业引擎失败不再自动降级 (0.4.0 起)」条目，说明移除自动降级后的真实报错行为并给出配置 Key、手动换引擎、改用离线引擎三种应对路径；同时标注当前处于内测阶段、正式发布时将重置版本号。
- **README 产物文件名版本号占位化**: 将安装章节中硬编码的版本号统一替换为 `<版本>` 占位符（如 `Transly_<版本>_amd64.deb`、`transly-extension-v<版本>.zip`、`Transly-<版本>-1.x86_64.rpm`），并说明 `<版本>` 即实际发布的语义化版本号；对本身不含版本号的固定文件名（`Transly-Portable-windows-x64.exe`、`Transly_aarch64.app.tar.gz`）显式标注，避免读者误以为遗漏。此后发版无需再手工同步 README，消除文档随版本漂移的维护负担。

## [0.4.0] - 2026-10-16

### 变更
- **项目授权模式转型为 Open-Core 商业买断混合许可**:
  - 底层核心逻辑库 `crates/transly-core` 独立声明为 **Apache License 2.0** 开源许可，为第三方开发者与社区提供无侵权隐患的底层翻译、缓存、OCR与语种检测基础引擎；
  - 桌面客户端前端 (`src/`)、平台适配层 (`src-tauri/`) 与沉浸式浏览器扩展 (`extension/`) 正式转为**商业专有买断授权 (Proprietary Commercial EULA)**，支持个人源码安全审计与本地私有构建调试 (Source-Available)，严禁未经许可公开发布、转售、租赁或重新打包商业成品；
  - 在 `src-tauri/Cargo.toml` 中标记 `publish = false` 与 `license = "LicenseRef-Proprietary"`，`package.json` 显式标注 `"license": "UNLICENSED"`，并同步更新全仓 19 处子模块 README 的许可证链接与声明。

### 新增
- **全离线轻量嵌入式翻译引擎支持 (`OfflineEngine`)**: 在 `transly-core` 中新增纯本地无外呼、零第三方服务依赖的离线翻译引擎，配合 Unicode 脚本语言检测器，完全满足无网断网、极度私密场景下的基础翻译保障；桌面端主窗口、划词悬浮迷你窗、设置中心与浏览器插件同步支持选择该引擎。
- **Firefox 浏览器扩展全链路打包与自动化校验门禁**: 新增 `scripts/test-firefox-package.mjs`，在 `package.json` 的 `test:e2e` 中纳入 Firefox 产物自动化校验，确保输出严格符合 Gecko MV3、UUID 绑定与白名单契约。
- **跨平台原生 OCR 异常优雅提示与平台分流优化**: 在 `transly-core` 与平台层中针对 Linux 等暂无原生 OCR 组件的系统输出友好的指引提示，强化位图内存预算与候选语言过滤。

### 修复
- **配对倒计时使用服务端权威 TTL**：修复桌面端收到 `expires_in` 后，倒计时 effect 又固定重置为 60 秒的问题，避免服务端调整配对会话时前端提前或延后显示过期。
- **浏览器扩展本地服务 URL 严格回环地址校验 (`serverUrl.ts`)**：增加 `normalizeLoopbackServerUrl` 与 `normalizeServerPort`，严格限制扩展通信仅允许回环地址（`127.0.0.1`、`localhost`、`[::1]`），防止恶意配置或误配导致 Bearer Token 泄露到外部网络。
- **DeepSeek 本地端点免 API Key 支持 (`commands/mod.rs`)**：在 `is_api_key_configured` 中对 `deepseek` 增加与 `openai` 同等的本地端点判定，当指向 Ollama 等本地回环服务时自动放行免配 Key。
- **本地批量翻译接口 items ID 唯一性校验 (`server/mod.rs`)**：在 `/translate` 入口对批次内 item ID 增加唯一性检查，杜绝重复 ID 导致下游客户端结果映射产生覆盖与错位。

## [0.3.1] - 2026-10-15

### 移除
- **移除多引擎对比翻译模式**: 移除桌面端多引擎同屏并发对比翻译交互、卡片聚合渲染与代际取消定时器逻辑，精简主翻译窗口为单引擎直连模式，直接呈现引擎选择下拉框；同步移除多语言包中的 `modeSingle` / `modeCompare` 键值，以及对应的前端与 E2E 自动化测试用例，降低并发调用额度开销与竞态维护负担。
- **彻底移除跨翻译引擎自动降级机制 (Breaking Change)**: 移除 `FallbackEngine` 包装器及其在 `EnginePool` 中的强制包裹逻辑，消除商业引擎（DeepL / OpenAI / DeepSeek）报错时静默切换至 Google 网页翻译所导致的用户困惑与质量降级；精简 `EngineCapabilities`，移除 `fallback_engine` 字段；优化缓存查询逻辑，严格按当前引擎隔离历史缓存；各引擎在遇到未配密钥或网络故障时直接向上报告真实错误；同步更新中英文语言包下拉文案。
- **移除过程性计划与设计稿文档**: 移除 `docs/superpowers/` 下全部 19 篇文档（10 篇 `plans/` 实施计划与 9 篇 `specs/` 设计稿）。这些文档是「当时」的决策快照，其对应功能已全部落地并记入本更新日志，继续留在版本库中只会造成文档沉余与事实漂移（代码早已偏离当初的计划措辞）；同时在 `.gitignore` 新增 `/docs/superpowers/` 规则，使后续同类过程文档默认不入库，版本库只保留长期有效的架构与规范说明。
- **移除评审报告与状态机架构文档**: 移除 `docs/2026-10-11-full-audit-review.md`（全量代码评审报告，其中 11 条 P0 与 22 条 P1 发现项已全部落地）、`docs/architecture-state-machines.md` 与 `docs/README.md`（其索引对象已随本次清理删除，索引本身失去意义）；同步移除 `src/task/README.md` 与 `PROJECT_RULES.md` 中对上述文件的引用，避免死链。
- **移除根目录踩坑知识库 `Lessons Learned.md` (Breaking Change)**: 移除 637 行踩坑记录文件，并同步清理 `crates/transly-core/src/cache.rs`、`src/bindings.ts`、`extension/src/selection/index.ts`、`extension/src/background/index.ts`、`src/task/useTaskRunner.ts`、`src/task/README.md`、`PROJECT_RULES.md` 中共 8 处对其条目的引用；随后进一步移除源码注释中沉淀自该知识库的因果解释文字（`CacheEntry` 字段类型取舍、气泡代际设计动机、空 `catch` 暴露规程的由来），注释统一回归极简描述。移除原因在于集中式踩坑文档与注释中的经验复述均难以随代码同步演进，容易造成文档沉余与事实漂移。**注意：`created_at` 必须保持 `u32` 而非 `i64`**——specta 会把 `i64` 导出为 JS `BigInt`，在 `JSON.stringify` 时直接抛错并导致前端整页崩溃，该约束现仅由类型定义与 CI 绑定比对保障。

### 新增
- **悬浮迷你窗双向对转快捷键 (`FloatWindow.tsx`)**: 悬浮迷你窗新增 `Ctrl+S` / `Cmd+S` 快捷键，支持全键盘极速在中文（`zh-CN`）与英文（`en`）之间双向对转切换并自动触发重新翻译。
- **术语库前端即时搜索与过滤 (`App.tsx`)**: 在设置中心的术语库卡片中新增术语本地搜索框，支持在数百至上千条术语中实时按源词或译文关键词模糊过滤并高亮展示。
- **任务耗时感知展示 (`useTaskRunner.ts`, `App.tsx`, `FloatWindow.tsx`)**: 在任务状态机快照中引入精确耗时指标 `durationMs`，并在主窗口与悬浮迷你窗的引擎信息栏同步呈现耗时（如 `1.2s`），让用户直观感知各引擎与大模型网关的响应性能。
- **SQLite 缓存物理回收与 WAL 截断 (`cache.rs`)**: 在全量清空翻译缓存（`clear_all_cache`）后追加调用 `PRAGMA wal_checkpoint(TRUNCATE)` 与 `VACUUM`，在磁盘底层释放数据页与缩容 WAL 文件，杜绝长期运行的磁盘碎片膨胀。
- **扩展后台首包瞬态容错快速重试 (`extension/src/background/index.ts`)**: 在扩展端处理 `TRANSLATE_BATCH` 请求时，针对电脑唤醒或桌面端进程冷启动初期的网络被拒或连接抖动（HTTP 5xx 或无响应），增加间隔 400ms 的静默快速重试，大幅提升系统休眠唤醒后的整页翻译成功率。
- **OpenAI/DeepSeek SSE 流式行缓冲跨包分片拼接 (`openai.rs`)**: 在 `OpenAIEngine::translate_stream` 中引入跨 TCP chunk 的流式行缓冲机制，未闭合的残余分包字节保留在缓冲区留待后续报文拼接成完整数据行，彻底消除网络分包或高频拆包导致的数据行阶段性反序列化失败与字符丢失问题；补充对应分包重组单元测试。
- **本地扩展配对会话被动惰性驱逐 (`pairing.rs`)**: 在 `PairingManager::poll_session`、`approve` 与 `reject` 调用链路中均顺带执行过期会话（`now < s.expires_at`）清理，消除极端探测与异常断连场景下的陈旧会话滞留。
- **LaTeX 数学公式保真保护与还原 (`preserve.rs`)**: 在 `stash_code_blocks` 中新增块级公式（`$$...$$`）与行内公式（`$...$`）识别与占位符保护逻辑，严格遵循 Pandoc 边界规范（排除 `\$` 转义及 `$100` 等金额数字误判），防止学术文档中的数学公式被翻译引擎篡改；补充对应单元测试。
- **推理模型 `reasoning_content` 容错回退 (`openai.rs`)**: 在 `OpenAIEngine` 的同步与流式响应解析中支持 `reasoning_content` 字段，当接入推理模型（如 DeepSeek-R1）且正式 `content` 为空时安全回退消费推理文本。
- **悬浮迷你窗 (`FloatWindow.tsx`) 引擎切换与首选同步**: 悬浮迷你窗初始化时自动读取用户配置的划词首选引擎（`getPreferredReplaceEngine`），并在顶栏提供引擎选择下拉框且切换后即时重译。
- **流式文件导出后端命令 (`export_translation_history`)**: 新增直接在后台线程流式导出翻译历史到本地文件的后端命令，支持 CSV（带 BOM 防 Excel 乱码与公式注入转义）与 JSON 格式，直接从 SQLite 游标写入用户 Downloads 目录，彻底消除前端一次性拉取数万条记录跨 IPC 传输导致内存暴涨与界面假死的问题。
- **悬浮迷你窗独立架构拆分 (`FloatWindow.tsx`)**: 在应用根入口 `main.tsx` 进行路由级分流，彻底解耦悬浮迷你窗（`#float`）与主窗口巨石组件，浮窗不再初始化主窗口的 50+ 个 Hook 与多套模态状态，冷启动性能大幅提升且内存更轻量，同时消除主窗口文本输入时的全局组件重绘。
- **浏览器扩展本地服务端口自适应配置与动态连接**: 扩展端后台 Service Worker 支持从本地存储动态读取 `transly_server_port` 或 `transly_server_url`，并放宽回环权限至 `http://127.0.0.1/*`；E2E 测试套件自动嗅探空闲端口，彻底解决本地已运行 Transly 桌面端时测试出现 `EADDRINUSE: 39281` 端口死锁的问题。
- **CI 工作流完整纳入桌面与扩展端到端自动化测试及 macOS Runner**: 在 `.github/workflows/ci.yml` 中配置 Playwright 无头环境，将桌面端 UI 交互 E2E（`pnpm run test:e2e:ui`）与浏览器插件真实端到端 E2E（`pnpm run test:e2e:extension`）纳入 CI 硬门禁，并新增 `macos-latest` 测试矩阵，保障跨平台质量。
- **多策略网络代理体系支持 (Auto / System / Manual / None)**: 新增 `ProxyMode` 与 `ProxyConfig` 强类型契约，支持默认自动（环境代理优先 -> 自动嗅探系统原生代理 -> 直连回退）、系统代理（强制读取 Windows 注册表 Internet Settings 等）、手动配置（支持指定 HTTP/HTTPS/SOCKS5 代理端口）以及强制直连（禁用代理）；在 `src-tauri` 平台层通过 `Win32_System_Registry` 原生嗅探 Windows 注册表代理；在桌面端设置面板新增“网络代理设置”交互卡片，修改后即时热重载引擎连接池，无需重启应用立即对桌面端与浏览器插件生效。
- **便携版单文件全内嵌标准构建工作流**: 在 `package.json` 中固化 `"build:portable": "tauri build --no-bundle"` 脚本，确保单文件编译时将前端静态资产（`dist/`）完整打包内嵌，杜绝裸运行 `cargo build --release` 导致的 `localhost:5173` 拒绝连接（ERR_CONNECTION_REFUSED）问题。
- **桌面端 UI 全交互真实渲染端到端自动化测试套件**: 在 `scripts/test-desktop-ui-e2e.mjs` 中构建基于 Playwright 的桌面端前端真实渲染 E2E 测试套件（`pnpm run test:e2e:ui`），全真模拟 37 个 Tauri IPC 命令与事件通道注册表，覆盖主窗口挂载、主题即时切换、即输即译、历史记录检索过滤与导出、设置中心与术语库管理、浏览器扩展配对申请弹窗以及迷你划词悬浮窗（`#float`）等全部 7 项交互场景，验证 0 未捕获异常。
- **Rust 本地服务真实 TCP 协议端到端集成测试**: 在 `src-tauri/src/server/tests.rs` 中新增 `test_real_tcp_server_e2e_lifecycle`，绑定系统真实 TCP 回环端口，全链路验证网络握手、`/health` 探针、DNS Rebinding 恶意 Host 阻断（403）、配对申请/轮询状态机、Bearer Token 强鉴权与批量翻译术语干预及缓存写入。
- **跨进程真实 Rust 本地服务与浏览器扩展端到端联调测试**: 新增 `transly_test_server` 轻量测试服务器二进制，升级 `scripts/test-extension-e2e.mjs`，由真实 Rust 进程托管 127.0.0.1:39281 本地服务与测试页面，实现浏览器扩展在真实浏览器环境中向真实 Rust 后端发起跨域通信与整页沉浸式翻译的全闭环验证。
- **划词原地替换首选引擎配置与直接报错提醒**: 支持在桌面端设置中选择划词原地替换（默认 `Alt+Shift+R`）的首选引擎（`Google` / `DeepL` / `DeepSeek` / `OpenAI`），配置安全保存在系统原生凭据中；当所选商业引擎缺少 API Key 或网络请求失败时，通过 `transly://replacement-failed` 事件向主窗口广播并在界面弹出明确 Toast 告警，杜绝静默隐蔽降级，并自动安全还原系统剪贴板。
- **浏览器扩展 Playwright 真实无头环境端到端集成测试**: 新增基于 Playwright 的无头 Chromium 插件集成测试套件（`pnpm run test:e2e:extension`），在挂载解压扩展与真实 DOM 环境下完整覆盖普通文本仅译文原地替换、`<a>` / `<strong>` 行内标签富文本保真、开放模式 Shadow DOM Web Components 穿透、双语对照模式以及一键还原 8 项核心用例。

### 修复
- **划词原地替换网络等待期剪贴板外部修改防卫 (`commands/mod.rs`)**: 在划词原地替换的第三阶段写入译文前，先探测当前剪贴板是否已被用户在外部应用中修改。若当前剪贴板既不等于选区原文也不等于旧剪贴板，立即中止写回与模拟粘贴，彻底杜绝翻译等待期间用户复制的新数据（如代码、密码）被静默冲掉。
- **未闭合块级数学公式避免吞噬后续正文 (`preserve.rs`)**: 修复当 Markdown 输入存在未闭合的 `$$` 围栏时，因仅消费首个 `$` 而在下一个字符将第二个 `$` 误判为行内公式起点、进而将文末所有正文吞噬进 stash 的严重缺陷；补充对应回归测试。
- **DeepL 批量翻译集成字符数与项数双上限 (`engine/deepl.rs`)**: 在 `DeepLEngine::translate_batch` 中集成 `plan_batch_ranges` 切片规划，同时满足项数（≤30）与字符总量（≤50,000）双重约束，杜绝长段落批量翻译时触碰 DeepL 网关 413 Payload Too Large 报错。
- **评审报告 15 项全量缺陷与质量加固**: 统一 `extension/scripts/build-firefox.mjs` 与服务端 `ALLOWED_FIREFOX_EXT_IDS` 的 Gecko Addon ID 并增加测试交叉断言；在翻译与流式链路中先提取代码块保护再预应用源端术语库替换，消除新增术语被旧缓存屏蔽及流式缓存命中重复广播事件的问题；修复浏览器扩展双语模式在重试/动态变更时重复插入译文节点、富文本段落悬停查看原文残缺（穿透提取完整段落原文）及 `cachedLimits` 配置变更后不刷新的问题；将 `save_proxy_config` 与 `reload_proxy` 的 SQLite/注册表同步 I/O 下沉至 `spawn_blocking`；新增 `import_glossary_terms` 单事务批量导入命令；为 `ShortcutStatusDto` 增加独立 `translate_error` / `replace_error` 字段消除硬编码快捷键冲突拆分；为 `FloatWindow.tsx` 补齐暗黑模式同步与复制失败提示，区分设置页 `saveStatus` 成功与失败视觉样式；加固 `format_timestamp_iso` 非负钳制与 `L1Cache` 队列顺序一致性，并清理多处历史遗留死代码。
- **代码块与数学公式占位符丢失安全保真 (`preserve.rs`)**: 当大模型偶发丢弃或改写生成的 `⟦TRANSLY_CODE_N⟧` 占位符时，`restore_code_blocks` 自动追踪未命中的 `stash` 索引并将其安全补回译文末尾，杜绝因模型格式幻觉静默删除用户的代码块或数学公式。
- **Google 网页翻译长文本跨语种分片拼接空格保真 (`google.rs`)**: 修正长文本切片后的拼接分隔符逻辑，同时考察译文首尾字符（西文词尾/标点与西文词首）与原文末尾特征，彻底解决长段中文切块翻译为英文时分片粘连（`...worldThis...`）及段落末尾残留多余空格的问题。
- **翻译缓存 L1 内存写入严格后置于 SQLite 事务提交 (`cache.rs`)**: 调整 `set` 与 `set_batch` 的写入时序，仅在 SQLite 写连接或事务 `commit()` 成功后才更新 L1 内存缓存，防止磁盘满或锁失败时 L1 产生未持久化的脏数据。
- **悬浮迷你窗切换引擎与目标语言即时生效 (`FloatWindow.tsx` / `useTaskRunner.ts`)**: 为 `runner.run` 扩展 `RunOptions`（支持直接传入 `engine` 与 `targetLang` 并同步刷新内部 Ref），修复在 React 异步状态重渲染前立即触发重译仍沿用旧引擎或旧目标语言的问题。
- **本地 HTTP 服务 CORS 扩展身份白名单固化 (`server/mod.rs` / `extension/manifest.json`)**: 为 Chrome 扩展 `manifest.json` 注入固定 RSA-2048 SPKI 公钥以锁定唯一扩展 ID（`ngfadgochghcplecgmdcjfancoikiihm`），并将服务端 `is_allowed_origin` 从宽泛的格式校验收紧为精确扩展 ID 白名单匹配，阻断第三方恶意扩展跨域调用 `/pair/request` 诱导配对。
- **浏览器扩展富文本占位符去重校验 (`richText.ts`)**: 在 `restoreRichTextComposite` 中新增占位符集合唯一性校验（`new Set(tokens).size === placeholderToId.size`），拦截重复输出同一占位符顶替缺失占位符的异常情况并安全触发回退。
- **SQLite 缓存与 WAL 旁路文件迁移失败原子回滚 (`storage.rs`)**: 在 `migrate_db_files` 中记录已移动文件列表，当 `-wal` 或 `-shm` 旁路文件移动失败时逆序回滚已搬移的主库文件，杜绝主库与旁路文件割裂导致的数据丢失。
- **CI/Release 工作流与跨平台扩展 E2E 加固 (`ci.yml` / `release.yml` / `test-extension-e2e.mjs` / `proxy.rs`)**: 为 `security-scan` 补齐 `pull-requests: read` 权限解决 PR 触发 Gitleaks 403 报错；在 `release.yml` 中仅对 `refs/tags/v*` 注入扩展版本号以兼容 `workflow_dispatch` 手动构建；在 CI 与扩展 E2E 中编译并跨平台启动真实 `transly_test_server`，优先选用支持 `--load-extension` 的 Playwright Chromium 信道；移除 `proxy.rs` 单测对特定开发机代理端口的硬编码断言。
- **浏览器扩展富文本占位符括线内空格容错归一化 (`richText.ts`)**: 针对大模型偶发在特殊占位符括线内插入空格（如 `⟦ T_1 ⟧` 或 `⟦/T_1 ⟧`）的现象，在 `extractPlaceholderTokens` 与 `restoreRichTextComposite` 中预先执行宽松正则归一化，避免占位符失配回退；补充对应单元测试。
- **浏览器扩展划词悬浮图标视口右边界防溢出 (`selection/index.ts`)**: 对划词悬浮“译”图标的横向坐标执行视口右边界夹取，防止贴边选中文本时图标超出视口引发页面横向滚动条。
- **扩展配对会话倒计时采用服务端权威 `expires_in`**: `transly://pairing-requested` 事件载荷新增 `expires_in` 字段，前端配对弹窗优先使用服务端下发的过期秒数，消除前后端双边硬编码潜在脱节。
- **浏览器扩展多层嵌套行内标签富文本段落保真翻译与深层提取**: 加固 `isLeafRichTextContainer`（引入 16 种合法行内标签白名单并检查全部子孙元素属性）与 `collectTextNodes`，行内元素采用 `TreeWalker` 中序深度遍历收集所有深层未翻译文本节点，并将段落所有子孙元素登记至 `processedContainers`，彻底解决 `<p><strong><a href="...">link</a></strong></p>` 等多层嵌套结构下深层文本被整段漏翻与碎片化翻译的缺陷；在 `scripts/test-extension-e2e.mjs` 中新增针对真实 DOM 的端到端回归用例（用例 3.5），并在 `richText.test.ts` 中补充嵌套聚合单元测试。
- **划词翻译气泡脚本 JSDoc 注释对齐隐私政策**: 修正 `extension/src/selection/index.ts` 头部注释对 `mouseup` 与 `selectionchange` 选区监听描述的陈旧陈述，如实反映当前仅在本地判定选区并驱动悬浮“译”图标、绝不主动读取或上传非选中文本的机制，与 `PRIVACY_POLICY.md` 保持完全一致。
- **富文本段落占位聚合奇偶硬编码与 DOM 错位颠倒修复**: 修正 `richText.ts` 与 `content/index.ts`，摒弃基于奇偶索引的脆弱推断，改由显式的 `isInline: boolean` 属性驱动，通过 `tokens.length + 1` 拓扑槽位实现对行内标签开头、连续标签等复杂 HTML 结构的 100% 精准回填与还原；补充行内开头与连续标签单元测试。
- **DeepL 批量响应条数硬契约对齐校验**: 修正 `deepl.rs` 中的 `translate_batch`，在反序列化后严格校验返回条数与请求切片长度一致性，数量失配时立即报错，触发上层 `translate_batch_tolerant` 安全退化为逐项单请求隔离翻译，杜绝由于数量差错导致的译文整体错位偏移。
- **整页翻译 DOM 遍历流式优化**: 移除 `content/index.ts` 中针对 Shadow DOM 探查的 `root.querySelectorAll('*')` 全页大数组分配，改用原生 `TreeWalker` 流式遍历与节点过滤，消除复杂网页或长列表页面上数百毫秒的主线程冻结与布局抖动。
- **术语库增量删除与 AC 自动机缓存保留**: 为核心 `Glossary` 引入原生 `remove` 方法，更新 `delete_glossary_term` 命令，在删除术语时直接执行内存增量更新并使编译缓存失效，彻底消除原本“全表读取 -> 销毁旧实例 -> 重新逐项循环插入所有词条”的沉重开销。
- **Google 网页翻译端点风控拦截与级联假死修复**: 将 `GoogleWebEngine` 默认客户端标识由易触发 429 人机拦截的 `client=gtx` 升级为 Chrome 扩展官方标识 `client=dict-chrome-ex`，并提供容错备选回退；配合系统代理自动嗅探，彻底解决国内网络环境下整页翻译单项超时 20 秒、整批串行挂死长达近 100 分钟的问题。
- **浏览器扩展批次失败统计与快速熔断机制**: 修正 `content/index.ts` 中的 `applyBatchResponse` 与 `sendBatchWithSplit`，当单批全项失败时正确上报失败批次计数（`failedBatches: 1`），使扩展徽标及时转为警示黄标（`WARN`）；新增连续 2 批 100% 失败快速熔断保护，遇网络不可达时即刻中断后续批次并弹出明确的中文引导提示，消除页面假死。
- **中英混排术语库单词边界误判修复**: 修正 `glossary.rs` 中 `is_word_char` 判定逻辑，排除中日韩 CJK 汉字字符，使汉字与西文字词相邻（如「用Rust重构」）时构成合法的词边界，解决中英混排场景下英文术语 100% 无法命中的缺陷；补充中英紧挨词条替换回归单测。
- **IPv6 Host 头解析切分漏洞加固**: 修正 `server/mod.rs` 中 `host_header_allowed` 对 IPv6 字面量的解析，严格验证 `']'` 之后仅允许为空或纯数字端口 `":\d+"`，拦截形如 `[::1].evil.com` 的绕过式域名，彻底堵死 DNS Rebinding 防线中的解析盲区；补充反例单测。
- **浏览器扩展富文本段落占位符回填硬阻断**: 在 `content/index.ts` 中修正 `applyBatchResponse` 复合项处理逻辑，当翻译结果中占位符丢失或语序损坏导致 `restoreRichTextComposite` 返回 `null` 时，立即阻断并登记失败项，禁止穿透写入首个子文本节点，杜绝 DOM 产生重复堆叠与排版错乱。
- **动态 MutationObserver 监听保留全局失败账本**: 移除增量监听防抖回调中对 `failedItemsMap.clear()` 的多余调用，确保首屏翻译产生的部分失败节点不会因页面后续微小 DOM 变动而被静默抹除，维持重试按钮有效性。
- **桌面端前端输入清空与任务重置原子化**: 在 `useTaskRunner.ts` 中规范输入清空行为，当 `!text.trim()` 时立即作废在途代际并复位 UI 快照至 `EMPTY`，防止旧译文常驻或永远停留在 `running` 打字机光标态；新增 `runner.reset()` 供「清空内容」按钮调用。
- **React 19 异步 Event 监听泄漏与悬浮窗通知重复治理**: 在 `useTaskRunner.ts` 的事件订阅中引入 `disposed` 守卫，杜绝组件快速卸载/重挂载导致的监听器内存泄漏；在 `App.tsx` 的 `transly://replacement-failed` 监听回调中增加 `if (isFloatWindow) return` 门禁，消除原地替换失败时多窗口重复弹出系统原生通知的问题。
- **剪贴板划词翻译与替换异常分支锁保护与 RAII 抑制复位**: 在 `trigger_selection_replacement` 失败与拦截分支中引入 `CLIPBOARD_SIMULATION_MUTEX` 互斥保护，并在 `trigger_selection_translation` 中引入 `SelFlagGuard`，防止任何异常导致剪贴板监听被永久静默。
- **Tauri 2 ACL 权限补齐与浮窗 Escape 关闭**: 在 `src-tauri/capabilities/default.json` 中补全 `"core:window:allow-hide"` 权限声明，并在 `App.tsx` 中为 `getCurrentWindow().hide()` 挂载未捕获 Promise 异常守卫，彻底解决悬浮迷你窗在真实环境中按 `Escape` 键无法关闭的问题。
- **浏览器扩展 Shadow DOM 节点类型规范化与双语还原修复**: 修正 `domTraversal.ts` 与 `content/index.ts` 中针对 ShadowRoot 的容器类型守卫，放行 `Node.DOCUMENT_FRAGMENT_NODE`（11），使 Web Components 内部富文本段落及嵌套 Shadow DOM 正常提取与翻译，并解决双语对照模式下 Shadow DOM 内兄弟译文节点无法还原清理的问题；将单测 mock 对齐 W3C 真实标准。
- **DeepL 批量翻译错误体脱敏防泄露**: 统一 `DeepLEngine::translate_batch` 错误处理，消除向前端与扩展 DOM 透传上游响应体的安全隐患，防止授权或账户敏感信息回显。
- **历史面板 CSV 导出双重引号转义修复**: 消除 `HistoryPanel` 导出 CSV 时调用方与 `formatCsvRow` 重复调用 `escapeCsvField` 导致字段出现破损三引号（`"""text"""`）的问题，并补充单行格式化与解析往返回归单测。
- **全局快捷键修改后状态槽位即时同步**: 在 `set_shortcut` 与 `set_replace_shortcut` 中注入 `ShortcutStatusSlot`，在快捷键注册成功与回滚时即时更新槽位状态，解决启动冲突后用户修改快捷键告警无法自动消除的问题。

### 优化
- **扩展端深层 DOM 遍历与双语还原单趟合并 (`domTraversal.ts` / `content/index.ts`)**: 合并 `iterateElementsDeep` 中的元素 yield 与开放 Shadow DOM 递归以及 `restoreOriginalText` 中的双语节点移除与属性清理，减少重复全量 DOM 扫描。
- **统一全链路端到端自动化测试入口**: 在 `package.json` 中配置 `"test:e2e"` 聚合命令，实现一键并行串联桌面端 UI 全交互测试与浏览器插件真实跨进程全套端到端自动化验证。
- **统一本地持久化配置仓储与冷启动脱敏**: 在 SQLite `cache.db` 中引入 `app_settings` 配置表，将剪贴板监听开关、缓存开关、快捷键设置、划词替换首选引擎、LLM 翻译风格及端点模型等非敏感配置从系统原生凭据管理器迁移至本地数据库，消除主线程启动阶段因串行读取凭据库（Keyring/DBus）引发的阻塞延迟，实现旧版无缝迁移；商业 API Key 与内部 Server Token 严格保留于操作系统原生安全凭据库中。
- **全量翻译历史导出支持**: 在 `HistoryPanel` 导出 CSV 与 JSON 时优先拉取 SQLite 全量历史数据（至多 100,000 条），彻底解决此前仅导出前端分页加载的首屏 50 条导致历史数据遗漏的问题。
- **快捷键录制组件视觉与可访问性增强**: 在 `ShortcutRecorder` 中增加 `keyup` 事件监听，实时响应修饰键释放并即时清空卡滞提示；重构录制卡片布局结构，消除 `<div role="button">` 内部嵌套原生 `<button>` 违反 W3C a11y 规范的问题。
- **深色模式（Dark Mode）样式深度对齐**: 补齐主翻译输入框容器、文本区与设置中心各配置卡片、下拉选择框及文件操作按钮的 `dark:` Tailwind 类名，消除切换暗色模式时的界面白灰斑驳与对比度撕裂。
- **本地端点与 Base URL 判定严谨化**: 在 `OpenAIEngine::is_local_endpoint` 与 `save_llm_config` 中引入规范 URL 解析，严格对齐主机名白名单（`localhost` / `127.0.0.1` / `::1`），杜绝形如 `localhost.evil.com` 或 `evil.com/localhost` 伪装子域名绕过非 HTTPS 限制。
- **划词气泡锚点定位包含选区高度**: 在 `selection/index.ts` 中采集选区矩形高度（`rect.height`），正常朝下放置时精准置于选区正文下方，杜绝气泡直接压在被选中文本上方遮挡视线。
- **Google 网页翻译批量容错隔离与受控并发**: 在 `GoogleWebEngine` 中直接实现 `translate_batch_tolerant`，按 4 项一组受控并发请求并逐项保留 `Result`，消除单项失败时导致整批成功项被全量重复二次外呼的性能浪费。

## [0.3.0] - 2026-10-11

### 安全

- **本地服务缺少 `Host` 头校验，DNS Rebinding 可窃取配对 Token (P0)**：唯一防线是 CORS 的 `AllowOrigin` 谓词，而 **CORS 不是服务端访问控制**——攻击者把域名 A 记录 TTL 设为 0 指向 `127.0.0.1` 后，恶意页面与本机服务同源，浏览器不发预检也不校验 CORS。`/pair/request` 为免鉴权握手端点，可被完整利用：诱导用户点「允许」即交出 Bearer Token，随后可任意消耗用户付费的翻译额度。现新增 `host_guard` 中间件（位于 CORS 之外），仅接受 `Host` 为 `127.0.0.1` / `localhost` / `[::1]`（含端口与 IPv6 字面量解析），其余一律 403；配套 2 项测试，并让既有 20 处请求构造携带合法 `Host`（此前全部省略，等于把被测防线绕过去）。
- **鉴权对空 Token 失效（fail-open）(P1)**：`constant_time_eq(b"", b"")` 恒为真，期望令牌为空串时 `Authorization: Bearer `（空值）即可通过。当前靠「Token 生成失败则不启动服务」偶然规避，属隐式正确。现 `bearer_matches` 对空期望值直接 fail-closed。
- **CORS 白名单只校验格式不校验身份 (P1)**：Chrome 分支只要 `chrome-extension://` + 32 个 `a-p` 字符即放行，任意已安装扩展均可读取全部响应。已在注释中明确标注该属性为格式校验而非身份隔离（Chrome 扩展 ID 可钉死，Firefox UUID 运行时生成，属已知折中）。
- **引擎错误体原样外传导致密钥前缀泄露 (P2)**：OpenAI 兼容网关的 401 响应会回显 `sk-proj-XXXX`，该字符串一路进入扩展页面 DOM、前端状态与日志。现仅保留状态码，响应体内容不落任何日志。
- **LLM 端点缺少协议校验 (P2)**：允许任意 `http://` 意味着 API Key 可能明文过网。现强制 `https://`，仅本地回环（Ollama / vLLM）例外。
- **`%APPDATA%` 为漫游目录，隐私数据会被同步出本机 (P2)**：域环境下翻译历史与自建术语库会被组策略同步到其它机器与备份介质，与「本地优先、零上报」承诺冲突。现改用 `%LOCALAPPDATA%`，并新增漫游目录一次性迁移（幂等、不覆盖），避免升级丢数据。

### 优化与增强
- **桌面端单文件免解压免安装便携版支持 (P1)**：在 `src-tauri/src/storage.rs` 中引入智能便携模式判定（支持可执行文件名包含 `portable`、同级 `portable.flag` 标记文件、同级 `data/` 目录、`--portable` 命令行参数及 `TRANSLY_PORTABLE` 环境变量），便携模式下数据隔离保存在程序同级 `./data` 目录中，实现数据随身携带；同级目录不可写时平滑安全降级至系统标准目录。在 GitHub Actions 发布流（`.github/workflows/release.yml`）中自动提取并发布独立的 Windows 单文件免安装可执行程序 `Transly-Portable-windows-x64.exe`，用户下载后无需解压、无需安装向导，双击直接运行。
- **LLM 翻译风格与持久化缓存隔离**：`OpenAIEngine` 实例标识与响应的实际引擎标识接入 `TranslationStyle` 后缀（如 `openai:gpt-4o-mini:academic`），彻底解决切换翻译风格后被旧通用风格缓存劫持的缺陷；添加独立隔离命中单测。
- **SSE 流式数据帧解析优化与多字节安全解码**：在 `OpenAIEngine::translate_stream` 中重构字节流处理算法，基于换行符执行游标单趟扫描，将原每行 `drain` 的 $\mathcal{O}(n^2)$ 内存搬移优化为单趟单次缩容，并在严格换行边界上执行安全 UTF-8 解码，杜绝跨网络 chunk 多字节字符被 `from_utf8_lossy` 截断损坏。
- **OpenAI / DeepSeek 批量翻译受控并发加速**：在 `OpenAIEngine` 中原生实现 `translate_batch_tolerant`，基于 `futures::stream::buffer_unordered(5)` 实施 5 路并发受控异步分发。将原本 30 项整批 45 秒以上的长耗时降低至 8~10 秒内，并在单项失败时保持故障隔离；支持批量能力声明。
- **本地离线模型 (Ollama) 本地回环免 Key 适配**：`OpenAIEngine` 自动识别 `localhost` / `127.0.0.1` / `[::1]` 本地回环端点，无需用户提供任何 API Key 即可直接发起请求，配合 `is_api_key_configured` 对本地端点的免配置放行，极大提升隐私用户的本地离线大模型使用体验。
- **扩展端 DOM 布局抖动消除与 WeakMap 视口缓存**：在 `extension/src/content/index.ts` 中引入 `WeakMap<Element, boolean>` 对父容器的包围盒几何检测进行单趟记忆，避免长页面数千个文本节点连续触发强制同步重排（Forced Synchronous Layout），显著降低主线程卡顿。
- **浏览器扩展富文本段落行内标签保真闭环**：完善 `richText.ts` 闭合标签 `⟦T_N⟧...⟦/T_N⟧` 提取与还原算法，在 `content/index.ts` 遍历时智能识别叶子富文本容器，将含有行内 `<a>`、`<span>`、`<strong>`、`<code>` 的段落整句打包翻译，并在返回后精准拆解回填，彻底杜绝跨语种语序倒装时碎片翻译破版缺陷；占位丢失时平滑安全降级。
- **桌面端悬浮迷你窗 Escape 键极速关闭支持**：在 `src/App.tsx` 中为 `#float` 迷你悬浮窗口添加全局 `Escape` 按键监听，按下瞬间安全隐藏窗口，提升键盘操作流的便捷性。
- **离线语言检测与智能对转西欧拉丁语系防偏转 (P0)**：在 `crates/transly-core` 的 `detector` 模块中细分主要西欧拉丁语系（法、德、西等）的特征重音字母与常用功能词检测。修复当用户默认目标语言为英文时输入欧系小语种（如法文 `Bonjour`）被无差别误判为英文而强行对转为中文的缺陷，确保外翻英场景准确路由；添加对应单测。
- **划词原地替换集成持久化缓存与历史沉淀 (P0)**：在 `src-tauri` 的 `trigger_selection_replacement` 流程中注入 `TranslationDependencies`，在无锁第二阶段优先查询 `TranslationCache`，并在翻译成功后安全写入缓存与历史记录；彻底消除原本原地替换完全绕过缓存与无历史沉淀的问题。
- **批量翻译引擎逐项容错隔离 (P1)**：在 `crates/transly-core` 的 `TranslateEngine` trait 中引入 `translate_batch_tolerant` 规范，并在 `FallbackEngine` 与服务端 `server/mod.rs` 中完整实现。当整批 30 项中个别单项出现网络抖动或超长异常时，自动平滑隔离故障项，确保其余成功项正常返回并写入缓存，杜绝“一处报错整批 30 项全军覆没”。
- **无边框悬浮迷你窗鼠标自由拖拽支持 (P2)**：在桌面端前端 `src/App.tsx` 的悬浮迷你窗（`#float`）Header 容器中添加 `data-tauri-drag-region` 属性与 `cursor-move` 视觉交互，允许用户按住浮窗顶栏自由拖拽移动窗口，解决弹出位置遮挡待读文本却无法挪开的体验痛点。
- **浏览器扩展配对确认浮层窗口隔离 (P2)**：在 `src/App.tsx` 中为 `transly://pairing-requested` 广播事件增加 `if (isFloatWindow) return;` 门禁，确保扩展自动配对请求仅在主窗口渲染，杜绝在 380×240 的微型悬浮窗内弹出裁剪破版卡片。
- **统一 Web Speech TTS 发音代码规范化 (P2)**：创建 `src/utils/speech.ts` 共享发音模块，自动将源语言为 `"auto"` 或非标代码的文本根据字符区段特征规范化为合法的 BCP-47 区域标记（如 `zh-CN`、`en-US`、`ja-JP`），消除浏览器原生发音静默失效缺陷并在主窗口与历史面板中复用；添加对应单测。
- **SQLite 翻译缓存 busy_timeout 锁并发防御 (P2)**：在 `crates/transly-core` 的 `TranslationCache` 初始化中为读写连接配置 `PRAGMA busy_timeout = 5000;`，将 SQLite 默认 0ms 锁超时提升为 5 秒自适应等待，防止后台任务与高并发整页翻译并发写事务冲突报错。
- **SQLite 冷启动后台异步维护 (P2)**：将 `src-tauri/src/main.rs` 启动阶段的 `cleanup_older_than_days` 与 `trim_to_max_items` 移至 `tauri::async_runtime::spawn_blocking` 异步执行，消除主线程冷启动磁盘扫描 I/O 阻塞。
- **术语库复合短语首尾单词边界判定优化 (P2)**：在 `crates/transly-core` 的 `Glossary` 中优化 `word_boundary` 判定规则，支持带空格的多词复合短语（如 `"large language model"`）独立校验首尾字符的非单词边界，杜绝词组内部因空格导致边界校验意外失效；添加对应单测。
- **Google 网页翻译请求伪装浏览器 User-Agent (P2)**：为 `GoogleWebEngine` 配置现代标准浏览器 User-Agent 标头，降低因使用默认客户端标头被 Google GTX 接口临时 429 拦截的风险。
- **Windows 原生 OCR 候选识别语言扩充 (P2)**：在 `crates/transly-core/src/ocr/mod.rs` 中为 Windows 原生 OCR 候选列表补充 `"zh-Hans-CN"` 与 `"zh-CN"`，显著提升非中文系统用户对中文图片截图的文字识别率。
- **OpenAI 兼容引擎 Base URL 自动规整容错 (P1)**：在 `crates/transly-core` 的 `OpenAIEngine` 中增加 `normalize_base_url`，自动识别并剥离用户粘贴输入的 `/chat/completions` 与尾部斜杠，杜绝拼接出 `.../chat/completions/chat/completions` 404 路径；添加对应单测。
- **多引擎对比翻译模式即输即译与防抖调度升级 (P1)**：在桌面端主界面（`src/App.tsx`）中升级多引擎对比翻译体验，输入框打字由独立防抖定时器自动触发多引擎并发对比更新，切换模式时无缝衔接且在对比模式下不再无谓触发单引擎翻译，消除交互割裂与额度空耗。
- **扩展端 `sendBatch` 通信未定义防御加固 (P2)**：在 `extension/src/content/index.ts` 中增强对 Service Worker 通信回调的防御性校验，捕获 `chrome.runtime.lastError` 与 `undefined` 响应，统一映射为结构化失败响应 `{ ok: false, error: ..., status: 503 }`，杜绝未捕获异常。
- **代码占位符抵抗大语言模型空格变动的松弛容错还原 (P1)**：在 `crates/transly-core` 的 `preserve` 模块中强化 `restore_code_blocks`，在严格匹配基础上引入 `⟦\s*TRANSLY_CODE_(\d+)\s*⟧` 正则二级容错提取与还原，彻底解决部分大模型在特殊括线内引入空格导致代码块无法还原丢失的隐患；添加对应单测。
- **扩展配对管理器并发会话硬上限防泛洪 (P2)**：在平台层 `src-tauri` 的 `pairing.rs` 中引入 `MAX_PENDING_SESSIONS = 16` 硬上限，创建会话时自动淘汰最早创建的会话，有效防止本地恶意脚本高频泛洪导致内存与广播事件暴涨；在 `server/tests.rs` 补充单测。
- **LLM 翻译专业领域风格预设 (P2)**：在 `crates/transly-core` 的 `OpenAIEngine` 中引入 `TranslationStyle` 枚举，支持【通用直译】、【学术技术】（严格锁定专业术语与代码标识符）、【文学润色】（流畅地道）与【商务公文】四种 Prompt 风格模板并自适应调节采样温度；桌面端设置中心新增全局风格选择器。
- **本地离线模型 (Ollama) 一键预置向导 (P2)**：在设置中心的 OpenAI 自定义端点配置区域提供一键填入本地 Ollama 默认 API 端点（`http://localhost:11434/v1`）与默认模型（`qwen2.5:7b`），方便离线安全翻译用户零门槛直连本地模型。
- **SQLite 翻译缓存读写分离并发解耦 (P1)**：将 `crates/transly-core` 中的 `TranslationCache` 升级为读写双连接分离架构（`File` 模式下采用独立 `writer` 互斥写连接与 `reader` 只读并发连接）。在 SQLite WAL 模式下，整页翻译的大批量事务写盘与用户前台的高频历史查询/缓存命中完全解耦、互不阻塞，杜绝锁争用卡顿。
- **现代 Web Components 开放 Shadow DOM 穿透翻译 (P2)**：在 `extension/src/content/index.ts` 中增强 DOM 遍历器，递归探测并收集拥有开放模式（`open`）Shadow Root 的自定义组件内部有效文本，彻底解决 Lit/Stencil 等现代前端组件库封装内容被整页翻译遗漏的局限。
- **术语库 Aho-Corasick 单趟多模式扫描重构 (P0)**：在 `crates/transly-core` 中重构 `Glossary::apply` 算法，引入高性能 `aho-corasick` crate (v1.1)。由原先按词长遍历的逐词 `Regex::replace_all` 升级为基于 `MatchKind::LeftmostLongest` 的单趟自动机流式扫描，匹配后精准执行后验 ASCII 单词边界检测，匹配性能从 $\mathcal{O}(N \times L)$ 优化至 $\mathcal{O}(L)$。彻底解决先前术语 A 替换为 B 之后 B 中文字被后续术语再次二次替换的级联缺陷，并天然保障长词优先命中。
- **浏览器扩展整页翻译 Viewport-First 优先切批调度 (P1)**：在 `extension/src/content/index.ts` 与 `shared/batchPlan.ts` 中实现视口感知切批调度。在收集待翻译 Text 节点时计算其实际几何包围盒，将当前可视视口内的可见文本优先拆批置顶并立即发送请求，使得用户在点击翻译后数百毫秒内即可看到当前屏幕文字变成译文；离屏（Offscreen）未滚动到的段落则平滑在后台批次中保序排队，大幅缩减首屏翻译等待的感官延迟。
- **浏览器扩展富文本段落行内标签保真翻译 (P1)**：在 `extension/src/shared/richText.ts` 中实现富文本行内占位提取与译文安全还原算法。针对 `<p>`、`<li>`、`<h1>`-`<h6>` 等段落中内嵌的 `<a>`、`<code>`、`<strong>` 等行内标签，以整句为单元进行占位保护（`⟦T_N⟧`）后统一翻译；返回后自动拆解回填，彻底杜绝跨语种（如中英文语序倒装）时离散文本节点被切碎翻译导致的语序破版，并在占位符丢失时无缝安全回退至逐节点原位替换。

### 变更

- **本地提交门禁恢复有效性 (P0)**：`lefthook.yml` 的三条 cargo 命令（`fmt` / `clippy` / `test`）此前全部以 `|| true` 结尾，退出码被无条件吞掉，**Rust 侧本地门禁 100% 不可能失败**——而 CHANGELOG 却记录着「lefthook 提交门禁真正生效」。现移除 `|| true` 并取消 `parallel`（三条 cargo 命令并发会争抢同一 `target/` 锁，实际被串行化）。
- **CI 补齐「禁止提交文件检查」**：`PROJECT_RULES` 的 CI 清单列有该项，此前无任何自动化实现。现新增 `repo-hygiene` job，拦截被跟踪的构建产物/依赖目录与超过 1MB 的文件。
- **CI 与 Release 全部改用 `pnpm install --frozen-lockfile`**：项目认真跟踪了锁文件，却用宽松解析安装，任何人改了 `package.json` 而漏更 lockfile 都不会被发现。
- **Release 补齐 Firefox 产物与版本号注入**：`build:extension:firefox` 此前从未在 CI 执行，Firefox 用户拿不到产物；扩展 manifest 版本号硬编码，打 tag 后产物版本不递增会被 Chrome Web Store 拒绝。现由 git tag 注入 `TRANSLY_EXT_VERSION`，Firefox 产物一并上传。
- **扩展构建脚本清理过期产物**：`fs.cpSync` 不删除目标目录中已不存在的文件，跨版本构建会残留幽灵文件；`gecko.id` 仍是未替换的 `{{EMAIL_...}}` 模板占位符，addons-linter 会判为非法。已分别修正。
- **格式化检查覆盖根配置与脚本**：`format:check` 的 glob 不含根目录配置与 `scripts/*.mjs`，实测 `tailwind.config.js`、`check-commit-msg.mjs` 不合规却 CI 全绿。现纳入射程并全部格式化。
- **术语库 Aho-Corasick 自动机缓存**：`Glossary::apply` 每次调用都重建自动机（构建成本 O(总模式长度)），500 词条单次约 4.7ms，整页翻译 30 项即累计 142ms 纯浪费且发生在 tokio worker 上。现缓存编译结果并在词条变更时失效。
- **占位符容错正则静态化**：改为 `LazyLock`，避免每次还原都重新编译 DFA。
- **文档与实现对齐**：`docs/README.md` 索引补全全部 16 篇文档（此前自订「每新增一篇必须登记」的规则，实际只登记 1 篇）；`PROJECT_RULES.md` 修正集成测试目录、补充 `storage.rs` 与扩展 `selection/`、`shared/` 目录说明、补记门禁禁止 `|| true` 的规程；`AGENTS.md` 修正 `Cargo.lock` 表述（实际已提交且应当提交）与 clippy 命令（补 `--all-targets`）。

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
- **OpenAI / DeepSeek 流式 SSE 解析器循环控制流缺陷修复 (P0)**：修复 `crates/transly-core` 的 `OpenAIEngine::translate_stream` 中内层 `break` 仅跳出按行扫描循环、外层网络字节流等待循环未中止的问题。引入 `'stream_loop` 循环标签并在收到 `data: [DONE]` 时显式退出外层循环，彻底消除 HTTP Keep-Alive 连接下长文本流式翻译挂起阻塞 30 秒直到客户端超时的严重缺陷。
- **全局快捷键重新注册失败原子回滚保障 (P1)**：在 `src-tauri` 的 `set_shortcut` 与 `set_replace_shortcut` 命令中，若新快捷键因系统热键冲突或格式非法导致注册失败，自动回滚重新注册 `old_shortcut`，彻底防止原快捷键被意外注销导致按键功能永久丢失。
- **原地替换剪贴板锁临界区三阶段拆分消除假死死锁风险 (P0)**：重构 `src-tauri` 中的 `trigger_selection_replacement` 流程，将原来“模拟复制 $\to$ 网络翻译 $\to$ 模拟粘贴”全程持有 `CLIPBOARD_SIMULATION_MUTEX` 互斥锁拆分为三阶段独立作用域。网络翻译过程完全在无锁状态下并发运行，彻底消除网络卡顿或大模型生成慢时挂死划词悬浮窗与剪贴板监听的死锁假死风险。
- **原地替换模拟粘贴时延防外部应用竞态优化 (P1)**：在 `trigger_selection_replacement` 中将模拟 `Ctrl+V` 后恢复旧剪贴板的时延从 120ms 放宽至 350ms，彻底消除 VS Code、Notion、Slack 等基于异步事件循环的编辑器应用尚未读到剪贴板即被写回旧内容覆盖的竞态。
- **DeepL 繁体中文与葡萄牙语目标代码规范化映射 (P1)**：在 `crates/transly-core` 的 `DeepLEngine` 中补齐 `"zh-tw" | "zh-hant" => "ZH-HANT"` 与 `"pt" => "PT-PT"` 大写规范化映射，并在兜底分支将代码统一大写，解决向 DeepL 官方 API 传入小写导致 400 Bad Request 的问题；添加单测锁定。
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

- **划词原地替换销毁用户新复制的剪贴板内容 (P0)**：流程分三阶段，阶段三无条件写回 `old_clipboard`，但阶段二的网络翻译可达数秒，期间用户复制的新内容会被**静默销毁**。现还原前先读取当前剪贴板比对，不等则放弃还原。
- **划词抑制窗口过短导致同一次划词被翻译两次 (P0)**：抑制标志在阶段一结束即复位，而译文要到阶段三才写入，中间窗口内剪贴板监听器会把选区原文当成新内容再触发一次翻译，并污染去重缓存导致用户随后手动复制同一段文本时不再触发。现抑制窗口覆盖全流程，并用 RAII guard 保证提前 return 路径也能复位（否则一次失败即永久抑制）。
- **已取消的流式任务仍被投递并真实计费 (P0)**：代际闸门只挡响应回流、不挡请求投递。`detectTextLanguage` 的 IPC 往返期间代际可能已作废，但请求照常发出，后端完整跑一次 LLM——用户看不到结果，API 已计费且幽灵译文写入缓存与历史。现于探测返回后复查代际。
- **清空输入框后右侧仍冒出旧译文 (P0)**：`run()` 对空串提前 return，未清理防抖、未取消在途任务。现清理动作排在空串守卫之前。
- **浮窗划词会误杀主窗口正在运行的 LLM 流 (P0)**：取消注册表键为全局 `u32`，而 `task_id` 每窗口各自从 0 计数。现键改为 `(窗口标签, task_id)`；同时 `tokio::select!` 改为只匹配 `Ok(())`，不再把「sender 被覆盖/丢弃」误判为用户主动取消。
- **术语库非 ASCII 词条边界判定失效 (P0)**：`word_boundary` 由「首尾都是 ASCII」的合取决定，导致 `café` 命中 `caféteria` 内部被腰斩成「a 咖啡teria」。现按书写系统区分：拉丁扩展等有明确词边界的语言参与边界判定，中日韩（无空格分词，字符级无法区分「机器」与「机器学习」）按术语库既定的「全局强制匹配」语义不做边界判定。配套 4 项回归测试。
- **批量容错路径数组越界 panic (P0)**：`translate_batch_tolerant` 用主引擎返回值的下标去索引请求切片，两者长度无校验关系，主引擎返回超长数组即 `index out of bounds` panic 掉 tokio worker。现以请求为准迭代，缺失项走降级、多余项丢弃；同时「主引擎少返回项」不再整批丢弃已成功结果。
- **流式降级导致界面文本重复拼接 (P0)**：主备引擎共用同一回调，主引擎推送部分内容后失败时，备用译文会追加在残缺输出之后。现追踪主引擎是否已推送，已推送则让降级结果静默返回，由最终响应整体覆盖。
- **占位符还原被原文自我污染 (P0)**：占位符是可预测纯文本，原文含 `⟦TRANSLY_CODE_n⟧` 时会被展开成别的 stash 内容，用户原文被静默改写（技术文档讲解该机制即可触发）。现用转义隔离，与替换顺序无关。
- **未闭合代码围栏吞掉整篇文档 (P1)**：未闭合的 ` ``` ` 把其后全部内容当代码块 stash，末尾正文永久不参与翻译。现改为原样输出剩余内容（同时避免只输出 ` ``` ` 导致残留反引号被误判成行内代码）。
- **智能对转覆盖用户显式选择的目标语言 (P1)**：目标设为日语时，日文输入被改译成中文。该分支正是此前「多语言对转偏转」修复遗漏的路径。现显式非中英目标一律尊重用户选择，并以表驱动测试覆盖全部目标语言。
- **历史搜索未转义 LIKE 通配符 (P1)**：搜 `al_ha` 会误命中 `alXha`，搜 `100%pct` 中 `%` 匹配任意字符。现转义 `%` `_` `\` 并声明 `ESCAPE`。
- **HTTP 客户端超时配置被静默丢弃 (P1)**：`Client::builder().timeout(..).build().unwrap_or_default()` 在 TLS 初始化失败时退化为**无超时**客户端（reqwest 默认无超时），比配置值更危险且无日志。现改为显式告警。
- **DeepL 未知语言码静默回落中文 (P1)**：`vi` 等码会被翻译成中文且无提示。现返回明确错误。
- **`clear_cache` 未递增 generation (P1)**：用户点「清空历史」时，在途请求持有的旧 `start_gen` 仍会通过门禁把记录写回。现走 `CacheController::clear_only` 递增代际。
- **缓存写入在 async 线程执行阻塞 SQLite (P1)**：`set_if_current` 是同步函数，抢全局串行门禁后同步写库，连接设了 `busy_timeout = 5000`，单次可占用一个 tokio worker 长达 5 秒。现统一经 `write_cache_offloaded` 下沉到阻塞线程池（4 处调用点）。
- **storage 迁移按后缀独立判断，陈旧 WAL 会损坏新库 (P1)**：新目录已有主库但缺 `-wal` 时，旧 WAL 会被单独搬入；SQLite 的 WAL 不校验盐值、只校验页大小，会被回放进新库造成静默数据损坏。现三个文件作为原子单元整体判断。
- **快捷键注册失败被静默丢弃 (P1)**：`Alt+Shift+D` 极易被占用，失败后用户只看到「划词莫名其妙不工作」。现新增 `ShortcutStatusSlot` 与 `get_shortcut_status` 命令，设置页展示冲突原因。
- **快捷键「注册成功但持久化失败」状态不一致 (P1)**：新键已生效、旧键已注销，keyring 里却是旧值，重启后回退。现持久化失败时对称回滚。
- **OCR 超大位图可致内存耗尽 (P2)**：下采样前无条件全量拷贝原像素，`20000×20000` 位图即 1.6 GB。现于任何拷贝前用 `is_within_ocr_pixel_budget` 拦截（纯逻辑下沉 core 并配测试）。
- **扩展「还原原文」不穿透 Shadow DOM (P0)**：翻译侧递归进入 open shadow root，还原侧的 TreeWalker 不穿透，导致 Web Components 页面上还原**永久失效**且无恢复路径。现抽出 `shared/domTraversal.ts` 统一深度遍历（light + open shadow），三处清理全部覆盖，并修掉 `document.body.contains` 对 shadow 内节点恒为 false 导致增量监听跳过的问题。配 8 项回归测试。
- **扩展「为此站点自动翻译」永不触发 (P1)**：manifest 未申请 `tabs` 权限，无该权限且未授予 `activeTab` 时 `tab.url` 为 `undefined`，`onUpdated` 整个分支短路。现申请 `tabs`（仅读 URL）并同步隐私政策。
- **扩展增量翻译在高频 DOM 变更页面上永久饥饿 (P2)**：纯防抖被持续重置导致回调永不执行。现改为防抖 + 最大等待兜底，并在 flush 时剔除已脱离文档的节点。
- **扩展缺瞬态重试 (P2)**：服务端 429（并发令牌耗尽）/500 直接整批判失败，一次瞬时限流永久损失最多 30 个节点译文。现对 429/500/502/503 做有上限退避重试（最多 2 次），且重试受代际闸门约束。
- **扩展字符统计口径与 Rust 不一致 (P2)**：JS 用 UTF-16 code unit，Rust 用码点，emoji 被误判超长且过度切批。现统一为码点（`countChars`），并修正测试中固化错误口径的断言。
- **扩展配对探针把非 401 一律判为已配对 (P2)**：500/404/403 与网络异常都会显示「已连接」并解锁翻译按钮。现仅 2xx 算成功，其余区分「服务异常」与「未运行」。
- **扩展 400 类错误统一归为 network (P2)**：把「语言对不支持」「引擎名非法」误导成网络不可达。现按状态码细分失败类别。
- **扩展配对失败展示原始英文错误 (P2)**：中文界面弹出 `Failed to fetch`。现映射为 i18n 文案（两端对称）。
- **隐私政策与实现不符 (P0)**：政策承诺「不监听鼠标选区、不常驻注入脚本」，实际 `selection.js` 按需注入后会常驻注册 `mouseup` / `selectionchange` 以驱动划词悬浮图标。现如实改写政策（含监听用途说明与新增的 `tabs` 权限说明），并同步生效日期。
- **CHANGELOG 0.2.1 段整块逐字重复**：6 条条目各出现两次，同一版本段内分类标题也重复，导致该版本发布说明实际不可读。已去重（379 → 279 行）。


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

[0.5.0]: https://github.com/Moody-Sin/Transly/compare/v0.4.1...v0.5.0
[未发布]: https://github.com/Moody-Sin/Transly/compare/v0.5.0...HEAD
[0.4.1]: https://github.com/Moody-Sin/Transly/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/Moody-Sin/Transly/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/Moody-Sin/Transly/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/Moody-Sin/Transly/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/Moody-Sin/Transly/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/Moody-Sin/Transly/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/Moody-Sin/Transly/releases/tag/v0.1.0
