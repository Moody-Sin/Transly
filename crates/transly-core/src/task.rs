use serde::{Deserialize, Serialize};

use crate::error::{Error, ErrorKind};

/// 跨入口共享的任务状态（仅数据；任务实例与生命周期由宿主或入口适配层持有）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Idle,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl TaskState {
    /// 结束状态：不再接受旧结果回流
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            TaskState::Succeeded | TaskState::Failed | TaskState::Cancelled
        )
    }

    /// 活动状态：存在在途请求
    pub fn is_active(self) -> bool {
        matches!(self, TaskState::Running)
    }
}

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
    let retryable = matches!(
        kind,
        ErrorKind::Network | ErrorKind::Timeout | ErrorKind::Engine
    );
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
    fn test_task_state_serde() {
        assert_eq!(
            serde_json::to_string(&TaskState::Cancelled).unwrap(),
            "\"cancelled\""
        );
        let back: TaskState = serde_json::from_str("\"running\"").unwrap();
        assert_eq!(back, TaskState::Running);
    }

    #[test]
    fn test_classify_error_retryable_matrix() {
        // 可重试：瞬时故障
        let eng = classify_error(&Error::Engine {
            engine: "deepl".into(),
            message: "rate limit".into(),
        });
        let timeout = classify_error(&Error::Timeout("request timed out".into()));
        assert!(eng.retryable, "Engine 限流应可重试");
        assert!(timeout.retryable, "Timeout 应可重试");
        assert_eq!(eng.kind, ErrorKind::Engine);
        assert_eq!(timeout.kind, ErrorKind::Timeout);

        // 不可重试：重试同样结果
        let input = classify_error(&Error::Input("翻译文本过长".into()));
        let auth = classify_error(&Error::MissingCredentials("api_key".into()));
        assert!(!input.retryable, "Input 错误重试必然同样失败");
        assert!(!auth.retryable, "Auth 错误须先配置密钥，重试无意义");
        assert_eq!(input.kind, ErrorKind::Input);
        assert_eq!(auth.kind, ErrorKind::Auth);
    }

    #[test]
    fn test_terminal_and_active_states() {
        assert!(TaskState::Succeeded.is_terminal());
        assert!(TaskState::Failed.is_terminal());
        assert!(TaskState::Cancelled.is_terminal());
        assert!(!TaskState::Running.is_terminal());
        assert!(TaskState::Running.is_active());
        assert!(!TaskState::Idle.is_active());
    }
}
