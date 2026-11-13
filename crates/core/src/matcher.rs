use std::sync::LazyLock;

use regex::Regex;
use strsim::jaro_winkler;
use unicode_normalization::UnicodeNormalization;

use crate::model::{MatchCandidate, MatchDecision, MatchResult, ParsedMedia};

static NON_ALNUM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[^\p{L}\p{N}]+").expect("title normalization regex"));

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MatchThresholds {
    /// At or above this score a match is auto-accepted.
    pub auto_accept: f32,
    /// At or above this score a match is shown for review.
    pub review: f32,
}

impl Default for MatchThresholds {
    fn default() -> Self {
        Self {
            auto_accept: 0.85,
            review: 0.55,
        }
    }
}

/// Lowercase, NFC, punctuation-free form used for fuzzy comparison.
pub fn normalize_title(input: &str) -> String {
    let normalized: String = input.nfc().collect();
    let lowercase = normalized.to_lowercase();
    let replaced = NON_ALNUM.replace_all(&lowercase, " ");
    replaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Score one candidate against the parsed filename.
pub fn score_candidate(
    parsed: &ParsedMedia,
    candidate: &MatchCandidate,
    local_runtime_minutes: Option<u32>,
    thresholds: MatchThresholds,
) -> MatchResult {
    let parsed_title = normalize_title(&parsed.title);
    let candidate_title = normalize_title(&candidate.title);
    let mut reasons = Vec::new();

    let title_score = if parsed_title.is_empty() || candidate_title.is_empty() {
        0.0
    } else {
        jaro_winkler(&parsed_title, &candidate_title) as f32
    };

    if title_score >= 0.98 {
        reasons.push("title matches exactly".to_string());
    } else if title_score >= 0.8 {
        reasons.push(format!(
            "title is a close match ({:.0}%)",
            title_score * 100.0
        ));
    }

    let year_score = match (parsed.year, candidate.year) {
        (Some(a), Some(b)) => {
            let diff = a.abs_diff(b);
            let score = match diff {
                0 => 1.0,
                1 => 0.6,
                _ => 0.0,
            };
            if diff == 0 {
                reasons.push(format!("year matches ({a})"));
            } else if diff == 1 {
                reasons.push(format!("year is off by one ({a} vs {b})"));
            } else {
                reasons.push(format!("year differs ({a} vs {b})"));
            }
            Some(score)
        }
        _ => None,
    };

    let runtime_score = match (local_runtime_minutes, candidate.runtime_minutes) {
        (Some(local), Some(candidate_runtime)) if local > 0 && candidate_runtime > 0 => {
            let diff = local.abs_diff(candidate_runtime);
            let score = if diff <= 2 {
                1.0
            } else if diff <= 5 {
                0.6
            } else {
                0.0
            };
            if diff <= 2 {
                reasons.push("runtime matches".to_string());
            }
            Some(score)
        }
        _ => None,
    };

    let mut score = match (year_score, runtime_score) {
        (Some(year), Some(runtime)) => title_score * 0.70 + year * 0.25 + runtime * 0.05,
        (Some(year), None) => title_score * 0.75 + year * 0.25,
        (None, Some(runtime)) => title_score * 0.95 + runtime * 0.05,
        (None, None) => title_score,
    };

    // A wrong year is a strong signal even when the title is similar.
    if let Some(year) = year_score {
        if year == 0.0 {
            score *= 0.55;
        }
    }
    score = score.clamp(0.0, 1.0);

    let decision = if score >= thresholds.auto_accept {
        MatchDecision::AutoAccept
    } else if score >= thresholds.review {
        MatchDecision::NeedsReview
    } else {
        MatchDecision::Unmatched
    };

    MatchResult {
        candidate: Some(candidate.clone()),
        score,
        title_score,
        year_score,
        runtime_score,
        decision,
        reasons,
    }
}

/// Pick the highest-scoring candidate. Ties break on popularity and then year.
pub fn best_match(
    parsed: &ParsedMedia,
    candidates: &[MatchCandidate],
    local_runtime_minutes: Option<u32>,
    thresholds: MatchThresholds,
) -> MatchResult {
    if candidates.is_empty() {
        return MatchResult::unmatched("no metadata candidates");
    }

    // Never mix movies and TV series, even when their titles are identical.
    let matching: Vec<&MatchCandidate> = candidates
        .iter()
        .filter(|candidate| candidate.media_type == parsed.media_type)
        .collect();
    if matching.is_empty() {
        return MatchResult::unmatched("no metadata candidates for this media type");
    }

    let mut scored: Vec<MatchResult> = matching
        .iter()
        .map(|candidate| score_candidate(parsed, candidate, local_runtime_minutes, thresholds))
        .collect();

    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let ap = a
                    .candidate
                    .as_ref()
                    .and_then(|c| c.popularity)
                    .unwrap_or(0.0);
                let bp = b
                    .candidate
                    .as_ref()
                    .and_then(|c| c.popularity)
                    .unwrap_or(0.0);
                bp.partial_cmp(&ap).unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    scored.remove(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(title: &str, year: Option<u16>) -> MatchCandidate {
        MatchCandidate::new("tmdb", "1", title, year)
    }

    #[test]
    fn normalizes_titles() {
        assert_eq!(normalize_title("The.Matrix!"), "the matrix");
        assert_eq!(normalize_title("Am\u{00e9}lie"), "am\u{00e9}lie");
    }

    #[test]
    fn exact_match_is_auto_accepted() {
        let parsed = ParsedMedia {
            title: "The Matrix".into(),
            year: Some(1999),
            ..Default::default()
        };
        let result = best_match(
            &parsed,
            &[candidate("The Matrix", Some(1999))],
            None,
            MatchThresholds::default(),
        );
        assert_eq!(result.decision, MatchDecision::AutoAccept);
        assert!(result.score > 0.95);
    }

    #[test]
    fn wrong_year_is_penalized() {
        let parsed = ParsedMedia {
            title: "Avatar".into(),
            year: Some(2009),
            ..Default::default()
        };
        let right = score_candidate(
            &parsed,
            &candidate("Avatar", Some(2009)),
            None,
            MatchThresholds::default(),
        );
        let wrong = score_candidate(
            &parsed,
            &candidate("Avatar", Some(2022)),
            None,
            MatchThresholds::default(),
        );
        assert!(right.score > wrong.score);
        assert_eq!(wrong.decision, MatchDecision::Unmatched);
    }

    #[test]
    fn fuzzy_title_needs_review() {
        let parsed = ParsedMedia {
            title: "Spider Man No Way Home".into(),
            year: Some(2021),
            ..Default::default()
        };
        let result = best_match(
            &parsed,
            &[candidate("Spider-Man: No Way Home", Some(2021))],
            None,
            MatchThresholds::default(),
        );
        assert_eq!(result.decision, MatchDecision::AutoAccept);
    }

    #[test]
    fn runtime_breaks_close_calls() {
        let parsed = ParsedMedia {
            title: "Dune".into(),
            year: Some(2021),
            ..Default::default()
        };
        let mut a = candidate("Dune", Some(2021));
        a.runtime_minutes = Some(155);
        let mut b = candidate("Dune", Some(2021));
        b.runtime_minutes = Some(90);
        b.id = "2".into();
        let result = best_match(&parsed, &[b, a], Some(155), MatchThresholds::default());
        assert_eq!(result.candidate.unwrap().runtime_minutes, Some(155));
    }

    #[test]
    fn empty_candidates_are_unmatched() {
        let parsed = ParsedMedia {
            title: "Anything".into(),
            ..Default::default()
        };
        let result = best_match(&parsed, &[], None, MatchThresholds::default());
        assert_eq!(result.decision, MatchDecision::Unmatched);
    }
}
