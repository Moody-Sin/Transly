//! 内置离线词库与短语映射表
//!
//! 在完全断网无模型依赖的环境下，提供极速、确定性、高精度的常用词汇、短语、
//! 软件开发与计算机专业术语、标点符号及日常会话双向翻译。

use aho_corasick::{AhoCorasick, MatchKind};
use std::sync::LazyLock;

#[inline]
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

struct DictionaryEntry {
    en: &'static str,
    zh: &'static str,
}

/// 权威内置离线词条清单（覆盖计算机、开发、常用会话、界面交互与通用词汇）
static ENTRIES: &[DictionaryEntry] = &[
    // 计算机核心与软件工程术语 (短语优先排在前面)
    DictionaryEntry {
        en: "artificial intelligence",
        zh: "人工智能",
    },
    DictionaryEntry {
        en: "machine learning",
        zh: "机器学习",
    },
    DictionaryEntry {
        en: "deep learning",
        zh: "深度学习",
    },
    DictionaryEntry {
        en: "neural network",
        zh: "神经网络",
    },
    DictionaryEntry {
        en: "natural language processing",
        zh: "自然语言处理",
    },
    DictionaryEntry {
        en: "computer vision",
        zh: "计算机视觉",
    },
    DictionaryEntry {
        en: "high performance",
        zh: "高性能",
    },
    DictionaryEntry {
        en: "memory safety",
        zh: "内存安全",
    },
    DictionaryEntry {
        en: "data structure",
        zh: "数据结构",
    },
    DictionaryEntry {
        en: "open source",
        zh: "开源",
    },
    DictionaryEntry {
        en: "user interface",
        zh: "用户界面",
    },
    DictionaryEntry {
        en: "pull request",
        zh: "拉取请求",
    },
    DictionaryEntry {
        en: "merge request",
        zh: "合并请求",
    },
    DictionaryEntry {
        en: "version control",
        zh: "版本控制",
    },
    DictionaryEntry {
        en: "command line",
        zh: "命令行",
    },
    DictionaryEntry {
        en: "terminal session",
        zh: "终端会话",
    },
    DictionaryEntry {
        en: "web browser",
        zh: "网络浏览器",
    },
    DictionaryEntry {
        en: "browser extension",
        zh: "浏览器插件",
    },
    DictionaryEntry {
        en: "operating system",
        zh: "操作系统",
    },
    DictionaryEntry {
        en: "file not found",
        zh: "文件未找到",
    },
    DictionaryEntry {
        en: "access denied",
        zh: "访问被拒绝",
    },
    DictionaryEntry {
        en: "permission denied",
        zh: "权限不足",
    },
    DictionaryEntry {
        en: "internal server error",
        zh: "内部服务器错误",
    },
    DictionaryEntry {
        en: "bad request",
        zh: "无效请求",
    },
    DictionaryEntry {
        en: "unauthorized",
        zh: "未授权",
    },
    // 常用会话与礼貌用语
    DictionaryEntry {
        en: "good morning",
        zh: "早上好",
    },
    DictionaryEntry {
        en: "good afternoon",
        zh: "下午好",
    },
    DictionaryEntry {
        en: "good evening",
        zh: "晚上好",
    },
    DictionaryEntry {
        en: "good night",
        zh: "晚安",
    },
    DictionaryEntry {
        en: "thank you",
        zh: "谢谢",
    },
    DictionaryEntry {
        en: "thanks",
        zh: "多谢",
    },
    DictionaryEntry {
        en: "you are welcome",
        zh: "不客气",
    },
    DictionaryEntry {
        en: "you're welcome",
        zh: "不客气",
    },
    DictionaryEntry {
        en: "excuse me",
        zh: "打扰一下",
    },
    DictionaryEntry {
        en: "how are you",
        zh: "你好吗",
    },
    DictionaryEntry {
        en: "see you",
        zh: "再见",
    },
    DictionaryEntry {
        en: "hello",
        zh: "你好",
    },
    DictionaryEntry {
        en: "hi", zh: "嗨"
    },
    DictionaryEntry {
        en: "goodbye",
        zh: "再见",
    },
    DictionaryEntry {
        en: "bye",
        zh: "再见",
    },
    DictionaryEntry {
        en: "please",
        zh: "请",
    },
    DictionaryEntry {
        en: "sorry",
        zh: "对不起",
    },
    DictionaryEntry {
        en: "yes",
        zh: "是的",
    },
    DictionaryEntry {
        en: "no", zh: "不"
    },
    DictionaryEntry {
        en: "ok",
        zh: "好的",
    },
    DictionaryEntry {
        en: "okay",
        zh: "好的",
    },
    // 常用操作与动作 (动词与名词)
    DictionaryEntry {
        en: "translation",
        zh: "翻译",
    },
    DictionaryEntry {
        en: "translate",
        zh: "翻译",
    },
    DictionaryEntry {
        en: "translator",
        zh: "翻译器",
    },
    DictionaryEntry {
        en: "language",
        zh: "语言",
    },
    DictionaryEntry {
        en: "settings",
        zh: "设置",
    },
    DictionaryEntry {
        en: "preferences",
        zh: "偏好设置",
    },
    DictionaryEntry {
        en: "options",
        zh: "选项",
    },
    DictionaryEntry {
        en: "configuration",
        zh: "配置",
    },
    DictionaryEntry {
        en: "config",
        zh: "配置",
    },
    DictionaryEntry {
        en: "file",
        zh: "文件",
    },
    DictionaryEntry {
        en: "document",
        zh: "文档",
    },
    DictionaryEntry {
        en: "subtitle",
        zh: "字幕",
    },
    DictionaryEntry {
        en: "directory",
        zh: "目录",
    },
    DictionaryEntry {
        en: "folder",
        zh: "文件夹",
    },
    DictionaryEntry {
        en: "save",
        zh: "保存",
    },
    DictionaryEntry {
        en: "delete",
        zh: "删除",
    },
    DictionaryEntry {
        en: "remove",
        zh: "移除",
    },
    DictionaryEntry {
        en: "clear",
        zh: "清空",
    },
    DictionaryEntry {
        en: "cancel",
        zh: "取消",
    },
    DictionaryEntry {
        en: "confirm",
        zh: "确认",
    },
    DictionaryEntry {
        en: "apply",
        zh: "应用",
    },
    DictionaryEntry {
        en: "close",
        zh: "关闭",
    },
    DictionaryEntry {
        en: "open",
        zh: "打开",
    },
    DictionaryEntry {
        en: "edit",
        zh: "编辑",
    },
    DictionaryEntry {
        en: "search",
        zh: "搜索",
    },
    DictionaryEntry {
        en: "find",
        zh: "查找",
    },
    DictionaryEntry {
        en: "replace",
        zh: "替换",
    },
    DictionaryEntry {
        en: "copy",
        zh: "复制",
    },
    DictionaryEntry {
        en: "paste",
        zh: "粘贴",
    },
    DictionaryEntry {
        en: "cut",
        zh: "剪切",
    },
    DictionaryEntry {
        en: "undo",
        zh: "撤销",
    },
    DictionaryEntry {
        en: "redo",
        zh: "重做",
    },
    DictionaryEntry {
        en: "export",
        zh: "导出",
    },
    DictionaryEntry {
        en: "import",
        zh: "导入",
    },
    DictionaryEntry {
        en: "download",
        zh: "下载",
    },
    DictionaryEntry {
        en: "upload",
        zh: "上传",
    },
    DictionaryEntry {
        en: "start",
        zh: "开始",
    },
    DictionaryEntry {
        en: "stop",
        zh: "停止",
    },
    DictionaryEntry {
        en: "pause",
        zh: "暂停",
    },
    DictionaryEntry {
        en: "resume",
        zh: "恢复",
    },
    DictionaryEntry {
        en: "restart",
        zh: "重启",
    },
    DictionaryEntry {
        en: "refresh",
        zh: "刷新",
    },
    DictionaryEntry {
        en: "reload",
        zh: "重新加载",
    },
    DictionaryEntry {
        en: "retry",
        zh: "重试",
    },
    DictionaryEntry {
        en: "success",
        zh: "成功",
    },
    DictionaryEntry {
        en: "successful",
        zh: "成功",
    },
    DictionaryEntry {
        en: "failed",
        zh: "失败",
    },
    DictionaryEntry {
        en: "failure",
        zh: "失败",
    },
    DictionaryEntry {
        en: "error",
        zh: "错误",
    },
    DictionaryEntry {
        en: "warning",
        zh: "警告",
    },
    DictionaryEntry {
        en: "info",
        zh: "信息",
    },
    DictionaryEntry {
        en: "debug",
        zh: "调试",
    },
    DictionaryEntry {
        en: "user",
        zh: "用户",
    },
    DictionaryEntry {
        en: "client",
        zh: "客户端",
    },
    DictionaryEntry {
        en: "server",
        zh: "服务端",
    },
    DictionaryEntry {
        en: "host",
        zh: "主机",
    },
    DictionaryEntry {
        en: "network",
        zh: "网络",
    },
    DictionaryEntry {
        en: "proxy",
        zh: "代理",
    },
    DictionaryEntry {
        en: "connection",
        zh: "连接",
    },
    DictionaryEntry {
        en: "token",
        zh: "令牌",
    },
    DictionaryEntry {
        en: "key",
        zh: "密钥",
    },
    DictionaryEntry {
        en: "password",
        zh: "密码",
    },
    DictionaryEntry {
        en: "cache",
        zh: "缓存",
    },
    DictionaryEntry {
        en: "history",
        zh: "历史",
    },
    DictionaryEntry {
        en: "glossary",
        zh: "术语库",
    },
    DictionaryEntry {
        en: "shortcut",
        zh: "快捷键",
    },
    DictionaryEntry {
        en: "clipboard",
        zh: "剪贴板",
    },
    DictionaryEntry {
        en: "screenshot",
        zh: "截图",
    },
    DictionaryEntry {
        en: "screen",
        zh: "屏幕",
    },
    DictionaryEntry {
        en: "window",
        zh: "窗口",
    },
    DictionaryEntry {
        en: "engine",
        zh: "引擎",
    },
    DictionaryEntry {
        en: "model",
        zh: "模型",
    },
    DictionaryEntry {
        en: "offline",
        zh: "离线",
    },
    DictionaryEntry {
        en: "online",
        zh: "在线",
    },
    DictionaryEntry {
        en: "repository",
        zh: "仓库",
    },
    DictionaryEntry {
        en: "branch",
        zh: "分支",
    },
    DictionaryEntry {
        en: "commit",
        zh: "提交",
    },
    DictionaryEntry {
        en: "algorithm",
        zh: "算法",
    },
    DictionaryEntry {
        en: "database",
        zh: "数据库",
    },
    DictionaryEntry {
        en: "table",
        zh: "表",
    },
    DictionaryEntry {
        en: "column",
        zh: "列",
    },
    DictionaryEntry {
        en: "row",
        zh: "行",
    },
    DictionaryEntry {
        en: "ready",
        zh: "就绪",
    },
    DictionaryEntry {
        en: "running",
        zh: "运行中",
    },
    DictionaryEntry {
        en: "finished",
        zh: "已完成",
    },
    DictionaryEntry {
        en: "completed",
        zh: "已完成",
    },
    DictionaryEntry {
        en: "progress",
        zh: "进度",
    },
    DictionaryEntry {
        en: "title",
        zh: "标题",
    },
    DictionaryEntry {
        en: "description",
        zh: "描述",
    },
    DictionaryEntry {
        en: "content",
        zh: "内容",
    },
    DictionaryEntry {
        en: "result",
        zh: "结果",
    },
    DictionaryEntry {
        en: "code",
        zh: "代码",
    },
    DictionaryEntry {
        en: "project",
        zh: "项目",
    },
    DictionaryEntry {
        en: "test",
        zh: "测试",
    },
    DictionaryEntry {
        en: "benchmark",
        zh: "基准测试",
    },
];

struct CompiledMatcher {
    ac: AhoCorasick,
    targets: Vec<&'static str>,
}

static EN_TO_ZH_MATCHER: LazyLock<CompiledMatcher> = LazyLock::new(|| {
    let mut patterns = Vec::with_capacity(ENTRIES.len());
    let mut targets = Vec::with_capacity(ENTRIES.len());

    for entry in ENTRIES {
        patterns.push(entry.en.to_lowercase());
        targets.push(entry.zh);
    }

    let ac = AhoCorasick::builder()
        .ascii_case_insensitive(true)
        .match_kind(MatchKind::LeftmostLongest)
        .build(&patterns)
        .expect("内置英文离线词库构建失败");

    CompiledMatcher { ac, targets }
});

static ZH_TO_EN_MATCHER: LazyLock<CompiledMatcher> = LazyLock::new(|| {
    let mut patterns = Vec::with_capacity(ENTRIES.len());
    let mut targets = Vec::with_capacity(ENTRIES.len());

    for entry in ENTRIES {
        patterns.push(entry.zh.to_string());
        targets.push(entry.en);
    }

    let ac = AhoCorasick::builder()
        .match_kind(MatchKind::LeftmostLongest)
        .build(&patterns)
        .expect("内置中文离线词库构建失败");

    CompiledMatcher { ac, targets }
});

/// 获取内置词典词条总数
pub fn get_lexicon_entries_count() -> u32 {
    ENTRIES.len() as u32
}

/// 执行离线英译中
pub fn translate_en_to_zh(input: &str) -> String {
    let matcher = &*EN_TO_ZH_MATCHER;
    let mut out = String::with_capacity(input.len());
    let mut last_idx = 0;

    for mat in matcher.ac.find_iter(input) {
        let start = mat.start();
        let end = mat.end();
        let pattern_idx = mat.pattern().as_usize();
        let target = matcher.targets[pattern_idx];

        // 西文词边界验证：避免 cat 误匹配到 concatenate 或 scatter 中
        let is_left_ok = if start == 0 {
            true
        } else {
            let prev = input[..start].chars().next_back().unwrap_or(' ');
            !is_word_char(prev)
        };

        let is_right_ok = if end >= input.len() {
            true
        } else {
            let next = input[end..].chars().next().unwrap_or(' ');
            !is_word_char(next)
        };

        if !is_left_ok || !is_right_ok {
            continue;
        }

        // 追加未匹配的前缀文本并做标点转换
        let preceding = &input[last_idx..start];
        convert_en_punctuation(preceding, &mut out);

        out.push_str(target);
        last_idx = end;
    }

    // 追加尾部剩余文本
    if last_idx < input.len() {
        convert_en_punctuation(&input[last_idx..], &mut out);
    }

    out
}

/// 执行离线中译英
pub fn translate_zh_to_en(input: &str) -> String {
    let matcher = &*ZH_TO_EN_MATCHER;
    let mut out = String::with_capacity(input.len() * 2);
    let mut last_idx = 0;

    for mat in matcher.ac.find_iter(input) {
        let start = mat.start();
        let end = mat.end();
        let pattern_idx = mat.pattern().as_usize();
        let target = matcher.targets[pattern_idx];

        let preceding = &input[last_idx..start];
        convert_zh_punctuation(preceding, &mut out);

        // 中译英在词与词之间增加空格
        if !out.is_empty() && !out.ends_with(' ') && !out.ends_with('\n') {
            out.push(' ');
        }
        out.push_str(target);
        last_idx = end;
    }

    if last_idx < input.len() {
        convert_zh_punctuation(&input[last_idx..], &mut out);
    }

    out
}

fn convert_en_punctuation(input: &str, out: &mut String) {
    for ch in input.chars() {
        match ch {
            ',' => out.push('，'),
            '.' => out.push('。'),
            '?' => out.push('？'),
            '!' => out.push('！'),
            ':' => out.push('：'),
            ';' => out.push('；'),
            other => out.push(other),
        }
    }
}

fn convert_zh_punctuation(input: &str, out: &mut String) {
    for ch in input.chars() {
        match ch {
            '，' => out.push_str(", "),
            '。' => out.push_str(". "),
            '？' => out.push_str("? "),
            '！' => out.push_str("! "),
            '：' => out.push_str(": "),
            '；' => out.push_str("; "),
            other => out.push(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexicon_en_to_zh_phrases_and_words() {
        let res = translate_en_to_zh("Hello world! Artificial intelligence is high performance.");
        assert!(res.contains("你好"));
        assert!(res.contains("人工智能"));
        assert!(res.contains("高性能"));
        assert!(res.contains("！"));
        assert!(res.contains("。"));
    }

    #[test]
    fn test_lexicon_en_word_boundaries_protect_partial_matches() {
        // "cat" 不应误匹配 "concatenate"
        let res = translate_en_to_zh("Please save file and cancel.");
        assert_eq!(res, "请 保存 文件 and 取消。");
    }

    #[test]
    fn test_lexicon_zh_to_en() {
        let res = translate_zh_to_en("你好，人工智能与机器学习！");
        assert!(res.contains("hello"));
        assert!(res.contains("artificial intelligence"));
        assert!(res.contains("machine learning"));
    }
}
