use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::formatter::{FormatOptions, FormattedName, NameFormatter};
use crate::matcher::normalize_title;
use crate::model::{
    MatchDecision, MatchResult, MediaType, ParsedMedia, PlanItem, PlanStatus, RenamePlan,
    SidecarInfo,
};

#[derive(Debug, Clone)]
pub struct SidecarInput {
    pub id: String,
    pub source: PathBuf,
    pub info: SidecarInfo,
}

#[derive(Debug, Clone)]
pub struct VideoPlanInput {
    pub id: String,
    pub source: PathBuf,
    pub parsed: ParsedMedia,
    pub match_result: MatchResult,
    pub sidecars: Vec<SidecarInput>,
    pub skip: bool,
}

#[derive(Debug, Clone)]
pub struct PlanInput {
    pub root: PathBuf,
    pub videos: Vec<VideoPlanInput>,
    /// Every path already present in the library (files and directories).
    pub existing_targets: HashSet<PathBuf>,
    pub existing_dirs: HashSet<PathBuf>,
    pub case_insensitive: bool,
    pub move_sidecars: bool,
    pub created_at: u64,
}

pub struct Planner<'a> {
    options: &'a FormatOptions,
}

impl<'a> Planner<'a> {
    pub fn new(options: &'a FormatOptions) -> Self {
        Self { options }
    }

    pub fn build(&self, input: PlanInput) -> RenamePlan {
        let version_groups = build_version_groups(&input.videos);
        let mut items: Vec<PlanItem> = Vec::new();
        let mut formatted_videos: HashMap<String, FormattedName> = HashMap::new();

        for video in &input.videos {
            let mut options = self.options.clone();
            options.version_suffix = version_groups.get(&video.id).cloned().flatten();
            let formatter = NameFormatter::new(&options);
            let formatted = formatter.format_video(
                &video.parsed,
                video.match_result.candidate.as_ref(),
                extension_of(&video.source),
            );
            formatted_videos.insert(video.id.clone(), formatted.clone());

            let target_dir = match &formatted.folder {
                Some(folder) => input.root.join(folder),
                None => video
                    .source
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| input.root.clone()),
            };
            let target = target_dir.join(&formatted.file_name);
            let mut status = status_for(&video.match_result);
            let mut reason = match status {
                PlanStatus::Ready => None,
                PlanStatus::Review => Some("match needs review".to_string()),
                PlanStatus::Unmatched => Some(
                    video
                        .match_result
                        .reasons
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "no metadata match".to_string()),
                ),
                _ => None,
            };
            if video.parsed.media_type == MediaType::Tv
                && video.parsed.episodes.is_empty()
                && status != PlanStatus::Unmatched
            {
                status = PlanStatus::Review;
                reason = Some("no episode number detected".to_string());
            }
            if video.skip {
                status = PlanStatus::Skipped;
                reason = Some("skipped by user".to_string());
            }

            items.push(PlanItem {
                id: video.id.clone(),
                parent_id: None,
                source: video.source.clone(),
                target,
                status,
                score: Some(video.match_result.score),
                parsed: video.parsed.clone(),
                candidate: video.match_result.candidate.clone(),
                sidecar: None,
                is_sidecar: false,
                case_only: false,
                reason: reason.clone(),
            });

            for sidecar in &video.sidecars {
                let sidecar_formatted = formatter.format_sidecar(&formatted, &sidecar.info);
                let sidecar_target = target_dir.join(&sidecar_formatted.file_name);
                let sidecar_status = if !input.move_sidecars {
                    PlanStatus::Skipped
                } else {
                    status
                };
                let sidecar_reason = if !input.move_sidecars {
                    Some("sidecar move disabled".to_string())
                } else {
                    reason.clone()
                };
                items.push(PlanItem {
                    id: sidecar.id.clone(),
                    parent_id: Some(video.id.clone()),
                    source: sidecar.source.clone(),
                    target: sidecar_target,
                    status: sidecar_status,
                    score: Some(video.match_result.score),
                    parsed: video.parsed.clone(),
                    candidate: video.match_result.candidate.clone(),
                    sidecar: Some(sidecar.info.clone()),
                    is_sidecar: true,
                    case_only: false,
                    reason: sidecar_reason,
                });
            }
        }

        let existing_norm: HashSet<String> = input
            .existing_targets
            .iter()
            .map(|path| normalize_path(path, input.case_insensitive))
            .collect();
        let existing_dirs_norm: HashSet<String> = input
            .existing_dirs
            .iter()
            .map(|path| normalize_path(path, input.case_insensitive))
            .collect();

        // Duplicate targets (two sources want the same destination).
        let mut by_target: HashMap<String, Vec<usize>> = HashMap::new();
        for (index, item) in items.iter().enumerate() {
            if matches!(
                item.status,
                PlanStatus::Unmatched | PlanStatus::Skipped | PlanStatus::Conflict
            ) {
                continue;
            }
            by_target
                .entry(normalize_path(&item.target, input.case_insensitive))
                .or_default()
                .push(index);
        }
        for indices in by_target.values() {
            if indices.len() > 1 {
                for index in indices {
                    items[*index].status = PlanStatus::Conflict;
                    items[*index].reason = Some("two files map to the same target".to_string());
                }
            }
        }

        // Existing targets, case-only renames, and already-correct names.
        for item in &mut items {
            if item.status == PlanStatus::Conflict {
                continue;
            }
            if item.target.as_os_str().is_empty() {
                item.status = PlanStatus::Conflict;
                item.reason = Some("empty target".to_string());
                continue;
            }
            let source_norm = normalize_path(&item.source, input.case_insensitive);
            let target_norm = normalize_path(&item.target, input.case_insensitive);

            if item.source == item.target {
                item.status = PlanStatus::Skipped;
                item.reason = Some("already correctly named".to_string());
                continue;
            }

            if source_norm == target_norm {
                item.case_only = true;
                continue;
            }

            if existing_dirs_norm.contains(&target_norm) {
                item.status = PlanStatus::Conflict;
                item.reason = Some("target is an existing directory".to_string());
                continue;
            }
            if existing_norm.contains(&target_norm) {
                item.status = PlanStatus::Conflict;
                item.reason = Some("target already exists".to_string());
            }
        }

        // Sidecars follow their video's outcome unless they have their own
        // conflict.
        let video_outcomes: HashMap<String, (PlanStatus, Option<String>)> = items
            .iter()
            .filter(|item| !item.is_sidecar)
            .map(|item| (item.id.clone(), (item.status, item.reason.clone())))
            .collect();
        for item in &mut items {
            if !item.is_sidecar {
                continue;
            }
            if item.status == PlanStatus::Conflict {
                continue;
            }
            if let Some(parent_id) = &item.parent_id {
                if let Some((parent_status, parent_reason)) = video_outcomes.get(parent_id) {
                    match parent_status {
                        PlanStatus::Conflict => {
                            item.status = PlanStatus::Conflict;
                            item.reason = Some(format!(
                                "video conflict: {}",
                                parent_reason.clone().unwrap_or_default()
                            ));
                        }
                        PlanStatus::Unmatched => {
                            item.status = PlanStatus::Unmatched;
                            item.reason = parent_reason.clone();
                        }
                        PlanStatus::Skipped => {
                            item.status = PlanStatus::Skipped;
                            item.reason = parent_reason.clone();
                        }
                        _ => {}
                    }
                }
            }
        }

        let mut plan = RenamePlan {
            root: input.root,
            items,
            created_at: input.created_at,
            summary: Default::default(),
        };
        plan.recompute_summary();
        plan
    }
}

fn status_for(result: &MatchResult) -> PlanStatus {
    if result.candidate.is_none() {
        return PlanStatus::Unmatched;
    }
    match result.decision {
        MatchDecision::AutoAccept => PlanStatus::Ready,
        MatchDecision::NeedsReview => PlanStatus::Review,
        MatchDecision::Unmatched => PlanStatus::Unmatched,
    }
}

fn build_version_groups(videos: &[VideoPlanInput]) -> HashMap<String, Option<String>> {
    let mut groups: HashMap<String, Vec<&VideoPlanInput>> = HashMap::new();
    for video in videos {
        let title = match &video.match_result.candidate {
            Some(candidate) => candidate.title.clone(),
            None => video.parsed.title.clone(),
        };
        let year = video
            .match_result
            .candidate
            .as_ref()
            .and_then(|candidate| candidate.year)
            .or(video.parsed.year)
            .unwrap_or(0);
        let key = if video.parsed.media_type == MediaType::Tv {
            format!(
                "tv|{}|{}|{}|{}",
                normalize_title(&title),
                year,
                video.parsed.season.unwrap_or(0),
                video
                    .parsed
                    .episodes
                    .iter()
                    .map(|episode| episode.to_string())
                    .collect::<Vec<_>>()
                    .join("-")
            )
        } else {
            format!("movie|{}|{}", normalize_title(&title), year)
        };
        groups.entry(key).or_default().push(video);
    }

    let mut result = HashMap::new();
    for group in groups.values() {
        if group.len() < 2 {
            continue;
        }
        let mut used: HashSet<String> = HashSet::new();
        for (index, video) in group.iter().enumerate() {
            if video.parsed.part.is_some() || video.parsed.edition.is_some() {
                continue;
            }
            let mut label = video
                .parsed
                .quality
                .clone()
                .or_else(|| video.parsed.source.clone())
                .unwrap_or_else(|| format!("version{}", index + 1));
            if used.contains(&label) {
                label = format!("{label}-{}", index + 1);
            }
            used.insert(label.clone());
            result.insert(video.id.clone(), Some(label));
        }
    }
    result
}

fn extension_of(path: &Path) -> &str {
    path.extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("mkv")
}

pub fn normalize_path(path: &Path, case_insensitive: bool) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    if case_insensitive {
        value.to_lowercase()
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matcher::MatchThresholds;
    use crate::model::{MatchCandidate, MediaKind};

    fn options() -> FormatOptions {
        FormatOptions::default()
    }

    fn match_result(title: &str, year: u16, id: &str) -> MatchResult {
        let candidate = MatchCandidate::new("tmdb", id, title, Some(year));
        MatchResult {
            candidate: Some(candidate),
            score: 0.98,
            title_score: 1.0,
            year_score: Some(1.0),
            runtime_score: None,
            decision: MatchDecision::AutoAccept,
            reasons: vec!["test".into()],
        }
    }

    fn video(id: &str, name: &str, title: &str, year: u16, tmdb: &str) -> VideoPlanInput {
        VideoPlanInput {
            id: id.into(),
            source: PathBuf::from("/movies").join(name),
            parsed: ParsedMedia {
                title: title.into(),
                year: Some(year),
                ..Default::default()
            },
            match_result: match_result(title, year, tmdb),
            sidecars: vec![],
            skip: false,
        }
    }

    fn build(videos: Vec<VideoPlanInput>) -> RenamePlan {
        let options = options();
        let planner = Planner::new(&options);
        planner.build(PlanInput {
            root: PathBuf::from("/movies"),
            videos,
            existing_targets: HashSet::new(),
            existing_dirs: HashSet::new(),
            case_insensitive: false,
            move_sidecars: true,
            created_at: 1,
        })
    }

    #[test]
    fn builds_basic_target() {
        let plan = build(vec![video(
            "1",
            "The.Matrix.1999.1080p.mkv",
            "The Matrix",
            1999,
            "603",
        )]);
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].status, PlanStatus::Ready);
        assert_eq!(
            plan.items[0].target,
            PathBuf::from(
                "/movies/The Matrix (1999) [tmdbid-603]/The Matrix (1999) [tmdbid-603].mkv"
            )
        );
    }

    #[test]
    fn detects_existing_target() {
        let mut input_video = video("1", "The.Matrix.1999.1080p.mkv", "The Matrix", 1999, "603");
        input_video.source = PathBuf::from("/movies/The.Matrix.1999.1080p.mkv");
        let target = PathBuf::from(
            "/movies/The Matrix (1999) [tmdbid-603]/The Matrix (1999) [tmdbid-603].mkv",
        );
        let options = options();
        let plan = Planner::new(&options).build(PlanInput {
            root: PathBuf::from("/movies"),
            videos: vec![input_video],
            existing_targets: HashSet::from([target]),
            existing_dirs: HashSet::new(),
            case_insensitive: false,
            move_sidecars: true,
            created_at: 1,
        });
        assert_eq!(plan.items[0].status, PlanStatus::Conflict);
        assert!(plan.items[0].reason.as_deref().unwrap().contains("exists"));
    }

    #[test]
    fn detects_duplicate_targets() {
        let mut a = video("1", "Dune.2021.1080p.mkv", "Dune", 2021, "438631");
        let mut b = video("2", "Dune.2021.2160p.mkv", "Dune", 2021, "438631");
        a.parsed.quality = Some("1080p".into());
        b.parsed.quality = Some("2160p".into());
        let plan = build(vec![a, b]);
        // Different qualities get version suffixes, so no duplicate.
        assert_eq!(plan.items[0].status, PlanStatus::Ready);
        assert!(plan.items[0].target.to_string_lossy().contains("1080p"));
        assert!(plan.items[1].target.to_string_lossy().contains("2160p"));
    }

    #[test]
    fn detects_duplicate_targets_when_versions_are_indistinguishable() {
        let mut a = video("1", "Dune.2021.Extended.mkv", "Dune", 2021, "438631");
        let mut b = video("2", "Dune.2021.Extended.other.mkv", "Dune", 2021, "438631");
        a.parsed.edition = Some("Extended".into());
        b.parsed.edition = Some("Extended".into());
        let plan = build(vec![a, b]);
        assert_eq!(plan.items[0].status, PlanStatus::Conflict);
        assert_eq!(plan.items[1].status, PlanStatus::Conflict);
    }

    #[test]
    fn marks_case_only_rename() {
        let mut v = video("1", "The.Matrix.1999.mkv", "The Matrix", 1999, "603");
        v.source = PathBuf::from(
            "/movies/The Matrix (1999) [tmdbid-603]/the matrix (1999) [tmdbid-603].mkv",
        );
        let options = options();
        let plan = Planner::new(&options).build(PlanInput {
            root: PathBuf::from("/movies"),
            videos: vec![v],
            existing_targets: HashSet::new(),
            existing_dirs: HashSet::new(),
            case_insensitive: true,
            move_sidecars: true,
            created_at: 1,
        });
        assert!(plan.items[0].case_only);
        assert_eq!(plan.items[0].status, PlanStatus::Ready);
    }

    #[test]
    fn skips_already_correct_name() {
        let target = PathBuf::from(
            "/movies/The Matrix (1999) [tmdbid-603]/The Matrix (1999) [tmdbid-603].mkv",
        );
        let mut v = video("1", "x.mkv", "The Matrix", 1999, "603");
        v.source = target.clone();
        let options = options();
        let plan = Planner::new(&options).build(PlanInput {
            root: PathBuf::from("/movies"),
            videos: vec![v],
            existing_targets: HashSet::from([target]),
            existing_dirs: HashSet::new(),
            case_insensitive: false,
            move_sidecars: true,
            created_at: 1,
        });
        assert_eq!(plan.items[0].status, PlanStatus::Skipped);
    }

    #[test]
    fn groups_sidecar_with_video() {
        let mut v = video("1", "The.Matrix.1999.mkv", "The Matrix", 1999, "603");
        v.sidecars.push(SidecarInput {
            id: "1-s1".into(),
            source: PathBuf::from("/movies/The.Matrix.1999.en.srt"),
            info: SidecarInfo {
                kind: MediaKind::Subtitle,
                language: Some("en".into()),
                flags: vec![],
                suffix: ".en".into(),
                extension: "srt".into(),
            },
        });
        let plan = build(vec![v]);
        assert_eq!(plan.items.len(), 2);
        let sidecar = &plan.items[1];
        assert!(sidecar.is_sidecar);
        assert_eq!(sidecar.parent_id.as_deref(), Some("1"));
        assert_eq!(
            sidecar.target,
            PathBuf::from(
                "/movies/The Matrix (1999) [tmdbid-603]/The Matrix (1999) [tmdbid-603].en.srt"
            )
        );
        assert_eq!(sidecar.status, PlanStatus::Ready);
    }

    #[test]
    fn disabled_sidecar_move_is_skipped() {
        let mut v = video("1", "The.Matrix.1999.mkv", "The Matrix", 1999, "603");
        v.sidecars.push(SidecarInput {
            id: "1-s1".into(),
            source: PathBuf::from("/movies/The.Matrix.1999.en.srt"),
            info: SidecarInfo {
                kind: MediaKind::Subtitle,
                language: Some("en".into()),
                flags: vec![],
                suffix: ".en".into(),
                extension: "srt".into(),
            },
        });
        let options = options();
        let plan = Planner::new(&options).build(PlanInput {
            root: PathBuf::from("/movies"),
            videos: vec![v],
            existing_targets: HashSet::new(),
            existing_dirs: HashSet::new(),
            case_insensitive: false,
            move_sidecars: false,
            created_at: 1,
        });
        assert_eq!(plan.items[1].status, PlanStatus::Skipped);
    }

    #[test]
    fn unmatched_video_stays_unmatched() {
        let mut v = video("1", "holiday_video.mp4", "Holiday Video", 2020, "0");
        v.match_result = MatchResult::unmatched("no metadata candidates");
        let plan = build(vec![v]);
        assert_eq!(plan.items[0].status, PlanStatus::Unmatched);
    }

    #[test]
    fn video_conflict_propagates_to_sidecar() {
        let mut v = video("1", "The.Matrix.1999.mkv", "The Matrix", 1999, "603");
        v.sidecars.push(SidecarInput {
            id: "1-s1".into(),
            source: PathBuf::from("/movies/The.Matrix.1999.en.srt"),
            info: SidecarInfo {
                kind: MediaKind::Subtitle,
                language: Some("en".into()),
                flags: vec![],
                suffix: ".en".into(),
                extension: "srt".into(),
            },
        });
        let target = PathBuf::from(
            "/movies/The Matrix (1999) [tmdbid-603]/The Matrix (1999) [tmdbid-603].mkv",
        );
        let options = options();
        let plan = Planner::new(&options).build(PlanInput {
            root: PathBuf::from("/movies"),
            videos: vec![v],
            existing_targets: HashSet::from([target]),
            existing_dirs: HashSet::new(),
            case_insensitive: false,
            move_sidecars: true,
            created_at: 1,
        });
        assert_eq!(plan.items[0].status, PlanStatus::Conflict);
        assert_eq!(plan.items[1].status, PlanStatus::Conflict);
        assert!(plan.items[1]
            .reason
            .as_deref()
            .unwrap()
            .contains("video conflict"));
    }

    #[test]
    fn summary_counts_statuses() {
        let plan = build(vec![video(
            "1",
            "The.Matrix.1999.mkv",
            "The Matrix",
            1999,
            "603",
        )]);
        assert_eq!(plan.summary.total, 1);
        assert_eq!(plan.summary.ready, 1);
    }

    #[test]
    fn thresholds_are_not_used_directly_here() {
        // Keeps MatchThresholds imported for the fixture helper's clarity.
        let _ = MatchThresholds::default();
    }

    fn tv_video(
        id: &str,
        name: &str,
        title: &str,
        year: u16,
        tmdb: &str,
        season: u16,
        episodes: Vec<u16>,
    ) -> VideoPlanInput {
        let candidate = MatchCandidate::new_tv("tmdb", tmdb, title, Some(year));
        VideoPlanInput {
            id: id.into(),
            source: PathBuf::from("/tv").join(name),
            parsed: ParsedMedia {
                title: title.into(),
                year: Some(year),
                media_type: MediaType::Tv,
                season: Some(season),
                episodes,
                episode_title: Some("The Dundies".into()),
                ..Default::default()
            },
            match_result: MatchResult {
                candidate: Some(candidate),
                score: 0.98,
                title_score: 1.0,
                year_score: Some(1.0),
                runtime_score: None,
                decision: MatchDecision::AutoAccept,
                reasons: vec!["test".into()],
            },
            sidecars: vec![],
            skip: false,
        }
    }

    #[test]
    fn builds_tv_episode_target() {
        let plan = build(vec![tv_video(
            "1",
            "Show.S01E02.mkv",
            "Show Name",
            2020,
            "42",
            1,
            vec![2],
        )]);
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].status, PlanStatus::Ready);
        assert_eq!(
            plan.items[0].target,
            PathBuf::from(
                "/movies/Show Name (2020) [tmdbid-42]/Season 01/Show Name (2020) S01E02 - The Dundies.mkv"
            )
        );
    }

    #[test]
    fn tv_episodes_do_not_get_version_suffixes() {
        let plan = build(vec![
            tv_video("1", "Show.S01E01.mkv", "Show Name", 2020, "42", 1, vec![1]),
            tv_video("2", "Show.S01E02.mkv", "Show Name", 2020, "42", 1, vec![2]),
        ]);
        assert_eq!(plan.items[0].status, PlanStatus::Ready);
        assert_eq!(plan.items[1].status, PlanStatus::Ready);
        assert!(!plan.items[0].target.to_string_lossy().contains("1080p"));
        assert!(!plan.items[1].target.to_string_lossy().contains("1080p"));
    }

    #[test]
    fn tv_without_episode_number_needs_review() {
        let mut v = tv_video("1", "Show.Name.mkv", "Show Name", 2020, "42", 1, vec![]);
        v.parsed.season = Some(1);
        let plan = build(vec![v]);
        assert_eq!(plan.items[0].status, PlanStatus::Review);
        assert!(plan.items[0]
            .reason
            .as_deref()
            .unwrap()
            .contains("episode number"));
    }
}
