# 彻底移除自动降级（Fallback）功能设计规范

## 1. 意图与背景 (Intent & Why)

Transly 此前在商业翻译引擎（DeepL / OpenAI / DeepSeek）上设计了基于 `FallbackEngine` 的自动容灾降级机制：当主引擎网络超时或报错时，自动转由免费的 Google Web 引擎接管。

但在实际使用中，该机制产生了诸多严重缺陷：
1. **违背用户预期与静默降级**：用户配置专有 API Key 使用 DeepL 或特定 LLM 风格润色，报错时若静默由 Google 翻译接管，用户无法感知 API 额度不足或网络异常，易产生困惑。
2. **状态不一致与排障困难**：流式打字机在主引擎中断后降级，极易出现文本拼接冲突或多次请求；写入缓存的引擎 ID 与查询 ID 不一致导致缓存永久 Miss；划词替换甚至需专门加装防御补丁拦截降级。
3. **架构沉余与多层缓存**：`EnginePool` 被迫维护 `cache` 与 `raw_cache` 两套连接池与读写锁；核心库多处存在针对 `(fallback)` 的字符串嗅探与特殊逻辑。

因此，决定**彻底移除自动降级功能**，使各翻译引擎职责单一，失败时如实报错，消除静默接管。

## 2. 目标与非目标 (Goals & Non-Goals)

### 目标 (Goals)
1. **删除 `FallbackEngine`**：移除 `crates/transly-core/src/engine/fallback.rs` 及其全部单测，从 `transly-core` 根部及 `engine` 模块移除相关导出。
2. **精简 `EngineCapabilities`**：从能力结构体中彻底移除 `fallback_engine: Option<String>` 字段。
3. **精简缓存查询接口**：`lookup_cache_with_fallback` 与 `lookup_batch_with_fallback` 移除二级降级嗅探逻辑，仅严格查询当前引擎专属缓存（保留函数名以保持兼容，或平滑收敛）。
4. **精简 `EnginePool` 连接池**：移除双重缓存 `raw_cache`，所有引擎直出；`get_engine` 与 `get_raw_engine` 均返回原生引擎；主引擎报错直接向上抛出真实结构化错误。
5. **清理防御补丁**：移除 `src-tauri/src/commands/mod.rs` 划词原地替换中针对 `(fallback)` 的防御性判断。
6. **更新 UI 文案**：更新 `src/i18n/zh-CN.json` 与 `en.json`，去除 `(自动降级)` / `(auto fallback)` 标识。
7. **清理与重构测试用例**：清理测试用例中依赖降级行为的测试（如 `test_fallback_cache_hit_on_secondary`），替换为真实单引擎报错或纯直出测试。
8. **更新项目文档与规范**：同步修改 `PROJECT_RULES.md`、`crates/transly-core/README.md`、`crates/transly-core/src/engine/README.md`，并在 `CHANGELOG.md` 记录重大调整。

### 非目标 (Non-Goals)
- 保持非跨引擎的良性回退逻辑不变：
  - `src-tauri/src/storage.rs`：便携版目录不可写时降级至标准目录。
  - `extension/src/shared/richText.ts`：富文本占位符丢失时回退为原位替换。
  - `src/utils/shortcutKeys.ts`：符号快捷键解析回退。
  - 单引擎内部的重试与协议降级（如 DeepL 混杂多语种回退单项、OpenAI 网关批量失败回退单项并发）。

## 3. 架构设计与改动细节

### 3.1 `crates/transly-core`
- **删除** `src/engine/fallback.rs`。
- **修改** `src/engine/mod.rs`：
  - 移除 `pub mod fallback; pub use fallback::FallbackEngine;`
  - `EngineCapabilities`: 删除 `fallback_engine: Option<String>`。
  - `lookup_cache_with_fallback`: 仅查询 `cache.get(text, source_lang, target_lang, engine.id())`，返回 `(cached_text, engine.id().to_string())`，不再有二级降级匹配。
  - `lookup_batch_with_fallback`: 仅批量根据 primary_keys 查询 `cache.get_by_keys`，命中即返回 `(text, engine.id().to_string())`，未命中为 `None`，彻底消除 `missing_fallback_keys` 轮询。
  - `TranslationResponse::actual_engine()`: 保留该辅助方法但直接返回 `&self.engine`（或兼容器旧缓存可能残留的 `(fallback)` 历史数据），避免破坏上层调用。
- **修改** `src/lib.rs`：移除 `FallbackEngine` 的 re-export。
- **修改** `src/engine/google.rs`, `deepl.rs`, `openai.rs`：在 `capabilities()` 构造中移除 `fallback_engine` 字段。

### 3.2 `src-tauri`
- **修改** `src-tauri/src/engine_pool.rs`：
  - 移除 `raw_cache` 字段，仅保留 `cache: Arc<RwLock<HashMap<String, Arc<dyn TranslateEngine>>>>`。
  - 移除 `create_engine_blocking` 中对 `FallbackEngine::new` 的包裹逻辑，直接返回 `raw`。
  - `get_raw_engine` 直接委托给 `get_engine`。
  - 简化 `reload_proxy` 与 `invalidate`，仅需清理唯一的 `cache`。
  - 调整连接池单测。
- **修改** `src-tauri/src/commands/mod.rs`：
  - 在划词原地替换逻辑中，删除 `if res.engine.ends_with("(fallback)")` 的防御性拦截分支（因为引擎已绝不降级）。
- **修改** `src-tauri/src/server/tests.rs`：
  - 移除或重构 `test_fallback_cache_hit_on_secondary`（断言不再命中备用引擎降级缓存，而是如实按未命中/失败处理）。
  - 更新相关测试命名与断言。

### 3.3 前端与语言包
- `src/i18n/zh-CN.json`：
  - `"engineDeepl": "DeepL (需API Key)"`
  - `"engineDeepseek": "DeepSeek LLM (需API Key)"`
  - `"engineOpenai": "OpenAI (需API Key)"`
- `src/i18n/en.json`：
  - `"engineDeepl": "DeepL (API key required)"`
  - `"engineDeepseek": "DeepSeek LLM (API key required)"`
  - `"engineOpenai": "OpenAI (API key required)"`

### 3.4 文档更新
- `PROJECT_RULES.md`
- `crates/transly-core/README.md`
- `crates/transly-core/src/engine/README.md`
- `CHANGELOG.md`
