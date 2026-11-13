use mediaparse_infra::config::ConfigError;
use mediaparse_infra::db::DbError;
use mediaparse_infra::executor::ExecutorError;
use mediaparse_infra::metadata::MetadataError;
use mediaparse_infra::scanner::ScanError;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("configuration error: {0}")]
    Config(#[from] ConfigError),
    #[error("database error: {0}")]
    Db(#[from] DbError),
    #[error("scan error: {0}")]
    Scan(#[from] ScanError),
    #[error("metadata error: {0}")]
    Metadata(#[from] MetadataError),
    #[error("executor error: {0}")]
    Executor(#[from] ExecutorError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Message(String),
}

pub type AppResult<T> = Result<T, AppError>;
