use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Broad classification of a file found by the scanner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaKind {
    Movie,
    Subtitle,
    Nfo,
    Image,
    Other,
}

/// Whether a video is a movie or a TV episode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    #[default]
    Movie,
    Tv,
}

impl MediaType {
    pub fn as_str(self) -> &'static str {
        match self {
            MediaType::Movie => "movie",
            MediaType::Tv => "tv",
        }
    }

    pub fn is_tv(self) -> bool {
        self == MediaType::Tv
    }
}

/// A detected multi-part marker (`CD1`, `part 2`, `disc3`, ...).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartMarker {
    /// 1-based part number.
    pub index: u32,
    /// Normalized marker word: `part`, `cd`, `disc` or `disk`.
    pub label: String,
}

/// Result of parsing a single media filename.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedMedia {
    pub title: String,
    pub year: Option<u16>,
    pub media_type: MediaType,
    /// Season number for TV episodes. `Some(0)` means specials.
    pub season: Option<u16>,
    /// Episode numbers in this file. Most files have one; multi-episode files
    /// have two or more.
    pub episodes: Vec<u16>,
    /// Episode title when it can be recovered from the file name or metadata.
    pub episode_title: Option<String>,
    pub quality: Option<String>,
    pub source: Option<String>,
    pub codec: Option<String>,
    pub edition: Option<String>,
    pub part: Option<PartMarker>,
    pub release_group: Option<String>,
    /// 0.0 - 1.0 heuristic confidence in the parse.
    pub confidence: f32,
}

impl Default for ParsedMedia {
    fn default() -> Self {
        Self {
            title: String::new(),
            year: None,
            media_type: MediaType::Movie,
            season: None,
            episodes: Vec::new(),
            episode_title: None,
            quality: None,
            source: None,
            codec: None,
            edition: None,
            part: None,
            release_group: None,
            confidence: 0.0,
        }
    }
}

impl ParsedMedia {
    pub fn has_title(&self) -> bool {
        !self.title.trim().is_empty()
    }

    /// Human readable `Title (Year)` used in diagnostics.
    pub fn display_title(&self) -> String {
        match self.year {
            Some(year) => format!("{} ({})", self.title, year),
            None => self.title.clone(),
        }
    }
}

/// A metadata candidate returned by TMDB, OMDb, or another provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchCandidate {
    /// Provider name, e.g. `tmdb` or `omdb`.
    pub provider: String,
    /// Provider-specific id, stored as a string.
    pub id: String,
    pub title: String,
    pub year: Option<u16>,
    #[serde(default)]
    pub media_type: MediaType,
    pub runtime_minutes: Option<u32>,
    pub popularity: Option<f64>,
    pub overview: Option<String>,
}

impl MatchCandidate {
    pub fn new(
        provider: impl Into<String>,
        id: impl Into<String>,
        title: impl Into<String>,
        year: Option<u16>,
    ) -> Self {
        Self {
            provider: provider.into(),
            id: id.into(),
            title: title.into(),
            year,
            media_type: MediaType::Movie,
            runtime_minutes: None,
            popularity: None,
            overview: None,
        }
    }

    pub fn new_tv(
        provider: impl Into<String>,
        id: impl Into<String>,
        title: impl Into<String>,
        year: Option<u16>,
    ) -> Self {
        Self {
            media_type: MediaType::Tv,
            ..Self::new(provider, id, title, year)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchDecision {
    /// Safe to rename without asking.
    AutoAccept,
    /// Plausible, but the user should confirm.
    NeedsReview,
    /// No usable candidate.
    Unmatched,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchResult {
    pub candidate: Option<MatchCandidate>,
    /// 0.0 - 1.0 combined score.
    pub score: f32,
    pub title_score: f32,
    pub year_score: Option<f32>,
    pub runtime_score: Option<f32>,
    pub decision: MatchDecision,
    pub reasons: Vec<String>,
}

impl MatchResult {
    pub fn unmatched(reason: impl Into<String>) -> Self {
        Self {
            candidate: None,
            score: 0.0,
            title_score: 0.0,
            year_score: None,
            runtime_score: None,
            decision: MatchDecision::Unmatched,
            reasons: vec![reason.into()],
        }
    }
}

/// Sidecar metadata needed by the planner/formatter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidecarInfo {
    pub kind: MediaKind,
    /// Language tag such as `en`, `en-US`, `eng`.
    pub language: Option<String>,
    /// Extra tags such as `forced`, `sdh`, `hi`.
    pub flags: Vec<String>,
    /// Suffix appended to the video base name. Examples: `.en`, `.en.forced`,
    /// `-poster`, or an empty string for `.nfo`.
    pub suffix: String,
    /// Extension without a leading dot.
    pub extension: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanStatus {
    /// Match is confident; safe to auto-apply.
    Ready,
    /// Match is plausible but needs user review.
    Review,
    /// No metadata match.
    Unmatched,
    /// Target collision, existing file, or invalid target.
    Conflict,
    /// Nothing to do (already correctly named, disabled, or skipped).
    Skipped,
}

impl PlanStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            PlanStatus::Ready => "ready",
            PlanStatus::Review => "review",
            PlanStatus::Unmatched => "unmatched",
            PlanStatus::Conflict => "conflict",
            PlanStatus::Skipped => "skipped",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictKind {
    DuplicateTarget,
    TargetExists,
    InvalidTarget,
    VideoConflict,
}

/// One source -> target rename operation (video or sidecar).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanItem {
    pub id: String,
    /// For sidecars, the id of the owning video item.
    pub parent_id: Option<String>,
    pub source: PathBuf,
    pub target: PathBuf,
    pub status: PlanStatus,
    pub score: Option<f32>,
    pub parsed: ParsedMedia,
    pub candidate: Option<MatchCandidate>,
    pub sidecar: Option<SidecarInfo>,
    pub is_sidecar: bool,
    /// Source and target differ only by case. The executor uses a two-step
    /// rename so this works on case-insensitive filesystems.
    pub case_only: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanSummary {
    pub total: usize,
    pub ready: usize,
    pub review: usize,
    pub unmatched: usize,
    pub conflicts: usize,
    pub skipped: usize,
    pub sidecars: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenamePlan {
    pub root: PathBuf,
    pub items: Vec<PlanItem>,
    pub created_at: u64,
    pub summary: PlanSummary,
}

impl RenamePlan {
    pub fn recompute_summary(&mut self) {
        let mut summary = PlanSummary {
            total: self.items.len(),
            ..PlanSummary::default()
        };
        for item in &self.items {
            match item.status {
                PlanStatus::Ready => summary.ready += 1,
                PlanStatus::Review => summary.review += 1,
                PlanStatus::Unmatched => summary.unmatched += 1,
                PlanStatus::Conflict => summary.conflicts += 1,
                PlanStatus::Skipped => summary.skipped += 1,
            }
            if item.is_sidecar {
                summary.sidecars += 1;
            }
        }
        self.summary = summary;
    }

    pub fn item(&self, id: &str) -> Option<&PlanItem> {
        self.items.iter().find(|item| item.id == id)
    }
}
