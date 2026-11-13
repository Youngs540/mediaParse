pub mod cache;
pub mod config;
pub mod db;
pub mod executor;
pub mod ffprobe;
pub mod journal;
pub mod metadata;
pub mod omdb;
pub mod rate_limit;
pub mod scanner;
pub mod sidecar;
pub mod tmdb;

use std::sync::Arc;

pub use cache::MetadataCache;
pub use config::{AppPaths, Config, MetadataProviderKind};
pub use db::Db;
pub use executor::{ApplyReport, Executor, ExecutorProgress, UndoReport};
pub use ffprobe::Ffprobe;
pub use journal::{Journal, RunSummary};
pub use metadata::{EpisodeDetails, MediaDetails, MetadataError, MetadataProvider};
pub use scanner::{
    scan_library, ScanEvent, ScanOptions, ScanOutput, ScanStats, ScannedFile, ScannedGroup,
};
pub use tmdb::TmdbClient;

pub fn provider_from_config(
    config: &Config,
    cache: Arc<MetadataCache>,
) -> Result<Arc<dyn MetadataProvider>, MetadataError> {
    match config.provider {
        MetadataProviderKind::Tmdb => Ok(Arc::new(TmdbClient::new(config, cache)?)),
        MetadataProviderKind::Omdb => Ok(Arc::new(omdb::OmdbClient::new(config, cache)?)),
    }
}
