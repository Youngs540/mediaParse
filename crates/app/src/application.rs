use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use futures::stream::{self, StreamExt};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;

use mediaparse_core::{
    best_match, parse_filename_with_context, FormatOptions, MatchCandidate, MatchDecision,
    MatchResult, MatchThresholds, MediaType, ParsedMedia, PlanInput, Planner, RenamePlan,
    SidecarInput, VideoPlanInput,
};
use mediaparse_infra::config::Config;
use mediaparse_infra::executor::{ApplyReport, Executor, ExecutorProgress, UndoReport};
use mediaparse_infra::ffprobe::Ffprobe;
use mediaparse_infra::metadata::MetadataProvider;
use mediaparse_infra::scanner::{self, ScanEvent, ScanOptions, ScanOutput, ScannedGroup};
use mediaparse_infra::sidecar::parse_sidecar;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BuildOptions {
    pub template: Option<String>,
    pub tv_template: Option<String>,
    pub series_folder_template: Option<String>,
    pub create_movie_folders: Option<bool>,
    pub create_tv_folders: Option<bool>,
    pub move_sidecars: Option<bool>,
    pub offline: Option<bool>,
    pub concurrency: Option<usize>,
    pub max_component_len: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSummary {
    pub root: PathBuf,
    pub files: usize,
    pub videos: usize,
    pub sidecars: usize,
    pub groups: usize,
    pub duration_ms: u64,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanProgress {
    pub phase: String,
    pub done: usize,
    pub total: usize,
    pub current: String,
}

#[derive(Clone)]
pub struct SessionPlan {
    pub id: String,
    pub root: PathBuf,
    pub plan: RenamePlan,
    pub videos: Vec<VideoPlanInput>,
    pub existing_targets: HashSet<PathBuf>,
    pub existing_dirs: HashSet<PathBuf>,
    pub options: FormatOptions,
    pub move_sidecars: bool,
    pub case_insensitive: bool,
}

pub fn scan_library(
    root: &Path,
    config: &Config,
    progress: impl Fn(ScanEvent) + Send + Sync + 'static,
) -> AppResult<ScanOutput> {
    let options = ScanOptions {
        threads: config.concurrency,
        ..Default::default()
    };
    Ok(scanner::scan_library(root, &options, progress)?)
}

pub async fn build_plan(
    root: &Path,
    config: &Config,
    provider: Arc<dyn MetadataProvider>,
    options: BuildOptions,
    progress: impl Fn(PlanProgress) + Send + Sync + 'static,
) -> AppResult<SessionPlan> {
    let progress: Arc<dyn Fn(PlanProgress) + Send + Sync> = Arc::new(progress);
    let root_owned = root.to_path_buf();
    let scan_options = ScanOptions {
        threads: options.concurrency.unwrap_or(config.concurrency),
        ..Default::default()
    };
    let scan_progress = Arc::clone(&progress);
    let output = tokio::task::spawn_blocking(move || {
        scanner::scan_library(&root_owned, &scan_options, move |event| {
            if let ScanEvent::Progress { files, current } = event {
                scan_progress(PlanProgress {
                    phase: "scan".to_string(),
                    done: files,
                    total: 0,
                    current: current.display().to_string(),
                });
            }
        })
    })
    .await
    .map_err(|error| AppError::Message(format!("scan task failed: {error}")))??;

    let ScanOutput {
        groups,
        stats: _,
        existing_files,
        existing_dirs,
    } = output;

    let parsed: Vec<(ScannedGroup, ParsedMedia)> = groups
        .into_par_iter()
        .map(|group| {
            let parent = group
                .video
                .path
                .parent()
                .and_then(|parent| parent.file_name())
                .map(|name| name.to_string_lossy().to_string());
            let grandparent = group
                .video
                .path
                .parent()
                .and_then(|parent| parent.parent())
                .and_then(|parent| parent.file_name())
                .map(|name| name.to_string_lossy().to_string());
            let parsed = parse_filename_with_context(
                &group.video.file_name,
                parent.as_deref(),
                grandparent.as_deref(),
            );
            (group, parsed)
        })
        .collect();

    let total = parsed.len();
    let done = Arc::new(AtomicUsize::new(0));
    let concurrency = options
        .concurrency
        .unwrap_or(config.concurrency)
        .clamp(1, 16);
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let thresholds = MatchThresholds {
        auto_accept: config.auto_accept_threshold,
        review: config.review_threshold,
    };
    let provider = Arc::clone(&provider);
    let ffprobe = config
        .ffprobe_path
        .as_deref()
        .filter(|path| !path.trim().is_empty())
        .map(Ffprobe::new);
    let progress_meta = Arc::clone(&progress);

    let mut results: Vec<(ScannedGroup, ParsedMedia, MatchResult)> =
        stream::iter(parsed.into_iter().map(|(group, mut parsed)| {
            let provider = Arc::clone(&provider);
            let semaphore = Arc::clone(&semaphore);
            let done = Arc::clone(&done);
            let progress = Arc::clone(&progress_meta);
            let ffprobe = ffprobe.clone();
            async move {
                let _permit = semaphore.acquire().await.ok();
                let search = provider
                    .search(&parsed.title, parsed.year, parsed.media_type)
                    .await;
                let candidates = match search {
                    Ok(candidates) => candidates,
                    Err(error) => {
                        let completed = done.fetch_add(1, Ordering::Relaxed) + 1;
                        progress(PlanProgress {
                            phase: "metadata".to_string(),
                            done: completed,
                            total,
                            current: group.video.file_name.clone(),
                        });
                        return (
                            group,
                            parsed,
                            MatchResult::unmatched(format!("metadata lookup failed: {error}")),
                        );
                    }
                };

                let runtime = match &ffprobe {
                    Some(probe) => probe.duration_minutes(&group.video.path).await,
                    None => None,
                };
                let mut result = best_match(&parsed, &candidates, runtime, thresholds);

                // If ffprobe gave us a runtime, fetch details for the best
                // candidate and re-score with runtime as a tie-breaker.
                if let (Some(runtime), Some(best)) = (runtime, result.candidate.clone()) {
                    if best.runtime_minutes.is_none() {
                        if let Ok(Some(details)) = provider.details(&best.id, best.media_type).await
                        {
                            if let Some(runtime_minutes) = details.runtime_minutes {
                                let enriched: Vec<MatchCandidate> = candidates
                                    .iter()
                                    .map(|candidate| {
                                        if candidate.id == best.id {
                                            MatchCandidate {
                                                runtime_minutes: Some(runtime_minutes),
                                                ..candidate.clone()
                                            }
                                        } else {
                                            candidate.clone()
                                        }
                                    })
                                    .collect();
                                result = best_match(&parsed, &enriched, Some(runtime), thresholds);
                            }
                        }
                    }
                }

                // TV files often carry no episode title in their name. Pull it
                // from the provider so the Jellyfin name includes it.
                if parsed.media_type == MediaType::Tv && parsed.episode_title.is_none() {
                    if let (Some(best), Some(season), Some(episode)) = (
                        result.candidate.as_ref(),
                        parsed.season,
                        parsed.episodes.first(),
                    ) {
                        if let Ok(Some(details)) =
                            provider.episode_details(&best.id, season, *episode).await
                        {
                            parsed.episode_title = details.title;
                        }
                    }
                }

                let completed = done.fetch_add(1, Ordering::Relaxed) + 1;
                progress(PlanProgress {
                    phase: "metadata".to_string(),
                    done: completed,
                    total,
                    current: group.video.file_name.clone(),
                });
                (group, parsed, result)
            }
        }))
        .buffer_unordered(concurrency)
        .collect()
        .await;

    // Metadata lookups finish out of order. Restore the scanner's path order
    // so the preview table and version suffixes stay deterministic.
    results.sort_by(|a, b| a.0.video.path.cmp(&b.0.video.path));

    let mut videos = Vec::with_capacity(results.len());
    for (group, parsed, match_result) in results {
        let video_id = item_id(&group.video.path);
        let video_stem = group
            .video
            .path
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_default();
        let sidecars = group
            .sidecars
            .iter()
            .enumerate()
            .map(|(index, sidecar)| {
                let info = sidecar.sidecar.clone().unwrap_or_else(|| {
                    parse_sidecar(&video_stem, &sidecar.file_name, sidecar.kind)
                });
                SidecarInput {
                    id: format!("{video_id}-s{index}"),
                    source: sidecar.path.clone(),
                    info,
                }
            })
            .collect();
        videos.push(VideoPlanInput {
            id: video_id,
            source: group.video.path.clone(),
            parsed,
            match_result,
            sidecars,
            skip: false,
        });
    }

    let mut format_options = FormatOptions {
        template: config.template.clone(),
        tv_template: config.tv_template.clone(),
        series_folder_template: config.series_folder_template.clone(),
        create_movie_folders: config.create_movie_folders,
        create_tv_folders: config.create_tv_folders,
        max_component_len: config.max_component_len,
        ..Default::default()
    };
    if let Some(template) = options.template {
        if !template.trim().is_empty() {
            format_options.template = template;
        }
    }
    if let Some(template) = options.tv_template {
        if !template.trim().is_empty() {
            format_options.tv_template = template;
        }
    }
    if let Some(template) = options.series_folder_template {
        if !template.trim().is_empty() {
            format_options.series_folder_template = template;
        }
    }
    if let Some(create) = options.create_movie_folders {
        format_options.create_movie_folders = create;
    }
    if let Some(create) = options.create_tv_folders {
        format_options.create_tv_folders = create;
    }
    if let Some(max_len) = options.max_component_len {
        format_options.max_component_len = max_len.clamp(32, 240);
    }
    let move_sidecars = options.move_sidecars.unwrap_or(config.move_sidecars);
    let case_insensitive = cfg!(any(windows, target_os = "macos"));
    let created_at = now_millis();
    let existing_targets: HashSet<PathBuf> = existing_files.into_iter().collect();
    let existing_dirs: HashSet<PathBuf> = existing_dirs.into_iter().collect();

    let plan = Planner::new(&format_options).build(PlanInput {
        root: root.to_path_buf(),
        videos: videos.clone(),
        existing_targets: existing_targets.clone(),
        existing_dirs: existing_dirs.clone(),
        case_insensitive,
        move_sidecars,
        created_at,
    });

    let sequence = PLAN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    Ok(SessionPlan {
        id: format!("plan-{created_at}-{sequence}"),
        root: root.to_path_buf(),
        plan,
        videos,
        existing_targets,
        existing_dirs,
        options: format_options,
        move_sidecars,
        case_insensitive,
    })
}

pub fn rebuild_plan(session: &mut SessionPlan) -> RenamePlan {
    let plan = Planner::new(&session.options).build(PlanInput {
        root: session.root.clone(),
        videos: session.videos.clone(),
        existing_targets: session.existing_targets.clone(),
        existing_dirs: session.existing_dirs.clone(),
        case_insensitive: session.case_insensitive,
        move_sidecars: session.move_sidecars,
        created_at: now_millis(),
    });
    session.plan = plan.clone();
    plan
}

pub fn set_match(
    session: &mut SessionPlan,
    item_id: &str,
    candidate: Option<MatchCandidate>,
) -> RenamePlan {
    let video_id = session
        .plan
        .items
        .iter()
        .find(|item| item.id == item_id)
        .map(|item| item.parent_id.clone().unwrap_or_else(|| item.id.clone()))
        .unwrap_or_else(|| item_id.to_string());
    if let Some(video) = session.videos.iter_mut().find(|video| video.id == video_id) {
        video.skip = false;
        video.match_result = match candidate {
            Some(candidate) => MatchResult {
                candidate: Some(candidate),
                score: 1.0,
                title_score: 1.0,
                year_score: Some(1.0),
                runtime_score: None,
                decision: MatchDecision::AutoAccept,
                reasons: vec!["manual match".to_string()],
            },
            None => MatchResult::unmatched("no match selected"),
        };
    }
    rebuild_plan(session)
}

pub fn skip_item(session: &mut SessionPlan, item_id: &str) -> RenamePlan {
    let video_id = session
        .plan
        .items
        .iter()
        .find(|item| item.id == item_id)
        .map(|item| item.parent_id.clone().unwrap_or_else(|| item.id.clone()))
        .unwrap_or_else(|| item_id.to_string());
    if let Some(video) = session.videos.iter_mut().find(|video| video.id == video_id) {
        video.skip = true;
    }
    rebuild_plan(session)
}

pub fn apply_plan(
    executor: &Executor,
    session: &SessionPlan,
    selected: &[String],
    progress: impl Fn(ExecutorProgress) + Send + Sync,
) -> AppResult<ApplyReport> {
    Ok(executor.apply(&session.plan, selected, progress)?)
}

pub fn undo_last(executor: &Executor) -> AppResult<UndoReport> {
    Ok(executor.undo_last()?)
}

pub async fn search_metadata(
    provider: Arc<dyn MetadataProvider>,
    query: &str,
    year: Option<u16>,
    media_type: MediaType,
) -> AppResult<Vec<MatchCandidate>> {
    Ok(provider.search(query, year, media_type).await?)
}

pub fn summary_from_output(root: &Path, output: &ScanOutput) -> ScanSummary {
    ScanSummary {
        root: root.to_path_buf(),
        files: output.stats.files,
        videos: output.stats.videos,
        sidecars: output.stats.sidecars,
        groups: output.stats.groups,
        duration_ms: output.stats.duration_ms,
        errors: output.stats.errors.clone(),
    }
}

pub fn item_id(path: &Path) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in path.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("v{hash:016x}")
}

static PLAN_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use mediaparse_core::{MatchCandidate, PlanStatus};
    use mediaparse_infra::db::Db;
    use mediaparse_infra::executor::Executor;
    use mediaparse_infra::journal::Journal;
    use mediaparse_infra::metadata::{EpisodeDetails, MediaDetails, MetadataError};
    use std::fs;
    use std::sync::Arc;

    struct FakeProvider;

    #[async_trait]
    impl MetadataProvider for FakeProvider {
        fn name(&self) -> &'static str {
            "fake"
        }

        async fn search(
            &self,
            query: &str,
            year: Option<u16>,
            media_type: MediaType,
        ) -> Result<Vec<MatchCandidate>, MetadataError> {
            let needle = query.to_lowercase();
            if needle.contains("matrix") {
                Ok(vec![MatchCandidate::new(
                    "tmdb",
                    "603",
                    "The Matrix",
                    year.or(Some(1999)),
                )])
            } else if media_type == MediaType::Tv && needle.contains("office") {
                Ok(vec![MatchCandidate::new_tv(
                    "tmdb",
                    "2316",
                    "The Office",
                    year.or(Some(2005)),
                )])
            } else {
                Ok(vec![])
            }
        }

        async fn details(
            &self,
            _id: &str,
            _media_type: MediaType,
        ) -> Result<Option<MediaDetails>, MetadataError> {
            Ok(None)
        }

        async fn episode_details(
            &self,
            _series_id: &str,
            _season: u16,
            episode: u16,
        ) -> Result<Option<EpisodeDetails>, MetadataError> {
            Ok(Some(EpisodeDetails {
                title: Some(format!("Episode {episode}")),
                ..Default::default()
            }))
        }
    }

    fn library() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("The.Matrix.1999.1080p.BluRay.x264-YTS.mkv"),
            b"video",
        )
        .unwrap();
        fs::write(
            dir.path()
                .join("The.Matrix.1999.1080p.BluRay.x264-YTS.en.srt"),
            b"subs",
        )
        .unwrap();
        dir
    }

    #[tokio::test]
    async fn builds_a_plan_with_sidecar() {
        let dir = library();
        let config = Config::default();
        let session = build_plan(
            dir.path(),
            &config,
            Arc::new(FakeProvider),
            BuildOptions::default(),
            |_| {},
        )
        .await
        .unwrap();

        assert_eq!(session.plan.items.len(), 2);
        let video = &session.plan.items[0];
        assert_eq!(video.status, PlanStatus::Ready);
        assert!(video.target.to_string_lossy().contains("The Matrix (1999)"));
        assert!(session.plan.items[1].is_sidecar);
        assert!(session.plan.items[1]
            .target
            .to_string_lossy()
            .ends_with(".en.srt"));
    }

    #[tokio::test]
    async fn builds_a_tv_plan_with_provider_episode_title() {
        let dir = tempfile::tempdir().unwrap();
        let season = dir.path().join("The Office").join("Season 01");
        fs::create_dir_all(&season).unwrap();
        fs::write(season.join("The.Office.S01E02.1080p.WEB-DL.mkv"), b"video").unwrap();

        let session = build_plan(
            dir.path(),
            &Config::default(),
            Arc::new(FakeProvider),
            BuildOptions::default(),
            |_| {},
        )
        .await
        .unwrap();

        let video = session
            .plan
            .items
            .iter()
            .find(|item| !item.is_sidecar)
            .unwrap();
        assert_eq!(video.parsed.media_type, MediaType::Tv);
        assert_eq!(video.parsed.season, Some(1));
        assert_eq!(video.parsed.episodes, vec![2]);
        assert_eq!(video.status, PlanStatus::Ready);
        assert!(video
            .target
            .to_string_lossy()
            .ends_with("Season 01/The Office (2005) S01E02 - Episode 2.mkv"));
    }

    #[tokio::test]
    async fn manual_match_rebuilds_target() {
        let dir = library();
        let config = Config::default();
        let mut session = build_plan(
            dir.path(),
            &config,
            Arc::new(FakeProvider),
            BuildOptions::default(),
            |_| {},
        )
        .await
        .unwrap();
        let item_id = session.plan.items[0].id.clone();
        let candidate = MatchCandidate::new("tmdb", "19995", "Avatar", Some(2009));
        let plan = set_match(&mut session, &item_id, Some(candidate));
        assert!(plan.items[0]
            .target
            .to_string_lossy()
            .contains("Avatar (2009)"));
        assert_eq!(plan.items[0].status, PlanStatus::Ready);
    }

    #[tokio::test]
    async fn skip_marks_video_and_sidecar_skipped() {
        let dir = library();
        let config = Config::default();
        let mut session = build_plan(
            dir.path(),
            &config,
            Arc::new(FakeProvider),
            BuildOptions::default(),
            |_| {},
        )
        .await
        .unwrap();
        let item_id = session.plan.items[0].id.clone();
        let plan = skip_item(&mut session, &item_id);
        assert_eq!(plan.items[0].status, PlanStatus::Skipped);
        assert_eq!(plan.items[1].status, PlanStatus::Skipped);
    }

    #[tokio::test]
    async fn apply_and_undo_round_trip() {
        let dir = library();
        let config = Config::default();
        let session = build_plan(
            dir.path(),
            &config,
            Arc::new(FakeProvider),
            BuildOptions::default(),
            |_| {},
        )
        .await
        .unwrap();
        let db = Arc::new(Db::open_in_memory().unwrap());
        let journal = Arc::new(Journal::new(db));
        let executor = Executor::new(journal);
        let selected = vec![session.plan.items[0].id.clone()];
        let report = apply_plan(&executor, &session, &selected, |_| {}).unwrap();
        assert_eq!(report.renamed, 2);
        let undo = undo_last(&executor).unwrap();
        assert_eq!(undo.restored, 2);
    }

    #[tokio::test]
    async fn plan_order_follows_source_paths() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["Zulu.2013.1080p.mkv", "Alpha.2016.1080p.mkv"] {
            fs::write(dir.path().join(name), b"video").unwrap();
        }
        let session = build_plan(
            dir.path(),
            &Config::default(),
            Arc::new(FakeProvider),
            BuildOptions::default(),
            |_| {},
        )
        .await
        .unwrap();

        let sources: Vec<_> = session
            .plan
            .items
            .iter()
            .filter(|item| !item.is_sidecar)
            .map(|item| item.source.clone())
            .collect();
        let mut sorted = sources.clone();
        sorted.sort();
        assert_eq!(sources, sorted);
    }

    #[tokio::test]
    async fn plan_ids_are_unique() {
        let dir = library();
        let config = Config::default();
        let first = build_plan(
            dir.path(),
            &config,
            Arc::new(FakeProvider),
            BuildOptions::default(),
            |_| {},
        )
        .await
        .unwrap();
        let second = build_plan(
            dir.path(),
            &config,
            Arc::new(FakeProvider),
            BuildOptions::default(),
            |_| {},
        )
        .await
        .unwrap();
        assert_ne!(first.id, second.id);
    }

    #[test]
    fn ids_are_stable() {
        let a = item_id(Path::new("/movies/a.mkv"));
        let b = item_id(Path::new("/movies/a.mkv"));
        let c = item_id(Path::new("/movies/b.mkv"));
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
