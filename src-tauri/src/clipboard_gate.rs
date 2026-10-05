use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// 剪贴板内容最短触发字符数：过短内容（换行残留、单字符）不打扰用户
pub const MIN_CLIPBOARD_CHARS: usize = 2;

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
}
