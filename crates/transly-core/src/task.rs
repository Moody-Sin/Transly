use serde::{Deserialize, Serialize};

use crate::error::{Error, ErrorKind};

/// 结构化任务错误：kind 供 UI 分类，retryable 供 UI 决定是否显示重试
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskError {
    pub kind: ErrorKind,
    pub message: String,
    pub retryable: bool,
}

/// 错误类别 → 可重试判定的唯一来源。
///
/// 判定原则：只有“重试可能得到不同结果”的瞬时故障才可重试。
/// Input（文本非法）、Auth（缺密钥）重试必然同样失败。
pub fn classify_error(err: &Error) -> TaskError {
    let kind = err.kind();
    let retryable = matches!(kind, ErrorKind::Network | ErrorKind::Engine);
    TaskError {
        kind,
        message: err.to_string(),
        retryable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    #[test]
    fn test_classify_error_retryable_matrix() {
        // 可重试：瞬时故障
        let eng = classify_error(&Error::Engine {
            engine: "deepl".into(),
            message: "rate limit".into(),
        });
        assert!(eng.retryable, "Engine 限流应可重试");
        assert_eq!(eng.kind, ErrorKind::Engine);

        // 不可重试：重试同样结果
        let input = classify_error(&Error::Input("翻译文本过长".into()));
        let auth = classify_error(&Error::MissingCredentials("api_key".into()));
        assert!(!input.retryable, "Input 错误重试必然同样失败");
        assert!(!auth.retryable, "Auth 错误须先配置密钥，重试无意义");
        assert_eq!(input.kind, ErrorKind::Input);
        assert_eq!(auth.kind, ErrorKind::Auth);

        // 不可重试：数据库/内部故障需先修复底层，盲目重试无意义
        let internal = classify_error(&Error::Internal("unexpected".into()));
        assert!(!internal.retryable);
        assert_eq!(internal.kind, ErrorKind::Internal);
    }
}
