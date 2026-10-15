use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// 剪贴板内容最短触发字符数：过短内容（换行残留、单字符）不打扰用户
const MIN_CLIPBOARD_CHARS: usize = 2;

/// 剪贴板去重：与上次内容相同则不重复触发翻译
pub fn should_emit(last: &Mutex<String>, current: &str) -> bool {
    let trimmed = current.trim();
    if trimmed.chars().count() < MIN_CLIPBOARD_CHARS {
        return false;
    }
    let mut guard = match last.lock() {
        Ok(g) => g,
        Err(_) => return false,
    };
    if *guard == trimmed {
        return false;
    }
    *guard = trimmed.to_string();
    true
}

/// 划词进行中标志：模拟 Ctrl+C 期间抑制剪贴板监听自触发
pub fn is_selection_in_progress(flag: &AtomicBool) -> bool {
    flag.load(Ordering::SeqCst)
}

/// 剪贴板内容始终未变化时的兜底策略。
///
/// 同一个探测函数被两个**风险等级完全不同**的调用方共用，因此必须显式区分：
/// - 划词浮窗（`trigger_selection_translation`）只读取并展示，误判无副作用；
/// - 划词原地替换（`trigger_selection_replacement`）会在第三阶段模拟 Ctrl+V
///   覆盖用户光标处内容，误判即造成**破坏性写**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnchangedFallback {
    /// 超时后返回最后一次读到的非空文本（兼容「所选内容恰好等于旧剪贴板」的场景）。
    /// 仅适用于只读场景。
    ReturnLastSeen,
    /// 超时后返回 None（fail-closed）。
    ///
    /// 用于破坏性写场景：程序无法区分「用户没有选中任何文本」与「选中的内容
    /// 恰好等于旧剪贴板」。前者若回退返回旧剪贴板，用户误触快捷键时，
    /// 几分钟前复制的旧内容（可能是代码或密码）会被翻译并强行粘贴到光标处。
    /// 宁可漏翻译一次，也绝不能改写用户文档。
    Abort,
}

/// 自适应探测剪贴板内容变化。
///
/// 在快捷键模拟按键（Ctrl+C）后，剪贴板内容的更新耗时在不同应用和系统负载下差异明显（5ms~100ms+）。
/// 采用自适应短轮询代替写死固定延时：
/// 1. 快速通道：一旦读到非空且与旧内容不同的文本，立即返回，大幅降低用户体感响应延时；
/// 2. 超时兜底：按 `fallback` 决定返回最后读到的非空文本还是 None；
/// 3. 若全程未读到任何非空文本，返回 None。
pub async fn poll_clipboard_change<F>(
    old_text: Option<&str>,
    mut read_fn: F,
    interval: std::time::Duration,
    timeout: std::time::Duration,
    fallback: UnchangedFallback,
) -> Option<String>
where
    F: FnMut() -> Option<String>,
{
    let start = std::time::Instant::now();
    let mut last_seen = None;

    while start.elapsed() < timeout {
        tokio::time::sleep(interval).await;
        if let Some(curr) = read_fn() {
            let trimmed = curr.trim();
            if !trimmed.is_empty() {
                // 如果与旧剪贴板内容不同，说明已捕获到最新的复制文本，立即快速返回
                if old_text.map(|o| o.trim()) != Some(trimmed) {
                    return Some(curr);
                }
                last_seen = Some(curr);
            }
        }
    }

    match fallback {
        UnchangedFallback::ReturnLastSeen => last_seen,
        UnchangedFallback::Abort => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_emit_dedupes_repeated_clipboard_content() {
        let last = std::sync::Mutex::new(String::new());
        // 首次剪贴板内容应放行
        assert!(should_emit(&last, "hello"));
        // 相同内容重复出现必须被抑制（否则 500ms 轮询会反复触发同一段文本）
        assert!(!should_emit(&last, "hello"));
        // 内容变化后重新放行
        assert!(should_emit(&last, "world"));
    }

    #[test]
    fn test_should_emit_filters_short_text() {
        let last = std::sync::Mutex::new(String::new());
        assert!(!should_emit(&last, "   "), "空白剪贴板不应触发");
        assert!(!should_emit(&last, "a"), "单字符不应触发浮窗");
    }

    #[test]
    fn test_selection_flag_blocks_clipboard_listener() {
        let flag = AtomicBool::new(false);
        // 划词期间抑制剪贴板监听，避免同一段文本被翻译两次、浮窗弹两次
        assert!(!is_selection_in_progress(&flag));
        flag.store(true, Ordering::SeqCst);
        assert!(is_selection_in_progress(&flag));
        flag.store(false, Ordering::SeqCst);
        assert!(!is_selection_in_progress(&flag));
    }

    #[tokio::test]
    async fn test_poll_clipboard_change_returns_early_when_text_changes() {
        let mut count = 0;
        let reader = move || {
            count += 1;
            if count == 1 {
                Some("old text".to_string())
            } else {
                Some("new highlighted text".to_string())
            }
        };

        let res = poll_clipboard_change(
            Some("old text"),
            reader,
            std::time::Duration::from_millis(5),
            std::time::Duration::from_millis(100),
            UnchangedFallback::Abort,
        )
        .await;

        assert_eq!(res, Some("new highlighted text".to_string()));
    }

    #[tokio::test]
    async fn test_poll_clipboard_change_fallback_on_same_content_timeout() {
        let reader = || Some("identical".to_string());

        let res = poll_clipboard_change(
            Some("identical"),
            reader,
            std::time::Duration::from_millis(5),
            std::time::Duration::from_millis(25),
            UnchangedFallback::ReturnLastSeen,
        )
        .await;

        assert_eq!(res, Some("identical".to_string()));
    }

    /// 回归：破坏性写场景下剪贴板始终未变化时必须 fail-closed。
    ///
    /// 复现的缺陷：用户在编辑器里敲代码时误触 `Alt+Shift+R`，此时并未选中任何文本，
    /// 模拟 Ctrl+C 后剪贴板毫无变化。旧实现回退返回剪贴板里的旧内容（可能是几分钟前
    /// 复制的代码或密码），流程继续把它翻译并模拟 Ctrl+V 粘贴到光标处，改写用户文档。
    #[tokio::test]
    async fn test_poll_clipboard_change_aborts_on_unchanged_for_destructive_write() {
        let stale = "SECRET_TOKEN_1234567890";
        let reader = || Some(stale.to_string());

        let res = poll_clipboard_change(
            Some(stale),
            reader,
            std::time::Duration::from_millis(5),
            std::time::Duration::from_millis(25),
            UnchangedFallback::Abort,
        )
        .await;

        assert_eq!(res, None, "未捕获到新内容时必须放弃，绝不能回退旧剪贴板");
    }

    #[tokio::test]
    async fn test_poll_clipboard_change_returns_none_when_empty() {
        let reader = || Some("   ".to_string());

        let res = poll_clipboard_change(
            None,
            reader,
            std::time::Duration::from_millis(5),
            std::time::Duration::from_millis(20),
            UnchangedFallback::ReturnLastSeen,
        )
        .await;

        assert_eq!(res, None);
    }
}
