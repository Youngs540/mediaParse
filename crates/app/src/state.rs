use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{Mutex, RwLock};

use mediaparse_core::{MatchCandidate, MediaType};
use mediaparse_infra::cache::MetadataCache;
use mediaparse_infra::config::{AppPaths, Config};
use mediaparse_infra::db::Db;
use mediaparse_infra::executor::Executor;
use mediaparse_infra::journal::Journal;
use mediaparse_infra::metadata::{MediaDetails, MetadataError, MetadataProvider};
use mediaparse_infra::provider_from_config;

use crate::application::SessionPlan;
use crate::error::AppResult;

pub struct AppState {
    pub paths: AppPaths,
    pub config: RwLock<Config>,
    pub cache: Arc<MetadataCache>,
    pub journal: Arc<Journal>,
    pub executor: Arc<Executor>,
    pub session: Mutex<Option<SessionPlan>>,
    provider: RwLock<Arc<dyn MetadataProvider>>,
}

impl AppState {
    pub fn new(paths: AppPaths) -> AppResult<Self> {
        paths.ensure()?;
        let config = Config::load_or_default(&paths.config_file)?;
        let db = Arc::new(Db::open(&paths.database_file)?);
        let cache = Arc::new(MetadataCache::new(Arc::clone(&db), config.cache_ttl_days));
        let journal = Arc::new(Journal::new(Arc::clone(&db)));
        let executor = Arc::new(Executor::new(Arc::clone(&journal)));
        let provider = provider_or_unconfigured(&config, Arc::clone(&cache));
        Ok(Self {
            paths,
            config: RwLock::new(config),
            cache,
            journal,
            executor,
            session: Mutex::new(None),
            provider: RwLock::new(provider),
        })
    }

    pub async fn provider(&self) -> Arc<dyn MetadataProvider> {
        let guard = self.provider.read().await;
        Arc::clone(&guard)
    }

    pub async fn update_config(&self, mut config: Config) -> AppResult<Config> {
        config.validate();
        config.save(&self.paths.config_file)?;
        let provider = provider_or_unconfigured(&config, Arc::clone(&self.cache));
        *self.config.write().await = config.clone();
        *self.provider.write().await = provider;
        Ok(config)
    }
}

fn provider_or_unconfigured(
    config: &Config,
    cache: Arc<MetadataCache>,
) -> Arc<dyn MetadataProvider> {
    match provider_from_config(config, cache) {
        Ok(provider) => provider,
        Err(_) => Arc::new(UnconfiguredProvider),
    }
}

/// Lets the app start and scan before credentials are configured. Metadata
/// lookups return a helpful configuration error instead of crashing startup.
struct UnconfiguredProvider;

#[async_trait]
impl MetadataProvider for UnconfiguredProvider {
    fn name(&self) -> &'static str {
        "unconfigured"
    }

    async fn search(
        &self,
        _query: &str,
        _year: Option<u16>,
        _media_type: MediaType,
    ) -> Result<Vec<MatchCandidate>, MetadataError> {
        Err(MetadataError::Config(
            "Configure a TMDB or OMDb API key in Settings".to_string(),
        ))
    }

    async fn details(
        &self,
        _id: &str,
        _media_type: MediaType,
    ) -> Result<Option<MediaDetails>, MetadataError> {
        Err(MetadataError::Config(
            "Configure a TMDB or OMDb API key in Settings".to_string(),
        ))
    }
}
