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

/// 自适应探测剪贴板内容变化。
///
/// 在快捷键模拟按键（Ctrl+C）后，剪贴板内容的更新耗时在不同应用和系统负载下差异明显（5ms~100ms+）。
/// 采用自适应短轮询代替写死固定延时：
/// 1. 快速通道：一旦读到非空且与旧内容不同的文本，立即返回，大幅降低用户体感响应延时；
/// 2. 超时兜底：若达到最大超时仍未与旧内容产生差异，则返回最后一次读到的非空文本（兼容用户所选内容与原剪贴板恰好一致的场景）；
/// 3. 若全程未读到任何非空文本，返回 None。
pub async fn poll_clipboard_change<F>(
    old_text: Option<&str>,
    mut read_fn: F,
    interval: std::time::Duration,
    timeout: std::time::Duration,
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

    last_seen
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
        )
        .await;

        assert_eq!(res, Some("identical".to_string()));
    }

    #[tokio::test]
    async fn test_poll_clipboard_change_returns_none_when_empty() {
        let reader = || Some("   ".to_string());

        let res = poll_clipboard_change(
            None,
            reader,
            std::time::Duration::from_millis(5),
            std::time::Duration::from_millis(20),
        )
        .await;

        assert_eq!(res, None);
    }
}
