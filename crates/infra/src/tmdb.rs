use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::Deserialize;

use mediaparse_core::{MatchCandidate, MediaType};

use crate::cache::MetadataCache;
use crate::config::Config;
use crate::metadata::{EpisodeDetails, MediaDetails, MetadataError, MetadataProvider};
use crate::rate_limit::RateLimiter;

const DEFAULT_BASE_URL: &str = "https://api.themoviedb.org/3";

pub struct TmdbClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    read_token: String,
    language: String,
    cache: Arc<MetadataCache>,
    limiter: RateLimiter,
    offline: bool,
    max_retries: u32,
}

impl TmdbClient {
    pub fn new(config: &Config, cache: Arc<MetadataCache>) -> Result<Self, MetadataError> {
        if !config.has_tmdb_credentials() {
            return Err(MetadataError::Config(
                "TMDB needs an API key or read token".to_string(),
            ));
        }
        Self::with_base_url(config, cache, DEFAULT_BASE_URL)
    }

    pub fn with_base_url(
        config: &Config,
        cache: Arc<MetadataCache>,
        base_url: impl Into<String>,
    ) -> Result<Self, MetadataError> {
        let http = reqwest::Client::builder()
            .user_agent("mediaparse/0.1 (+https://github.com/mediaparse)")
            .timeout(Duration::from_secs(15))
            .build()?;
        Ok(Self {
            http,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: config.tmdb_api_key.clone(),
            read_token: config.tmdb_read_token.clone(),
            language: config.language.clone(),
            cache,
            limiter: RateLimiter::new(config.requests_per_second),
            offline: config.offline,
            max_retries: 3,
        })
    }

    async fn get_json<T: DeserializeOwned>(
        &self,
        path: &str,
        query: Vec<(&str, String)>,
    ) -> Result<T, MetadataError> {
        let url = format!("{}{}", self.base_url, path);
        let mut attempt = 0;
        loop {
            let _permit = self.limiter.acquire().await;
            let mut request = self.http.get(&url).query(&query);
            if !self.read_token.trim().is_empty() {
                request = request.bearer_auth(self.read_token.trim());
            } else if !self.api_key.trim().is_empty() {
                request = request.query(&[("api_key", self.api_key.trim())]);
            }

            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        return response.json::<T>().await.map_err(MetadataError::from);
                    }

                    let retry_after = response
                        .headers()
                        .get("retry-after")
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value| value.parse::<u64>().ok());
                    let body = response.text().await.unwrap_or_default();

                    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
                        return Err(MetadataError::Api(
                            "TMDB rejected the configured credentials".to_string(),
                        ));
                    }
                    if status == StatusCode::NOT_FOUND {
                        return Err(MetadataError::Api(format!(
                            "TMDB resource not found: {path}"
                        )));
                    }
                    if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
                        if attempt >= self.max_retries {
                            return Err(if status == StatusCode::TOO_MANY_REQUESTS {
                                MetadataError::RateLimited
                            } else {
                                MetadataError::Api(format!("TMDB HTTP {status}: {body}"))
                            });
                        }
                        let delay = retry_after
                            .map(Duration::from_secs)
                            .unwrap_or_else(|| backoff(attempt));
                        tokio::time::sleep(delay).await;
                        attempt += 1;
                        continue;
                    }
                    return Err(MetadataError::Api(format!("TMDB HTTP {status}: {body}")));
                }
                Err(error) => {
                    if (error.is_timeout() || error.is_connect()) && attempt < self.max_retries {
                        tokio::time::sleep(backoff(attempt)).await;
                        attempt += 1;
                        continue;
                    }
                    return Err(MetadataError::Http(error));
                }
            }
        }
    }

    fn search_key(&self, query: &str, year: Option<u16>, media_type: MediaType) -> String {
        format!(
            "{}:search:{}:{}:{}",
            media_type.as_str(),
            normalize_query(query),
            year.map(|y| y.to_string()).unwrap_or_default(),
            self.language
        )
    }

    fn details_key(&self, id: &str, media_type: MediaType) -> String {
        format!("{}:{id}:{}", media_type.as_str(), self.language)
    }

    fn episode_key(&self, id: &str, season: u16, episode: u16) -> String {
        format!("tv:{id}:s{season}e{episode}:{}", self.language)
    }
}

#[async_trait]
impl MetadataProvider for TmdbClient {
    fn name(&self) -> &'static str {
        "tmdb"
    }

    async fn search(
        &self,
        query: &str,
        year: Option<u16>,
        media_type: MediaType,
    ) -> Result<Vec<MatchCandidate>, MetadataError> {
        let key = self.search_key(query, year, media_type);
        if let Some(cached) = self.cache.get::<Vec<MatchCandidate>>("tmdb", &key)? {
            return Ok(cached);
        }
        if self.offline {
            return Err(MetadataError::Offline);
        }

        let mut params = vec![
            ("query", query.to_string()),
            ("language", self.language.clone()),
            ("include_adult", "false".to_string()),
        ];
        let candidates: Vec<MatchCandidate> = match media_type {
            MediaType::Movie => {
                if let Some(year) = year {
                    params.push(("year", year.to_string()));
                }
                let response: TmdbSearchResponse = self.get_json("/search/movie", params).await?;
                response
                    .results
                    .into_iter()
                    .map(|movie| movie.into_candidate())
                    .collect()
            }
            MediaType::Tv => {
                if let Some(year) = year {
                    params.push(("first_air_date_year", year.to_string()));
                }
                let response: TmdbTvSearchResponse = self.get_json("/search/tv", params).await?;
                response
                    .results
                    .into_iter()
                    .map(|series| series.into_candidate())
                    .collect()
            }
        };
        if let Err(error) = self.cache.put("tmdb", &key, &candidates) {
            tracing::warn!(%error, "failed to write TMDB search to cache");
        }
        Ok(candidates)
    }

    async fn details(
        &self,
        id: &str,
        media_type: MediaType,
    ) -> Result<Option<MediaDetails>, MetadataError> {
        let key = self.details_key(id, media_type);
        if let Some(cached) = self.cache.get::<MediaDetails>("tmdb", &key)? {
            return Ok(Some(cached));
        }
        if self.offline {
            return Err(MetadataError::Offline);
        }

        let details = match media_type {
            MediaType::Movie => {
                let response: TmdbDetails = self
                    .get_json(
                        &format!("/movie/{id}"),
                        vec![
                            ("language", self.language.clone()),
                            ("append_to_response", "external_ids".to_string()),
                        ],
                    )
                    .await?;
                response.into_details(id)
            }
            MediaType::Tv => {
                let response: TmdbTvDetails = self
                    .get_json(
                        &format!("/tv/{id}"),
                        vec![
                            ("language", self.language.clone()),
                            ("append_to_response", "external_ids".to_string()),
                        ],
                    )
                    .await?;
                response.into_details(id)
            }
        };
        if let Err(error) = self.cache.put("tmdb", &key, &details) {
            tracing::warn!(%error, "failed to write TMDB details to cache");
        }
        Ok(Some(details))
    }

    async fn episode_details(
        &self,
        series_id: &str,
        season: u16,
        episode: u16,
    ) -> Result<Option<EpisodeDetails>, MetadataError> {
        let key = self.episode_key(series_id, season, episode);
        if let Some(cached) = self.cache.get::<EpisodeDetails>("tmdb", &key)? {
            return Ok(Some(cached));
        }
        if self.offline {
            return Err(MetadataError::Offline);
        }

        let response: TmdbEpisode = self
            .get_json(
                &format!("/tv/{series_id}/season/{season}/episode/{episode}"),
                vec![("language", self.language.clone())],
            )
            .await?;
        let details = response.into_details();
        if let Err(error) = self.cache.put("tmdb", &key, &details) {
            tracing::warn!(%error, "failed to write TMDB episode to cache");
        }
        Ok(Some(details))
    }
}

fn backoff(attempt: u32) -> Duration {
    Duration::from_millis(500 * 2u64.pow(attempt.min(5)))
}

fn normalize_query(query: &str) -> String {
    query
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn parse_year(date: Option<&str>) -> Option<u16> {
    date?.get(0..4)?.parse().ok()
}

#[derive(Debug, Deserialize)]
struct TmdbSearchResponse {
    #[serde(default)]
    results: Vec<TmdbMovie>,
}

#[derive(Debug, Deserialize)]
struct TmdbMovie {
    id: u64,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    original_title: Option<String>,
    #[serde(default)]
    release_date: Option<String>,
    #[serde(default)]
    popularity: Option<f64>,
    #[serde(default)]
    overview: Option<String>,
}

impl TmdbMovie {
    fn into_candidate(self) -> MatchCandidate {
        MatchCandidate {
            provider: "tmdb".to_string(),
            id: self.id.to_string(),
            title: self
                .title
                .or(self.original_title)
                .unwrap_or_else(|| "Untitled".to_string()),
            year: parse_year(self.release_date.as_deref()),
            media_type: MediaType::Movie,
            runtime_minutes: None,
            popularity: self.popularity,
            overview: self.overview,
        }
    }
}

#[derive(Debug, Deserialize)]
struct TmdbTvSearchResponse {
    #[serde(default)]
    results: Vec<TmdbTv>,
}

#[derive(Debug, Deserialize)]
struct TmdbTv {
    id: u64,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    original_name: Option<String>,
    #[serde(default)]
    first_air_date: Option<String>,
    #[serde(default)]
    popularity: Option<f64>,
    #[serde(default)]
    overview: Option<String>,
}

impl TmdbTv {
    fn into_candidate(self) -> MatchCandidate {
        MatchCandidate {
            provider: "tmdb".to_string(),
            id: self.id.to_string(),
            title: self
                .name
                .or(self.original_name)
                .unwrap_or_else(|| "Untitled".to_string()),
            year: parse_year(self.first_air_date.as_deref()),
            media_type: MediaType::Tv,
            runtime_minutes: None,
            popularity: self.popularity,
            overview: self.overview,
        }
    }
}

#[derive(Debug, Deserialize)]
struct TmdbDetails {
    id: u64,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    original_title: Option<String>,
    #[serde(default)]
    release_date: Option<String>,
    #[serde(default)]
    runtime: Option<u32>,
    #[serde(default)]
    popularity: Option<f64>,
    #[serde(default)]
    overview: Option<String>,
    #[serde(default)]
    external_ids: Option<TmdbExternalIds>,
}

#[derive(Debug, Deserialize)]
struct TmdbExternalIds {
    #[serde(default)]
    imdb_id: Option<String>,
}

impl TmdbDetails {
    fn into_details(self, fallback_id: &str) -> MediaDetails {
        let candidate = MatchCandidate {
            provider: "tmdb".to_string(),
            id: if self.id == 0 {
                fallback_id.to_string()
            } else {
                self.id.to_string()
            },
            title: self
                .title
                .or(self.original_title)
                .unwrap_or_else(|| "Untitled".to_string()),
            year: parse_year(self.release_date.as_deref()),
            media_type: MediaType::Movie,
            runtime_minutes: self.runtime,
            popularity: self.popularity,
            overview: self.overview.clone(),
        };
        MediaDetails {
            candidate,
            runtime_minutes: self.runtime,
            imdb_id: self.external_ids.and_then(|ids| ids.imdb_id),
            overview: self.overview,
        }
    }
}

#[derive(Debug, Deserialize)]
struct TmdbTvDetails {
    id: u64,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    original_name: Option<String>,
    #[serde(default)]
    first_air_date: Option<String>,
    #[serde(default)]
    episode_run_time: Vec<u32>,
    #[serde(default)]
    popularity: Option<f64>,
    #[serde(default)]
    overview: Option<String>,
    #[serde(default)]
    external_ids: Option<TmdbExternalIds>,
}

impl TmdbTvDetails {
    fn into_details(self, fallback_id: &str) -> MediaDetails {
        let runtime = self.episode_run_time.first().copied();
        let candidate = MatchCandidate {
            provider: "tmdb".to_string(),
            id: if self.id == 0 {
                fallback_id.to_string()
            } else {
                self.id.to_string()
            },
            title: self
                .name
                .or(self.original_name)
                .unwrap_or_else(|| "Untitled".to_string()),
            year: parse_year(self.first_air_date.as_deref()),
            media_type: MediaType::Tv,
            runtime_minutes: runtime,
            popularity: self.popularity,
            overview: self.overview.clone(),
        };
        MediaDetails {
            candidate,
            runtime_minutes: runtime,
            imdb_id: self.external_ids.and_then(|ids| ids.imdb_id),
            overview: self.overview,
        }
    }
}

#[derive(Debug, Deserialize)]
struct TmdbEpisode {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    runtime: Option<u32>,
    #[serde(default)]
    air_date: Option<String>,
    #[serde(default)]
    overview: Option<String>,
}

impl TmdbEpisode {
    fn into_details(self) -> EpisodeDetails {
        EpisodeDetails {
            title: self.name.filter(|name| !name.trim().is_empty()),
            runtime_minutes: self.runtime,
            air_date: self.air_date,
            overview: self.overview,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn server(
        body: &'static str,
        counter: Arc<AtomicUsize>,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let mut buffer = [0u8; 8192];
                let _ = socket.read(&mut buffer).await;
                counter.fetch_add(1, Ordering::SeqCst);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(response.as_bytes()).await;
            }
        });
        (format!("http://{address}"), handle)
    }

    fn config() -> Config {
        Config {
            tmdb_read_token: "test-token".into(),
            requests_per_second: 50,
            ..Config::default()
        }
    }

    fn client(base_url: String) -> TmdbClient {
        let cache = Arc::new(MetadataCache::new(
            Arc::new(Db::open_in_memory().unwrap()),
            30,
        ));
        TmdbClient::with_base_url(&config(), cache, base_url).unwrap()
    }

    #[tokio::test]
    async fn searches_and_caches() {
        let body = r#"{"results":[{"id":603,"title":"The Matrix","release_date":"1999-03-31","popularity":42.0,"overview":"Neo"}]}"#;
        let counter = Arc::new(AtomicUsize::new(0));
        let (base, handle) = server(body, Arc::clone(&counter)).await;
        let client = client(base);
        let first = client
            .search("The Matrix", Some(1999), MediaType::Movie)
            .await
            .unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].id, "603");
        assert_eq!(first[0].year, Some(1999));
        assert_eq!(first[0].media_type, MediaType::Movie);
        let second = client
            .search("the matrix", Some(1999), MediaType::Movie)
            .await
            .unwrap();
        assert_eq!(second.len(), 1);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        handle.abort();
    }

    #[tokio::test]
    async fn searches_tv_and_caches() {
        let body = r#"{"results":[{"id":2316,"name":"The Office","first_air_date":"2005-03-24","popularity":50.0,"overview":"Scranton"}]}"#;
        let counter = Arc::new(AtomicUsize::new(0));
        let (base, handle) = server(body, Arc::clone(&counter)).await;
        let client = client(base);
        let first = client
            .search("The Office", Some(2005), MediaType::Tv)
            .await
            .unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].id, "2316");
        assert_eq!(first[0].year, Some(2005));
        assert_eq!(first[0].media_type, MediaType::Tv);
        let second = client
            .search("the office", Some(2005), MediaType::Tv)
            .await
            .unwrap();
        assert_eq!(second.len(), 1);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        handle.abort();
    }

    #[tokio::test]
    async fn parses_details_runtime() {
        let body = r#"{"id":603,"title":"The Matrix","release_date":"1999-03-31","runtime":136,"popularity":42.0,"overview":"Neo","external_ids":{"imdb_id":"tt0133093"}}"#;
        let counter = Arc::new(AtomicUsize::new(0));
        let (base, handle) = server(body, counter).await;
        let client = client(base);
        let details = client
            .details("603", MediaType::Movie)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(details.runtime_minutes, Some(136));
        assert_eq!(details.imdb_id.as_deref(), Some("tt0133093"));
        assert_eq!(details.candidate.id, "603");
        assert_eq!(details.candidate.media_type, MediaType::Movie);
        handle.abort();
    }

    #[tokio::test]
    async fn parses_tv_details_runtime() {
        let body = r#"{"id":2316,"name":"The Office","first_air_date":"2005-03-24","episode_run_time":[22],"popularity":50.0,"overview":"Scranton","external_ids":{"imdb_id":"tt0386676"}}"#;
        let counter = Arc::new(AtomicUsize::new(0));
        let (base, handle) = server(body, counter).await;
        let client = client(base);
        let details = client
            .details("2316", MediaType::Tv)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(details.runtime_minutes, Some(22));
        assert_eq!(details.imdb_id.as_deref(), Some("tt0386676"));
        assert_eq!(details.candidate.media_type, MediaType::Tv);
        handle.abort();
    }

    #[tokio::test]
    async fn parses_episode_details() {
        let body =
            r#"{"name":"The Dundies","runtime":22,"air_date":"2005-04-05","overview":"Party"}"#;
        let counter = Arc::new(AtomicUsize::new(0));
        let (base, handle) = server(body, counter).await;
        let client = client(base);
        let details = client.episode_details("2316", 1, 2).await.unwrap().unwrap();
        assert_eq!(details.title.as_deref(), Some("The Dundies"));
        assert_eq!(details.runtime_minutes, Some(22));
        assert_eq!(details.air_date.as_deref(), Some("2005-04-05"));
        handle.abort();
    }

    #[tokio::test]
    async fn offline_miss_returns_error() {
        let cache = Arc::new(MetadataCache::new(
            Arc::new(Db::open_in_memory().unwrap()),
            30,
        ));
        let mut config = config();
        config.offline = true;
        let client = TmdbClient::with_base_url(&config, cache, "http://127.0.0.1:1").unwrap();
        let result = client.search("Anything", None, MediaType::Movie).await;
        assert!(matches!(result, Err(MetadataError::Offline)));
    }

    #[test]
    fn parses_release_year() {
        assert_eq!(parse_year(Some("1999-03-31")), Some(1999));
        assert_eq!(parse_year(None), None);
        assert_eq!(parse_year(Some("bad")), None);
    }
}
