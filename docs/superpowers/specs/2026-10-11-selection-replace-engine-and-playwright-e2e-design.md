# 划词原地替换首选引擎与浏览器插件端到端集成测试设计

**日期**：2026-10-11
**作者**：Transly 架构组
**状态**：设计中 (Review)

---

## 1. 概述与背景

Transly 聚焦轻量极速的桌面与沉浸式整页翻译。在本次迭代中，我们将推进两个关键优化：
1. **划词原地替换首选引擎配置与直接报错提醒**：
   当前快捷键划词原地替换（默认 `Alt+Shift+R`）硬编码使用 `GoogleWebEngine`。为满足高级用户对大模型润色（DeepSeek / OpenAI）及专业术语（DeepL）的高质量替换诉求，系统需支持在设置中配置首选替换引擎。当所选引擎因未配置密钥、额度耗尽或网络故障失败时，**严格直接报错提醒**，禁止静默隐蔽降级，并安全保护剪贴板。
2. **端到端 Playwright 浏览器插件真实无头集成测试**：
   当前扩展仅有 Node.js 内存 mock 单测。为保障在真实浏览器（Chromium）下 DOM 树提取、Shadow DOM 穿透、富文本闭合占位、双语对照兄弟节点追加以及一键还原的绝对保真性，引入真实的 Headless 插件加载集成测试套件。

---

## 2. 划词原地替换首选引擎设计

### 2.1 存储与命令设计
- **配置持久化**：
  - 存储键名：`preferred_replace_engine`（保存在系统原生凭据管理器 `keyring` 中）。
  - 合法值：`google` (默认值)、`deepl`、`deepseek`、`openai`。
- **IPC 命令契约 (`src-tauri/src/commands/mod.rs`)**：
  ```rust
  #[tauri::command]
  #[specta::specta]
  pub async fn get_preferred_replace_engine() -> Result<String, String>;

  #[tauri::command]
  #[specta::specta]
  pub async fn save_preferred_replace_engine(engine: String) -> Result<(), String>;
  ```
  - 同步导出至 `src/bindings.ts`。

### 2.2 替换执行流程与失败报错策略
在 `trigger_selection_replacement` 工作流中：
1. **阶段一（原子模拟复制）**：
   - 保持现有 `CLIPBOARD_SIMULATION_MUTEX` 互斥与 `sel_flag` 全程保护，自适应轮询获取当前选区文本 `clean`。
2. **阶段二（异步准备与首选引擎调度）**：
   - 读取 `preferred_replace_engine`（未配置或异常时回退 `"google"`）。
   - 校验引擎合法性并获取引擎实例：`let active_engine = deps.pool.get_engine(&preferred).await;`。
   - 凭据前置校验：若用户选择了商业引擎（DeepL/OpenAI/DeepSeek）但未配置有效 Key（非本地端点）：
     - 立即触发失败报错通知；
     - **安全退出，绝不执行静默降级，绝不改写剪贴板**。
   - 缓存优先查询：基于 `transly_core::lookup_cache_with_fallback` 查询已有历史记录。
   - 引擎翻译：
     - 若缓存未命中，调用 `active_engine.translate(&req)`；
     - 若翻译返回 `Err(e)`：
       - 通过 Tauri 事件广播 `transly://replacement-failed`（携带错误信息与引擎名称）；
       - 前端弹出瞬时 Toast 错误卡片（例如：“划词原地替换失败 [DeepSeek]：API Key 不能为空”或网络错误）；
       - 记录 `eprintln!` 错误日志并立即终止，原剪贴板内容自动保留。
   - 翻译成功后：
     - 将译文与实际引擎标识（`res.actual_engine()`）写入 SQLite 缓存；
     - 应用全局术语库规则 `g.apply(&restored)`。
3. **阶段三（模拟粘贴与剪贴板恢复）**：
   - 将最终译文写入系统剪贴板，发送 `Ctrl+V` (Cmd+V) 覆盖选区；
   - 延迟 350ms 等待宿主软件消费；
   - 校验剪贴板内容，安全还原用户原始剪贴板。

### 2.3 前端 UI 交互设计
- 在设置中心（`src/App.tsx`）的“常规设置”或“全局快捷键”配置区增加：
  - **标签**：`划词原地替换引擎` (Preferred In-Place Replacement Engine)
  - **下拉选项**：
    - `Google 网页翻译 (免Key)`
    - `DeepL 翻译`
    - `DeepSeek 深度求索`
    - `OpenAI 智能翻译`
  - 切换时自动持久化保存；
  - 监听 `transly://replacement-failed` 事件，弹出显式错误 Toast。
- 同步补齐 `src/i18n/zh-CN.json` 与 `src/i18n/en.json` 的国际化条目。

---

## 3. 浏览器扩展 Playwright 端到端集成测试设计

### 3.1 目标与架构
- **测试环境**：
  - 使用 Node.js 的 `@playwright/test` 或轻量 `playwright` 驱动 Chromium 启动。
  - 启动参数配置：
    ```ts
    const context = await chromium.launchPersistentContext(userDataDir, {
      headless: true, // 或 'new'
      args: [
        `--disable-extensions-except=${extensionDistPath}`,
        `--load-extension=${extensionDistPath}`,
      ],
    });
    ```
- **桩服务 (Mock Server)**：
  - 启动内置微型 HTTP 测试服务器（端口动态或固定，例如 `127.0.0.1:39281`），为扩展提供标准的 `/health`、`/translate` 与 `/verify_token` 响应；
  - 同时托管包含标准测试场景的 HTML 页面（纯文本、带标签富文本、Open Shadow DOM Web Components、多段落混合）。

### 3.2 覆盖的核心用例集合
1. **用例 1: 整页仅译文原地 DOM 替换**：
   - 网页加载后，点击“翻译此网页”，断言 HTML 内部文本节点在原位被替换为目标译文；结构标签（`div`, `p`, `span`）未被破坏。
2. **用例 2: 富文本段落标签保真性**：
   - 段落内含有 `<a>` 链接与 `<strong>` 加粗；翻译后链接 `href` 保持不变，文本结构按译文顺序精准嵌套。
3. **用例 3: 开放模式 Shadow DOM 穿透**：
   - 自定义 Web Component 内部的 `<template>` 内容被成功遍历并原地替换译文。
4. **用例 4: 双语对照模式与一键还原**：
   - 切换至双语模式翻译，断言原文节点保留，其后生成 `.transly-bilingual-trans` 兄弟节点；
   - 点击“还原原文”，断言兄弟节点完全被移除，文本内容恢复为初始值。

---

## 4. 验证计划与风险控制

1. **Rust 门禁**：
   - `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace` 全绿；
   - 确保 `get_preferred_replace_engine` 与 `save_preferred_replace_engine` 单测覆盖。
2. **前端与扩展构建**：
   - `pnpm run generate:bindings` 确认前后端强类型绑定零漂移；
   - `pnpm run lint` + `pnpm run test:task` + `pnpm run build` 全绿；
   - `pnpm --filter transly-extension run build` 扩展产物构建就绪。
3. **E2E 真实运行验证**：
   - 运行 Playwright 扩展集成测试，验证无头 Chromium 下真实 DOM 替换流程通过。
