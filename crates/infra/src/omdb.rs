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

const DEFAULT_BASE_URL: &str = "https://www.omdbapi.com";

pub struct OmdbClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    cache: Arc<MetadataCache>,
    limiter: RateLimiter,
    offline: bool,
    max_retries: u32,
}

impl OmdbClient {
    pub fn new(config: &Config, cache: Arc<MetadataCache>) -> Result<Self, MetadataError> {
        if config.omdb_api_key.trim().is_empty() {
            return Err(MetadataError::Config("OMDb needs an API key".to_string()));
        }
        Self::with_base_url(config, cache, DEFAULT_BASE_URL)
    }

    pub fn with_base_url(
        config: &Config,
        cache: Arc<MetadataCache>,
        base_url: impl Into<String>,
    ) -> Result<Self, MetadataError> {
        let http = reqwest::Client::builder()
            .user_agent("mediaparse/0.1")
            .timeout(Duration::from_secs(15))
            .build()?;
        Ok(Self {
            http,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: config.omdb_api_key.clone(),
            cache,
            limiter: RateLimiter::new(config.requests_per_second),
            offline: config.offline,
            max_retries: 3,
        })
    }

    async fn get_json<T: DeserializeOwned>(
        &self,
        query: Vec<(&str, String)>,
    ) -> Result<T, MetadataError> {
        let mut attempt = 0;
        loop {
            let _permit = self.limiter.acquire().await;
            let mut params = query.clone();
            params.push(("apikey", self.api_key.trim().to_string()));
            match self.http.get(&self.base_url).query(&params).send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        return response.json::<T>().await.map_err(MetadataError::from);
                    }
                    let body = response.text().await.unwrap_or_default();
                    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
                        return Err(MetadataError::Api(
                            "OMDb rejected the configured API key".to_string(),
                        ));
                    }
                    if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
                        if attempt >= self.max_retries {
                            return Err(if status == StatusCode::TOO_MANY_REQUESTS {
                                MetadataError::RateLimited
                            } else {
                                MetadataError::Api(format!("OMDb HTTP {status}: {body}"))
                            });
                        }
                        tokio::time::sleep(Duration::from_millis(500 * 2u64.pow(attempt.min(5))))
                            .await;
                        attempt += 1;
                        continue;
                    }
                    return Err(MetadataError::Api(format!("OMDb HTTP {status}: {body}")));
                }
                Err(error) => {
                    if (error.is_timeout() || error.is_connect()) && attempt < self.max_retries {
                        tokio::time::sleep(Duration::from_millis(500 * 2u64.pow(attempt.min(5))))
                            .await;
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
            "{}:search:{}:{}",
            media_type.as_str(),
            query.trim().to_lowercase(),
            year.map(|y| y.to_string()).unwrap_or_default()
        )
    }

    fn details_key(&self, id: &str, media_type: MediaType) -> String {
        format!("{}:{id}", media_type.as_str())
    }

    fn episode_key(&self, id: &str, season: u16, episode: u16) -> String {
        format!("tv:{id}:s{season}e{episode}")
    }
}

#[async_trait]
impl MetadataProvider for OmdbClient {
    fn name(&self) -> &'static str {
        "omdb"
    }

    async fn search(
        &self,
        query: &str,
        year: Option<u16>,
        media_type: MediaType,
    ) -> Result<Vec<MatchCandidate>, MetadataError> {
        let key = self.search_key(query, year, media_type);
        if let Some(cached) = self.cache.get::<Vec<MatchCandidate>>("omdb", &key)? {
            return Ok(cached);
        }
        if self.offline {
            return Err(MetadataError::Offline);
        }

        let mut params = vec![
            ("s", query.to_string()),
            (
                "type",
                match media_type {
                    MediaType::Movie => "movie",
                    MediaType::Tv => "series",
                }
                .to_string(),
            ),
        ];
        if let Some(year) = year {
            params.push(("y", year.to_string()));
        }
        let response: OmdbSearchResponse = self.get_json(params).await?;
        if response.response.eq_ignore_ascii_case("false") {
            if response
                .error
                .as_deref()
                .map(|e| e.to_lowercase().contains("not found"))
                .unwrap_or(false)
            {
                let empty: Vec<MatchCandidate> = Vec::new();
                let _ = self.cache.put("omdb", &key, &empty);
                return Ok(empty);
            }
            return Err(MetadataError::Api(
                response
                    .error
                    .unwrap_or_else(|| "OMDb search failed".to_string()),
            ));
        }
        let candidates: Vec<MatchCandidate> = response
            .search
            .unwrap_or_default()
            .into_iter()
            .map(|item| item.into_candidate(media_type))
            .collect();
        if let Err(error) = self.cache.put("omdb", &key, &candidates) {
            tracing::warn!(%error, "failed to write OMDb search to cache");
        }
        Ok(candidates)
    }

    async fn details(
        &self,
        id: &str,
        media_type: MediaType,
    ) -> Result<Option<MediaDetails>, MetadataError> {
        let key = self.details_key(id, media_type);
        if let Some(cached) = self.cache.get::<MediaDetails>("omdb", &key)? {
            return Ok(Some(cached));
        }
        if self.offline {
            return Err(MetadataError::Offline);
        }

        let response: OmdbDetails = self
            .get_json(vec![("i", id.to_string()), ("plot", "short".to_string())])
            .await?;
        if response.response.eq_ignore_ascii_case("false") {
            return Ok(None);
        }
        let details = response.into_details(id, media_type);
        if let Err(error) = self.cache.put("omdb", &key, &details) {
            tracing::warn!(%error, "failed to write OMDb details to cache");
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
        if let Some(cached) = self.cache.get::<EpisodeDetails>("omdb", &key)? {
            return Ok(Some(cached));
        }
        if self.offline {
            return Err(MetadataError::Offline);
        }

        let response: OmdbEpisode = self
            .get_json(vec![
                ("i", series_id.to_string()),
                ("Season", season.to_string()),
                ("Episode", episode.to_string()),
                ("plot", "short".to_string()),
            ])
            .await?;
        if response.response.eq_ignore_ascii_case("false") {
            return Ok(None);
        }
        let details = response.into_details();
        if let Err(error) = self.cache.put("omdb", &key, &details) {
            tracing::warn!(%error, "failed to write OMDb episode to cache");
        }
        Ok(Some(details))
    }
}

#[derive(Debug, Deserialize)]
struct OmdbSearchResponse {
    #[serde(rename = "Search", default)]
    search: Option<Vec<OmdbSearchItem>>,
    #[serde(rename = "Response", default)]
    response: String,
    #[serde(rename = "Error", default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OmdbSearchItem {
    #[serde(rename = "Title", default)]
    title: String,
    #[serde(rename = "Year", default)]
    year: String,
    #[serde(rename = "imdbID", default)]
    imdb_id: String,
}

impl OmdbSearchItem {
    fn into_candidate(self, media_type: MediaType) -> MatchCandidate {
        MatchCandidate {
            provider: "omdb".to_string(),
            id: self.imdb_id,
            title: self.title,
            year: parse_year(&self.year),
            media_type,
            runtime_minutes: None,
            popularity: None,
            overview: None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct OmdbDetails {
    #[serde(rename = "Title", default)]
    title: String,
    #[serde(rename = "Year", default)]
    year: String,
    #[serde(rename = "Runtime", default)]
    runtime: String,
    #[serde(rename = "imdbID", default)]
    imdb_id: String,
    #[serde(rename = "Plot", default)]
    plot: Option<String>,
    #[serde(rename = "Response", default)]
    response: String,
}

impl OmdbDetails {
    fn into_details(self, fallback_id: &str, media_type: MediaType) -> MediaDetails {
        let runtime = parse_runtime(&self.runtime);
        let candidate = MatchCandidate {
            provider: "omdb".to_string(),
            id: if self.imdb_id.is_empty() {
                fallback_id.to_string()
            } else {
                self.imdb_id.clone()
            },
            title: self.title,
            year: parse_year(&self.year),
            media_type,
            runtime_minutes: runtime,
            popularity: None,
            overview: self.plot.clone(),
        };
        MediaDetails {
            candidate,
            runtime_minutes: runtime,
            imdb_id: Some(if self.imdb_id.is_empty() {
                fallback_id.to_string()
            } else {
                self.imdb_id
            }),
            overview: self.plot,
        }
    }
}

#[derive(Debug, Deserialize)]
struct OmdbEpisode {
    #[serde(rename = "Title", default)]
    title: String,
    #[serde(rename = "Runtime", default)]
    runtime: String,
    #[serde(rename = "Released", default)]
    released: Option<String>,
    #[serde(rename = "Plot", default)]
    plot: Option<String>,
    #[serde(rename = "Response", default)]
    response: String,
}

impl OmdbEpisode {
    fn into_details(self) -> EpisodeDetails {
        let title = self.title.trim();
        EpisodeDetails {
            title: if title.is_empty() {
                None
            } else {
                Some(title.to_string())
            },
            runtime_minutes: parse_runtime(&self.runtime),
            air_date: self.released,
            overview: self.plot,
        }
    }
}

fn parse_year(value: &str) -> Option<u16> {
    let digits: String = value.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.len() == 4 {
        digits.parse().ok()
    } else {
        value
            .split(|c: char| !c.is_ascii_digit())
            .find(|part| part.len() == 4)
            .and_then(|part| part.parse().ok())
    }
}

fn parse_runtime(value: &str) -> Option<u32> {
    value
        .split_whitespace()
        .find_map(|part| part.parse::<u32>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_years() {
        assert_eq!(parse_year("1999"), Some(1999));
        assert_eq!(parse_year("1999–2003"), Some(1999));
        assert_eq!(parse_year("unknown"), None);
    }

    #[test]
    fn parses_runtime() {
        assert_eq!(parse_runtime("136 min"), Some(136));
        assert_eq!(parse_runtime("N/A"), None);
    }

    #[test]
    fn maps_search_item() {
        let item = OmdbSearchItem {
            title: "The Matrix".into(),
            year: "1999".into(),
            imdb_id: "tt0133093".into(),
        };
        let candidate = item.into_candidate(MediaType::Movie);
        assert_eq!(candidate.provider, "omdb");
        assert_eq!(candidate.id, "tt0133093");
        assert_eq!(candidate.year, Some(1999));
        assert_eq!(candidate.media_type, MediaType::Movie);
    }

    #[test]
    fn maps_series_search_item() {
        let item = OmdbSearchItem {
            title: "The Office".into(),
            year: "2005–2013".into(),
            imdb_id: "tt0386676".into(),
        };
        let candidate = item.into_candidate(MediaType::Tv);
        assert_eq!(candidate.id, "tt0386676");
        assert_eq!(candidate.year, Some(2005));
        assert_eq!(candidate.media_type, MediaType::Tv);
    }

    #[test]
    fn maps_episode_details() {
        let episode = OmdbEpisode {
            title: "The Dundies".into(),
            runtime: "22 min".into(),
            released: Some("2005-04-05".into()),
            plot: Some("Party".into()),
            response: "True".into(),
        };
        let details = episode.into_details();
        assert_eq!(details.title.as_deref(), Some("The Dundies"));
        assert_eq!(details.runtime_minutes, Some(22));
        assert_eq!(details.air_date.as_deref(), Some("2005-04-05"));
    }
}
