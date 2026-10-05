use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Input,
    Auth,
    Network,
    Engine,
    Timeout,
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

    #[error("语言未支持: {0}")]
    UnsupportedLanguage(String),

    #[error("未配置密钥或参数: {0}")]
    MissingCredentials(String),

    #[error("请求超时: {0}")]
    Timeout(String),

    #[error("内部错误: {0}")]
    Internal(String),
}

impl Error {
    pub fn kind(&self) -> ErrorKind {
        match self {
            Error::Input(_) | Error::UnsupportedLanguage(_) => ErrorKind::Input,
            Error::MissingCredentials(_) => ErrorKind::Auth,
            Error::Network(_) => ErrorKind::Network,
            Error::Engine { .. } => ErrorKind::Engine,
            Error::Timeout(_) => ErrorKind::Timeout,
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
        let err_input = Error::UnsupportedLanguage("invalid_lang".into());
        assert_eq!(err_input.kind(), ErrorKind::Input);

        let err_auth = Error::MissingCredentials("api_key".into());
        assert_eq!(err_auth.kind(), ErrorKind::Auth);

        let err_engine = Error::Engine {
            engine: "deepl".into(),
            message: "rate limit".into(),
        };
        assert_eq!(err_engine.kind(), ErrorKind::Engine);

        let err_timeout = Error::Timeout("request timed out".into());
        assert_eq!(err_timeout.kind(), ErrorKind::Timeout);
    }
}
