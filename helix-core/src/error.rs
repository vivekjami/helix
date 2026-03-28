use thiserror::Error;

#[derive(Error, Debug)]
pub enum HelixError {
    #[error("document is empty or reduces to nothing after normalization")]
    EmptyDocument,

    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    #[error("redis pool error")]
    Redis(#[from] deadpool_redis::PoolError),

    #[error("redis command error: {0}")]
    RedisCommand(#[from] redis::RedisError),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("index not initialized")]
    IndexNotInitialized,

    #[error("duplicate doc_id: {0}")]
    DuplicateDocId(u64),

    #[error("invalid config: {0}")]
    InvalidConfig(String),
}
