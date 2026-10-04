use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
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

    #[error("内部错误: {0}")]
    Internal(String),
}
