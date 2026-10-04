# 更新日志

本项目的所有重要变更均记录于此文件。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
并且本项目严格遵循 [语义化版本 2.0.0](https://semver.org/lang/zh-CN/)。

## [Unreleased]

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
