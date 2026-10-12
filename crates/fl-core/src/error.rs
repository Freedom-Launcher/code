#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("unknown action: {0}")]
    UnknownAction(String),
    #[error("permission denied: {0}")]
    Denied(String),
    #[error("invalid arguments: {0}")]
    BadArgs(String),
    #[error("{0}")]
    Msg(String),
    #[error("network error: {0}")]
    Net(#[from] reqwest::Error),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("data error: {0}")]
    Json(#[from] serde_json::Error),
}

impl CoreError {
    pub fn msg(s: impl Into<String>) -> Self {
        Self::Msg(s.into())
    }
}
