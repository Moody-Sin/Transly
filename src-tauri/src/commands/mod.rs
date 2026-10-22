//! Tauri command 接入层（薄封装）。
//!
//! 本模块只做三件事：参数校验、`spawn_blocking` 下沉阻塞 I/O、错误分类映射。
//! 翻译/缓存/OCR/术语库等实际逻辑一律在 `transly-core`。
//!
//! 命令按领域拆到同级子模块，`mod.rs` 统一 `pub use` 重导出，
//! 目的是让 `app_command_builder!` 里的 `$crate::commands::xxx` 路径保持不变
//! （改动路径会连带影响 `src/bindings.ts` 的生成结果与 CI 的绑定同步校验）。
//!
//! | 子模块 | 领域 |
//! |---|---|
//! | `translate` | 单条/流式翻译、流式任务取消注册表 |
//! | `glossary` | 术语库增删改查与批量导入 |
//! | `history` | 翻译历史分页/检索/删除/导出、缓存开关 |
//! | `settings` | 剪贴板监听、剪贴板图片 OCR、全局快捷键录制、语言检测 |
//! | `float` | 浮窗请求队列与粘滞状态 |
//! | `server` | 本地服务 Token、扩展配对、服务运行状态 |
//! | `shortcut_status` | 三个快捷键注册状态的聚合视图 |
//! | `llm` | LLM 配置、API Key、引擎能力、网络代理 |
//!
//! 划词与截图两组命令分别在同级 `selection.rs` / `screenshot.rs`。

pub mod screenshot;
pub mod selection;

mod float;
mod glossary;
mod history;
mod llm;
mod server;
mod settings;
mod shortcut_status;
mod translate;

pub use float::*;
pub use glossary::*;
pub use history::*;
pub use llm::*;
pub use server::*;
pub use settings::*;
pub use shortcut_status::*;
pub use translate::*;

// 划词/浮窗相关符号在 `commands::selection`，此处重导出：
// `app_command_builder!` 与 `main.rs` 的既有引用路径保持不变，避免连带改动。
pub use selection::{
    place_float_near_cursor, trigger_selection_replacement, trigger_selection_translation,
    ReplacementFailedPayload, SelectionContext, DEFAULT_REPLACE_SHORTCUT, DEFAULT_SHORTCUT,
};

#[cfg(test)]
mod tests {
    use super::*;
    // 拆分后测试不再自动继承 mod.rs 头部的 use，逐项补齐。
    use crate::engine_pool::EnginePool;
    use std::sync::Arc;
    use transly_core::{Error, ErrorKind, ProxyConfig, ProxyMode, TranslationCache};

    use super::float::PENDING_FLOAT_CAPACITY;

    fn req(text: &str, source: &'static str, id: u64) -> FloatRequest {
        FloatRequest {
            text: text.to_string(),
            source,
            request_id: id,
        }
    }

    #[test]
    fn test_pending_float_requests_are_fifo_not_overwritten() {
        // 回归：单槽实现下，划词与剪贴板两条链路的请求会互相覆盖，
        // 先到的请求静默丢失，用户表现为“第一次划词没反应”。
        let pending = PendingFloatRequests::default();
        assert!(!pending.push(req("first", "selection", 1)));
        assert!(!pending.push(req("second", "clipboard", 2)));

        let queue = pending.0.lock().unwrap();
        assert_eq!(queue.len(), 2, "两条请求都必须保留");
        assert_eq!(queue[0].text, "first", "必须保持先进先出顺序");
        assert_eq!(queue[1].text, "second");
    }

    #[test]
    fn test_pending_float_requests_bounded_capacity_drops_oldest() {
        let pending = PendingFloatRequests::default();
        for i in 0..PENDING_FLOAT_CAPACITY {
            assert!(!pending.push(req(&format!("t{i}"), "clipboard", i as u64)));
        }
        // 第 CAPACITY+1 条：应丢弃最旧项并上报丢弃
        assert!(pending.push(req("overflow", "clipboard", 999)));

        let queue = pending.0.lock().unwrap();
        assert_eq!(queue.len(), PENDING_FLOAT_CAPACITY, "容量必须有界");
        assert_eq!(queue.front().unwrap().text, "t1", "丢弃的是最旧项");
        assert_eq!(queue.back().unwrap().text, "overflow");
    }

    #[test]
    fn test_translate_error_dto_serializes_structured_kind() {
        let dto = TranslateErrorDto::from_error(&Error::Engine {
            engine: "deepl".into(),
            message: "rate limit".into(),
        });
        let json = serde_json::to_string(&dto).unwrap();
        // 前端按此 key 分类，禁止退化为裸字符串
        assert!(json.contains(r#""kind":"engine""#));
        assert!(json.contains(r#""retryable":true"#));
        // 不得泄露密钥或 Authorization 头
        assert!(!json.to_lowercase().contains("authorization"));
        assert!(!json.to_lowercase().contains("api_key\":\""));
    }

    #[test]
    fn test_translate_error_dto_auth_not_retryable() {
        let dto = TranslateErrorDto::from_error(&Error::MissingCredentials("deepl key".into()));
        assert_eq!(dto.kind, ErrorKind::Auth);
        assert!(!dto.retryable, "缺密钥时重试无意义，不得提供重试入口");
    }

    #[test]
    fn test_translate_error_dto_input_not_retryable() {
        let dto = TranslateErrorDto::from_error(&Error::Input("翻译文本过长".into()));
        assert_eq!(dto.kind, ErrorKind::Input);
        assert!(!dto.retryable);
    }

    #[tokio::test]
    async fn test_pairing_commands_approve_and_reject() {
        let manager = Arc::new(crate::server::pairing::PairingManager::new());
        let s1 = match manager.create_session("client-1".into(), None).await {
            crate::server::pairing::CreateSessionOutcome::Created(id, _) => id,
            o => panic!("{o:?}"),
        };
        let s2 = match manager.create_session("client-2".into(), None).await {
            crate::server::pairing::CreateSessionOutcome::Created(id, _) => id,
            o => panic!("{o:?}"),
        };

        // approve 测试
        assert!(manager.approve(&s1).await);
        // 重复 approve 应失败
        assert!(!manager.approve(&s1).await);

        // reject 测试
        assert!(manager.reject(&s2).await);
        // 重复 reject 应失败
        assert!(!manager.reject(&s2).await);
    }

    #[tokio::test]
    async fn test_detect_text_language_command() {
        let res = detect_text_language("你好世界".to_string()).await;
        assert_eq!(res, Ok(Some("zh-CN".to_string())));

        let res_en = detect_text_language("Hello world".to_string()).await;
        assert_eq!(res_en, Ok(Some("en".to_string())));

        let res_none = detect_text_language("".to_string()).await;
        assert_eq!(res_none, Ok(None));
    }

    #[test]
    fn test_shortcut_status_slot_transitions() {
        let slot = ShortcutStatusSlot::default();
        let initial = slot.get();
        assert!(initial.ok);
        assert_eq!(initial.translate_error, None);
        assert_eq!(initial.replace_error, None);

        slot.set_translate_failed("Ctrl+Shift+T: conflict".to_string());
        let failed = slot.get();
        assert!(!failed.ok);
        assert_eq!(
            failed.translate_error.as_deref(),
            Some("Ctrl+Shift+T: conflict")
        );
        assert_eq!(failed.replace_error, None);

        // 原地替换注册成功不应冲掉划词快捷键的失败
        slot.set_replace_ok();
        let still_failed = slot.get();
        assert!(!still_failed.ok);
        assert_eq!(
            still_failed.translate_error.as_deref(),
            Some("Ctrl+Shift+T: conflict")
        );

        // 原地替换也失败时应独立保留两者的原因并聚合 reason
        slot.set_replace_failed("Ctrl+Shift+X: conflict".to_string());
        let both_failed = slot.get();
        assert!(!both_failed.ok);
        assert_eq!(
            both_failed.translate_error.as_deref(),
            Some("Ctrl+Shift+T: conflict")
        );
        assert_eq!(
            both_failed.replace_error.as_deref(),
            Some("Ctrl+Shift+X: conflict")
        );
        assert_eq!(
            both_failed.reason.as_deref(),
            Some("Ctrl+Shift+T: conflict; Ctrl+Shift+X: conflict")
        );

        // 划词修复后只留原地替换失败
        slot.set_translate_ok();
        let replace_only = slot.get();
        assert!(!replace_only.ok);
        assert_eq!(replace_only.translate_error, None);
        assert_eq!(
            replace_only.replace_error.as_deref(),
            Some("Ctrl+Shift+X: conflict")
        );

        slot.set_replace_ok();
        let ok = slot.get();
        assert!(ok.ok);
        assert_eq!(ok.reason, None);
        assert_eq!(ok.translate_error, None);
        assert_eq!(ok.replace_error, None);
    }

    /// 关键回归：截图快捷键（第三槽）必须独立于另两个槽位。
    ///
    /// 槽位从 2 个加到 3 个时，`reason` 若用两两 match 聚合就会漏掉第三槽；
    /// 且「新增一个失败不得影响既有两个的状态」是产品硬要求
    ///（三张设置卡片各自显示自己的冲突提示）。
    #[test]
    fn test_shortcut_status_slot_third_slot_is_independent() {
        let slot = ShortcutStatusSlot::default();
        assert!(slot.get().ok);

        // 只截图快捷键失败：另两个必须仍为 None
        slot.set_screenshot_failed("Alt+Shift+S: conflict".to_string());
        let only_shot = slot.get();
        assert!(!only_shot.ok);
        assert_eq!(
            only_shot.screenshot_error.as_deref(),
            Some("Alt+Shift+S: conflict")
        );
        assert_eq!(only_shot.translate_error, None);
        assert_eq!(only_shot.replace_error, None);
        // reason 必须包含第三槽（两两 match 的实现在此会得到 None）
        assert_eq!(only_shot.reason.as_deref(), Some("Alt+Shift+S: conflict"));

        // 三个都失败：reason 必须聚合全部三条，不丢信息
        slot.set_translate_failed("t".to_string());
        slot.set_replace_failed("r".to_string());
        let all = slot.get();
        assert_eq!(all.reason.as_deref(), Some("t; r; Alt+Shift+S: conflict"));

        // 截图槽修好不应影响另两个；反之亦然
        slot.set_screenshot_ok();
        let two = slot.get();
        assert_eq!(two.screenshot_error, None);
        assert_eq!(two.translate_error.as_deref(), Some("t"));
        assert_eq!(two.replace_error.as_deref(), Some("r"));
        assert_eq!(two.reason.as_deref(), Some("t; r"));
    }

    #[test]
    fn test_save_preferred_replace_engine_validation() {
        assert!(validate_preferred_replace_engine("invalid_eng").is_err());
        assert!(validate_preferred_replace_engine("").is_err());
        assert_eq!(
            validate_preferred_replace_engine("google"),
            Ok("google".to_string())
        );
        assert_eq!(
            validate_preferred_replace_engine("google_web"),
            Ok("google".to_string())
        );
        assert_eq!(
            validate_preferred_replace_engine("deepl"),
            Ok("deepl".to_string())
        );
        assert_eq!(
            validate_preferred_replace_engine("deepseek"),
            Ok("deepseek".to_string())
        );
        assert_eq!(
            validate_preferred_replace_engine("openai"),
            Ok("openai".to_string())
        );
        assert_eq!(
            validate_preferred_replace_engine("offline"),
            Ok("offline".to_string())
        );
    }

    #[tokio::test]
    async fn test_preferred_target_lang_persistence() {
        let dir = std::env::temp_dir().join(format!("transly-lang-test-{}", rand::random::<u64>()));
        let cache = TranslationCache::open_default(&dir);

        // 未配置时默认 None（调用处回退 zh-CN）
        assert_eq!(cache.get_setting("preferred_target_lang").unwrap(), None);

        // 非法语言代码校验
        assert!(!transly_core::validate_language_code("invalid-lang"));

        // 保存合法语言代码
        cache.set_setting("preferred_target_lang", "en").unwrap();
        assert_eq!(
            cache
                .get_setting("preferred_target_lang")
                .unwrap()
                .as_deref(),
            Some("en")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_proxy_config_save_and_get() {
        let dir =
            std::env::temp_dir().join(format!("transly-proxy-test-{}", rand::random::<u64>()));
        let cache = TranslationCache::open_default(&dir);
        let pool = Arc::new(EnginePool::with_cache(cache.clone()));

        // 默认应为 auto
        let initial = cache.get_setting("proxy_mode").unwrap();
        assert_eq!(initial, None);

        // 验证手动代理没有 URL 时被拒绝
        let invalid_manual = ProxyConfig {
            mode: ProxyMode::Manual,
            url: None,
        };
        assert!(validate_proxy_config(&invalid_manual).is_err());
        assert!(
            save_proxy_config_internal(invalid_manual, &pool, cache.clone())
                .await
                .is_err()
        );

        // 保存合法手动代理
        let valid_manual = ProxyConfig {
            mode: ProxyMode::Manual,
            url: Some("127.0.0.1:7890".to_string()),
        };
        assert!(validate_proxy_config(&valid_manual).is_ok());
        assert!(
            save_proxy_config_internal(valid_manual, &pool, cache.clone())
                .await
                .is_ok()
        );

        assert_eq!(
            cache.get_setting("proxy_mode").unwrap().as_deref(),
            Some("manual")
        );
        assert_eq!(
            cache.get_setting("proxy_url").unwrap().as_deref(),
            Some("127.0.0.1:7890")
        );

        let retrieved = get_proxy_config_internal(&cache).unwrap();
        assert_eq!(retrieved.mode, ProxyMode::Manual);
        assert_eq!(retrieved.url.as_deref(), Some("127.0.0.1:7890"));

        // 切换为 none
        let none_cfg = ProxyConfig {
            mode: ProxyMode::None,
            url: None,
        };
        assert!(validate_proxy_config(&none_cfg).is_ok());
        assert!(save_proxy_config_internal(none_cfg, &pool, cache.clone())
            .await
            .is_ok());

        assert_eq!(
            cache.get_setting("proxy_mode").unwrap().as_deref(),
            Some("none")
        );
        assert_eq!(cache.get_setting("proxy_url").unwrap(), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_is_local_endpoint_support_for_llm_engines() {
        assert!(transly_core::OpenAIEngine::is_local_endpoint(
            "http://localhost:11434/v1"
        ));
        assert!(transly_core::OpenAIEngine::is_local_endpoint(
            "http://127.0.0.1:11434/v1"
        ));
        assert!(transly_core::OpenAIEngine::is_local_endpoint(
            "http://[::1]:11434/v1"
        ));
        assert!(!transly_core::OpenAIEngine::is_local_endpoint(
            "https://api.deepseek.com"
        ));
    }
}

#[cfg(test)]
mod cancel_semantics_tests {
    /// 回归：取消信号的三种语义必须严格区分。
    ///
    /// 复现的缺陷：`cancel_rx` 返回 `Err(RecvError)` 的时机是「同键 sender 被
    /// 覆盖/丢弃」，属注册表内部事件；旧实现却在该分支直接返回
    /// 「流式任务的取消通道已失效」，把一次本可正常完成的翻译变成伪错误，
    /// 与其相邻注释声明的语义自相矛盾。
    ///
    /// 现在该情形让取消分支永久挂起，select 完全由引擎结果决定。
    /// 本测试覆盖三种组合，防止后人「顺手简化」时改坏这段语义。
    async fn run_cancel_select(drop_sender: bool, engine_delay_ms: u64) -> &'static str {
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        if drop_sender {
            drop(tx);
        } else {
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                let _ = tx.send(());
            });
        }
        let engine = async move {
            tokio::time::sleep(std::time::Duration::from_millis(engine_delay_ms)).await;
            "engine_result"
        };
        let cancel_signal = async move {
            match rx.await {
                Ok(()) => {}
                Err(_) => std::future::pending::<()>().await,
            }
        };
        tokio::select! {
            r = engine => r,
            _ = cancel_signal => "cancelled",
        }
    }

    #[tokio::test]
    async fn test_cancel_signal_distinguishes_user_cancel_from_channel_drop() {
        assert_eq!(
            run_cancel_select(true, 50).await,
            "engine_result",
            "sender 析构（同键任务覆盖）不得被当成用户取消，必须等引擎返回真实结果"
        );
        assert_eq!(
            run_cancel_select(false, 500).await,
            "cancelled",
            "用户主动取消必须立即生效，不能等引擎跑完"
        );
        assert_eq!(
            run_cancel_select(false, 5).await,
            "engine_result",
            "引擎先返回时应正常完成"
        );
    }
}
