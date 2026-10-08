use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

/// 错误分类。
///
/// 分类集合与前端 `ErrorKind` 字面量联合一一对应（经 specta 导出）；
/// 每个变体都必须有真实构造路径，否则应随构造方一并删除。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Input,
    Auth,
    Network,
    Engine,
    Internal,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("输入错误: {0}")]
    Input(String),

    #[error("网络请求错误: {0}")]
    Network(#[from] reqwest::Error),

    #[error("缓存/数据库错误: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("序列化错误: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("翻译引擎错误 [{engine}]: {message}")]
    Engine { engine: String, message: String },

    #[error("未配置密钥或参数: {0}")]
    MissingCredentials(String),

    #[error("内部错误: {0}")]
    Internal(String),
}

impl Error {
    pub fn kind(&self) -> ErrorKind {
        match self {
            Error::Input(_) => ErrorKind::Input,
            Error::MissingCredentials(_) => ErrorKind::Auth,
            Error::Network(_) => ErrorKind::Network,
            Error::Engine { .. } => ErrorKind::Engine,
            Error::Database(_) | Error::Serialization(_) | Error::Internal(_) => {
                ErrorKind::Internal
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_kind_mapping() {
        let err_input = Error::Input("翻译文本过长".into());
        assert_eq!(err_input.kind(), ErrorKind::Input);

        let err_auth = Error::MissingCredentials("api_key".into());
        assert_eq!(err_auth.kind(), ErrorKind::Auth);

        let err_engine = Error::Engine {
            engine: "deepl".into(),
            message: "rate limit".into(),
        };
        assert_eq!(err_engine.kind(), ErrorKind::Engine);

        let err_internal = Error::Internal("unexpected".into());
        assert_eq!(err_internal.kind(), ErrorKind::Internal);
    }
}
