use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

use crate::model::{MediaType, ParsedMedia, PartMarker};

static YEAR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|[^0-9])((?:19|20)\d{2})").expect("year regex"));
static PART_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[\s._\-])(cd|part|disc|disk)[\s._\-]*([1-9])(?:[\s._\-]|$)")
        .expect("part regex")
});
static GROUP_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"-([A-Z0-9]{2,})$").expect("group regex"));
static RES_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[^a-z0-9])(2160p|1080p|720p|480p|576p|4k|uhd)(?:[^a-z0-9]|$)")
        .expect("resolution regex")
});
static SOURCE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?:^|[^a-z0-9])(blu-?ray|bdrip|brrip|web-?dl|webrip|web|hdtv|dvdrip|dvd|remux|hddvd|hdcam|cam)(?:[^a-z0-9]|$)",
    )
    .expect("source regex")
});
static CODEC_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?:^|[^a-z0-9])(x264|h\.?264|avc|x265|h\.?265|hevc|av1|vp9|xvid|divx)(?:[^a-z0-9]|$)",
    )
    .expect("codec regex")
});
static TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)\b(?:
            2160p|1080p|720p|480p|576p|4k|uhd|
            blu-?ray|bdrip|brrip|web-?dl|webdl|webrip|web|hdtv|dvdrip|dvd|remux|hddvd|hdcam|cam|
            x264|h\.?264|avc|x265|h\.?265|hevc|av1|vp9|xvid|divx|
            hdr10\+?|hdr|dolby[\s._-]*vision|sdr|10bit|8bit|
            aac|ac3|eac3|ddp?[\s._-]*\d[\s._-]*\d|atmos|truehd|dts(?:-hd)?|flac|mp3|
            repack|proper|rerip|internal|limited|complete|extended|unrated|theatrical|imax|remastered|
            yts|yify|rarbg|evo|ntb|fgt|galaxyrg|ion10|tigole|qxr|psa|anoxmous|
            korean|english|hindi|dual[\s._-]*audio|multi|subbed|dubbed|
            ita|fre|ger|spa|jpn|kor|eng|hin|tamil|telugu|forced|sdh|cc|default|foreign
        )\b",
    )
    .expect("tag regex")
});
static SIDECAR_SUFFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)(?:[._\-\s](?:
            en|eng|english|fr|fre|french|de|ger|german|es|spa|spanish|it|ita|italian|
            pt|por|portuguese|ru|rus|russian|ja|jpn|japanese|ko|kor|korean|
            zh|chi|chinese|hi|hin|hindi|ta|tam|tamil|te|tel|telugu|
            nl|dut|dutch|pl|pol|polish|sv|swe|swedish|no|nor|norwegian|
            da|dan|danish|fi|fin|finnish|tr|tur|turkish|ar|ara|arabic|he|heb|hebrew|
            forced|sdh|cc|default|foreign|commentary
        ))$",
    )
    .expect("sidecar suffix regex")
});
static TV_SXXEYY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)(?:^|[^a-z0-9])s(\d{1,2})[\s._-]*e(\d{1,3})(?:(?:[\s._-]*-?e|[\s._-]+)(\d{1,3}))?(?:[^a-z0-9]|$)",
    )
    .expect("SxxEyy regex")
});
static TV_NXNN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?ix)(?:^|[^a-z0-9])(\d{1,2})x(\d{2,3})(?:[^a-z0-9]|$)").expect("NxNN regex")
});
static TV_SEASON_EPISODE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)(?:^|[^a-z0-9])season[\s._-]*(\d{1,2})[\s._-]*(?:episode|ep|e)[\s._-]*(\d{1,3})(?:[^a-z0-9]|$)",
    )
    .expect("Season Episode regex")
});
static TV_EPISODE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?ix)(?:^|[^a-z0-9])(?:episode|ep)[\s._-]*(\d{1,3})(?:[^a-z0-9]|$)")
        .expect("Episode regex")
});
static TV_EPISODE_PREFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?ix)^[\s._-]*(?:e|ep|episode)?[\s._-]*(\d{1,3})(?:[\s._-]|$)")
        .expect("episode prefix regex")
});
static SEASON_FOLDER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:season|series)[\s._-]*(\d{1,2})$").expect("season folder regex")
});
static SHORT_SEASON_FOLDER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^s(\d{1,2})$").expect("short season folder regex"));
static SPECIALS_FOLDER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(?:specials?|season[\s._-]*0)$").expect("specials regex"));

static EDITION_PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    let entries: Vec<(&str, &'static str)> = vec![
        (r"(?i)\bdirector'?s?[\s._-]*cut\b", "Director's Cut"),
        (r"(?i)\bextended[\s._-]*edition\b", "Extended Edition"),
        (r"(?i)\bspecial[\s._-]*edition\b", "Special Edition"),
        (r"(?i)\bultimate[\s._-]*edition\b", "Ultimate Edition"),
        (
            r"(?i)\bcollector'?s?[\s._-]*edition\b",
            "Collector's Edition",
        ),
        (r"(?i)\bfinal[\s._-]*cut\b", "Final Cut"),
        (r"(?i)\btheatrical(?:[\s._-]*cut)?\b", "Theatrical"),
        (r"(?i)\bextended\b", "Extended"),
        (r"(?i)\bunrated\b", "Unrated"),
        (r"(?i)\bimax\b", "IMAX"),
        (r"(?i)\bremastered\b", "Remastered"),
        (r"(?i)\bcriterion\b", "Criterion"),
        (r"(?i)\buncut\b", "Uncut"),
    ];
    entries
        .into_iter()
        .map(|(pattern, label)| (Regex::new(pattern).expect("edition regex"), label))
        .collect()
});
static BRACKETS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\[\](){}]").expect("bracket regex"));
static SEP_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[._]+").expect("separator regex"));
static MULTI_SPACE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").expect("space regex"));

const GENERIC_FOLDERS: &[&str] = &[
    "movies",
    "movie",
    "films",
    "film",
    "videos",
    "video",
    "downloads",
    "download",
    "media",
    "library",
    "tv",
    "tv shows",
    "series",
    "anime",
    "unsorted",
    "incoming",
    "complete",
    "new folder",
];

/// Parse a filename such as `The.Matrix.1999.1080p.BluRay.x264-YTS.mkv`.
/// `parent_hint` is usually the containing folder name.
pub fn parse_filename(file_name: &str, parent_hint: Option<&str>) -> ParsedMedia {
    parse_filename_with_context(file_name, parent_hint, None)
}

/// Parse a filename with both the immediate parent and grandparent folders.
///
/// TV libraries commonly use `Series (Year)/Season 01/Series S01E02.mkv`, so
/// the grandparent supplies the series title when the parent is a season
/// folder.
pub fn parse_filename_with_context(
    file_name: &str,
    parent_hint: Option<&str>,
    grandparent_hint: Option<&str>,
) -> ParsedMedia {
    let stem = Path::new(file_name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| file_name.to_string());
    let stem = strip_sidecar_suffix(&stem);
    parse_stem(&stem, parent_hint, grandparent_hint)
}

/// Parse a path, using its parent directory name as the folder hint.
pub fn parse_path(path: &Path) -> ParsedMedia {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let parent = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().to_string());
    let grandparent = path
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().to_string());
    parse_filename_with_context(&name, parent.as_deref(), grandparent.as_deref())
}

fn parse_stem(
    stem: &str,
    parent_hint: Option<&str>,
    grandparent_hint: Option<&str>,
) -> ParsedMedia {
    let parent_season = parent_hint.and_then(season_from_folder);
    // When the parent is a season folder, the series title/year live in the
    // grandparent folder.
    let series_hint = if parent_season.is_some() {
        grandparent_hint
    } else {
        parent_hint
    };

    let mut parsed = parse_core(stem, parent_season);
    let mut used_parent = false;

    if let Some(hint) = series_hint {
        let parent = parse_core(hint, None);
        let parent_title = parent.title.trim();
        if !parent_title.is_empty() && !is_generic_folder(parent_title) {
            if parsed.title.trim().is_empty() || is_generic_folder(&parsed.title) {
                parsed.title = parent_title.to_string();
                used_parent = true;
            }
            if parsed.year.is_none() {
                parsed.year = parent.year;
                used_parent = true;
            }
        }
    }

    if parsed.season.is_none() {
        parsed.season = parent_season;
    }
    if parsed.media_type == MediaType::Movie && parent_season.is_some() {
        // A video directly inside a season folder is still a TV file even when
        // the episode number is missing. The planner marks it for review.
        parsed.media_type = MediaType::Tv;
    }

    if parsed.title.trim().is_empty() {
        parsed.title = "Untitled".to_string();
    }

    let mut confidence = 0.35_f32;
    if parsed.year.is_some() {
        confidence += 0.25;
    }
    if parsed.title.split_whitespace().count() >= 2 {
        confidence += 0.15;
    }
    if parsed.quality.is_some() || parsed.source.is_some() || parsed.codec.is_some() {
        confidence += 0.15;
    }
    if parsed.part.is_some() || parsed.edition.is_some() {
        confidence += 0.05;
    }
    if parsed.media_type == MediaType::Tv {
        confidence += 0.05;
        if !parsed.episodes.is_empty() {
            confidence += 0.05;
        }
    }
    if used_parent {
        confidence = (confidence - 0.05).max(0.3);
    }
    parsed.confidence = confidence.min(1.0);
    parsed
}

fn parse_core(stem: &str, parent_season: Option<u16>) -> ParsedMedia {
    let working = stem.nfc().collect::<String>();
    let marker = extract_tv_marker(&working, parent_season);

    if let Some(marker) = marker {
        let prefix = &working[..marker.start];
        let suffix = &working[marker.end..];
        // Keep metadata tags from both sides of the episode marker, but take
        // the title and year only from the prefix (the series part).
        let mut combined = format!("{prefix} {suffix}");
        let release_group = extract_release_group(&mut combined);
        let part = extract_part(&mut combined);
        let edition = extract_edition(&mut combined);
        let quality = extract_resolution(&combined);
        let source = extract_source(&combined);
        let codec = extract_codec(&combined);
        let year = extract_year(prefix);
        let title = clean_title(prefix);
        let episode_title = clean_episode_title(suffix);

        return ParsedMedia {
            title,
            year: year.map(|(year, _)| year),
            media_type: MediaType::Tv,
            season: Some(marker.season),
            episodes: marker.episodes,
            episode_title,
            quality,
            source,
            codec,
            edition,
            part,
            release_group,
            confidence: 0.0,
        };
    }

    let mut combined = working;
    let release_group = extract_release_group(&mut combined);
    let part = extract_part(&mut combined);
    let edition = extract_edition(&mut combined);
    let quality = extract_resolution(&combined);
    let source = extract_source(&combined);
    let codec = extract_codec(&combined);
    let year = extract_year(&combined);

    let title_source = match year {
        Some((_, start)) => &combined[..start],
        None => &combined[..],
    };
    let title = clean_title(title_source);

    ParsedMedia {
        title,
        year: year.map(|(year, _)| year),
        media_type: MediaType::Movie,
        season: None,
        episodes: Vec::new(),
        episode_title: None,
        quality,
        source,
        codec,
        edition,
        part,
        release_group,
        confidence: 0.0,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TvMarker {
    season: u16,
    episodes: Vec<u16>,
    start: usize,
    end: usize,
}

fn extract_tv_marker(input: &str, parent_season: Option<u16>) -> Option<TvMarker> {
    if let Some(caps) = TV_SXXEYY_RE.captures(input) {
        let season = caps.get(1)?.as_str().parse().ok()?;
        let first = caps.get(2)?.as_str().parse().ok()?;
        let mut episodes = vec![first];
        if let Some(second) = caps.get(3).and_then(|mat| mat.as_str().parse::<u16>().ok()) {
            if !episodes.contains(&second) {
                episodes.push(second);
            }
        }
        let mat = caps.get(0)?;
        return Some(TvMarker {
            season,
            episodes,
            start: mat.start(),
            end: mat.end(),
        });
    }

    if let Some(caps) = TV_NXNN_RE.captures(input) {
        let season = caps.get(1)?.as_str().parse().ok()?;
        let episode = caps.get(2)?.as_str().parse().ok()?;
        let mat = caps.get(0)?;
        return Some(TvMarker {
            season,
            episodes: vec![episode],
            start: mat.start(),
            end: mat.end(),
        });
    }

    if let Some(caps) = TV_SEASON_EPISODE_RE.captures(input) {
        let season = caps.get(1)?.as_str().parse().ok()?;
        let episode = caps.get(2)?.as_str().parse().ok()?;
        let mat = caps.get(0)?;
        return Some(TvMarker {
            season,
            episodes: vec![episode],
            start: mat.start(),
            end: mat.end(),
        });
    }

    if let Some(caps) = TV_EPISODE_RE.captures(input) {
        let episode = caps.get(1)?.as_str().parse().ok()?;
        let mat = caps.get(0)?;
        return Some(TvMarker {
            season: parent_season.unwrap_or(1),
            episodes: vec![episode],
            start: mat.start(),
            end: mat.end(),
        });
    }

    // Files such as `02 - Episode Title.mkv` or `E02.mkv` inside a season
    // folder have no season marker of their own.
    if let Some(season) = parent_season {
        if let Some(caps) = TV_EPISODE_PREFIX_RE.captures(input) {
            let episode = caps.get(1)?.as_str().parse().ok()?;
            let mat = caps.get(0)?;
            return Some(TvMarker {
                season,
                episodes: vec![episode],
                start: mat.start(),
                end: mat.end(),
            });
        }
    }

    None
}

fn season_from_folder(name: &str) -> Option<u16> {
    let trimmed = name.trim();
    if SPECIALS_FOLDER_RE.is_match(trimmed) {
        return Some(0);
    }
    SEASON_FOLDER_RE
        .captures(trimmed)
        .or_else(|| SHORT_SEASON_FOLDER_RE.captures(trimmed))
        .and_then(|caps| caps.get(1)?.as_str().parse().ok())
}

fn clean_episode_title(raw: &str) -> Option<String> {
    if raw.trim().is_empty() {
        return None;
    }
    let mut working = raw.to_string();
    let _ = extract_release_group(&mut working);
    let cleaned = clean_title(&working);
    if cleaned.is_empty() {
        return None;
    }
    if cleaned
        .chars()
        .all(|c| c.is_ascii_digit() || c.is_whitespace())
    {
        return None;
    }
    const JUNK: &[&str] = &[
        "repack",
        "proper",
        "rerip",
        "internal",
        "limited",
        "complete",
        "multi",
        "dual audio",
        "subbed",
        "dubbed",
    ];
    if JUNK.iter().any(|junk| cleaned.eq_ignore_ascii_case(junk)) {
        return None;
    }
    Some(cleaned)
}

fn extract_year(input: &str) -> Option<(u16, usize)> {
    let mut found = None;
    for caps in YEAR_RE.captures_iter(input) {
        let Some(mat) = caps.get(1) else { continue };
        let next_is_digit = input[mat.end()..]
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false);
        if next_is_digit {
            continue;
        }
        if let Ok(year) = mat.as_str().parse::<u16>() {
            if (1900..=2099).contains(&year) {
                found = Some((year, mat.start()));
            }
        }
    }
    found
}

fn extract_part(input: &mut String) -> Option<PartMarker> {
    let mut result = None;
    for caps in PART_RE.captures_iter(input) {
        let label = caps.get(1).map(|m| m.as_str().to_ascii_lowercase());
        let index = caps.get(2).and_then(|m| m.as_str().parse::<u32>().ok());
        if let (Some(label), Some(index)) = (label, index) {
            result = Some(PartMarker {
                index,
                label: if label == "disk" {
                    "disc".to_string()
                } else {
                    label
                },
            });
        }
    }
    if result.is_some() {
        *input = PART_RE.replace_all(input, " ").to_string();
    }
    result
}

fn extract_edition(input: &mut String) -> Option<String> {
    for (pattern, label) in EDITION_PATTERNS.iter() {
        if pattern.is_match(input) {
            *input = pattern.replace_all(input, " ").to_string();
            return Some((*label).to_string());
        }
    }
    None
}

fn extract_release_group(input: &mut String) -> Option<String> {
    let trimmed = input.trim_end();
    if let Some(caps) = GROUP_RE.captures(trimmed) {
        let group = caps.get(1)?.as_str().to_string();
        let start = caps.get(0)?.start();
        input.truncate(start);
        return Some(group);
    }
    None
}

fn extract_resolution(input: &str) -> Option<String> {
    RES_RE.captures(input).and_then(|caps| {
        let mat = caps.get(1)?;
        Some(match mat.as_str().to_ascii_lowercase().as_str() {
            "4k" | "uhd" => "2160p".to_string(),
            other => other.to_string(),
        })
    })
}

fn extract_source(input: &str) -> Option<String> {
    SOURCE_RE.captures(input).and_then(|caps| {
        let mat = caps.get(1)?;
        let raw = mat.as_str().to_ascii_lowercase().replace(['.', '-'], "");
        Some(match raw.as_str() {
            "bluray" | "bdrip" | "brrip" => "BluRay".to_string(),
            "webdl" | "webrip" | "web" => "WEB-DL".to_string(),
            "hdtv" => "HDTV".to_string(),
            "dvdrip" | "dvd" => "DVD".to_string(),
            "remux" => "REMUX".to_string(),
            "hddvd" => "HD-DVD".to_string(),
            "hdcam" | "cam" => "CAM".to_string(),
            _ => mat.as_str().to_string(),
        })
    })
}

fn extract_codec(input: &str) -> Option<String> {
    CODEC_RE.captures(input).and_then(|caps| {
        let mat = caps.get(1)?;
        let raw = mat.as_str().to_ascii_lowercase().replace('.', "");
        Some(match raw.as_str() {
            "h264" | "avc" => "x264".to_string(),
            "h265" | "hevc" => "x265".to_string(),
            other => other.to_string(),
        })
    })
}

fn clean_title(raw: &str) -> String {
    let mut title = raw.nfc().collect::<String>();
    title = SEP_RE.replace_all(&title, " ").to_string();
    title = BRACKETS_RE.replace_all(&title, " ").to_string();
    title = TAG_RE.replace_all(&title, " ").to_string();
    title = title.replace(" - ", " ");
    title = MULTI_SPACE_RE.replace_all(&title, " ").to_string();
    title = title
        .trim()
        .trim_matches(|c: char| c == '-' || c == '_' || c == '.' || c == ' ')
        .to_string();
    smart_title_case(&title)
}

fn smart_title_case(input: &str) -> String {
    const SMALL: &[&str] = &[
        "a", "an", "and", "as", "at", "but", "by", "for", "from", "in", "into", "nor", "of", "on",
        "or", "over", "the", "to", "up", "via", "vs", "with",
    ];
    let words: Vec<&str> = input.split_whitespace().collect();
    let last = words.len().saturating_sub(1);
    let mut out = Vec::with_capacity(words.len());
    for (index, word) in words.iter().enumerate() {
        let lower = word.to_lowercase();
        let keep_lower = if index != 0 && index != last && SMALL.contains(&lower.as_str()) {
            // "The" starts a subtitle after a major word, but stays lowercase
            // in phrases such as "of the".
            if lower == "the" {
                let previous = words[index - 1].to_lowercase();
                matches!(
                    previous.as_str(),
                    "of" | "in"
                        | "on"
                        | "at"
                        | "for"
                        | "from"
                        | "with"
                        | "to"
                        | "and"
                        | "or"
                        | "a"
                        | "an"
                )
            } else {
                true
            }
        } else {
            false
        };
        if keep_lower {
            out.push(lower);
        } else {
            out.push(capitalize_word(word));
        }
    }
    out.join(" ")
}

fn capitalize_word(word: &str) -> String {
    word.split('-')
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => first
                    .to_uppercase()
                    .chain(chars.flat_map(|c| c.to_lowercase()))
                    .collect(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join("-")
}

fn is_generic_folder(title: &str) -> bool {
    let lower = title.trim().to_lowercase();
    GENERIC_FOLDERS.contains(&lower.as_str())
}

fn strip_sidecar_suffix(stem: &str) -> String {
    let mut current = stem.to_string();
    loop {
        let next = SIDECAR_SUFFIX_RE.replace(&current, "").to_string();
        if next == current {
            break;
        }
        current = next;
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(name: &str) -> ParsedMedia {
        parse_filename(name, None)
    }

    #[test]
    fn parses_matrix() {
        let p = parse("The.Matrix.1999.1080p.BluRay.x264-YTS.mkv");
        assert_eq!(p.title, "The Matrix");
        assert_eq!(p.year, Some(1999));
        assert_eq!(p.quality.as_deref(), Some("1080p"));
        assert_eq!(p.source.as_deref(), Some("BluRay"));
        assert_eq!(p.codec.as_deref(), Some("x264"));
        assert_eq!(p.release_group.as_deref(), Some("YTS"));
    }

    #[test]
    fn parses_underscores_and_webdl() {
        let p = parse("inception_2010_720p_web-dl.mp4");
        assert_eq!(p.title, "Inception");
        assert_eq!(p.year, Some(2010));
        assert_eq!(p.quality.as_deref(), Some("720p"));
        assert_eq!(p.source.as_deref(), Some("WEB-DL"));
    }

    #[test]
    fn parses_extended_edition() {
        let p = parse("Avatar.2009.EXTENDED.2160p.HDR.REMUX.mkv");
        assert_eq!(p.title, "Avatar");
        assert_eq!(p.year, Some(2009));
        assert_eq!(p.quality.as_deref(), Some("2160p"));
        assert_eq!(p.edition.as_deref(), Some("Extended"));
    }

    #[test]
    fn parses_cd_part() {
        let p = parse("Dune 2021 CD1.avi");
        assert_eq!(p.title, "Dune");
        assert_eq!(p.year, Some(2021));
        let part = p.part.expect("part");
        assert_eq!(part.index, 1);
        assert_eq!(part.label, "cd");
    }

    #[test]
    fn parses_part_word() {
        let p = parse("A.Movie.2020.Part.2.1080p.mkv");
        assert_eq!(p.title, "A Movie");
        assert_eq!(p.part.unwrap().index, 2);
    }

    #[test]
    fn keeps_year_in_title_when_a_later_year_exists() {
        let p = parse("Blade.Runner.2049.2017.1080p.BluRay.x264.mkv");
        assert_eq!(p.title, "Blade Runner 2049");
        assert_eq!(p.year, Some(2017));
    }

    #[test]
    fn parses_title_that_is_a_year() {
        let p = parse("1917.2019.2160p.WEB-DL.mkv");
        assert_eq!(p.title, "1917");
        assert_eq!(p.year, Some(2019));
    }

    #[test]
    fn parses_hyphenated_title() {
        let p = parse("Spider-Man.No.Way.Home.2021.2160p.WEB-DL.mkv");
        assert_eq!(p.title, "Spider-Man No Way Home");
        assert_eq!(p.year, Some(2021));
    }

    #[test]
    fn parses_directors_cut() {
        let p = parse("Some.Movie.2021.Directors.Cut.REMASTERED.1080p.mkv");
        assert_eq!(p.title, "Some Movie");
        assert_eq!(p.edition.as_deref(), Some("Director's Cut"));
    }

    #[test]
    fn uses_parent_folder_year() {
        let p = parse_filename(
            "The.Matrix.1080p.BluRay.x264.mkv",
            Some("The Matrix (1999)"),
        );
        assert_eq!(p.title, "The Matrix");
        assert_eq!(p.year, Some(1999));
    }

    #[test]
    fn uses_parent_title_when_file_has_none() {
        let p = parse_filename("1080p.mkv", Some("The Matrix (1999)"));
        assert_eq!(p.title, "The Matrix");
        assert_eq!(p.year, Some(1999));
    }

    #[test]
    fn ignores_generic_parent_folder() {
        let p = parse_filename("holiday_video_final_v2.mp4", Some("Movies"));
        assert_eq!(p.title, "Holiday Video Final V2");
        assert_eq!(p.year, None);
    }

    #[test]
    fn strips_sidecar_language() {
        let p = parse("The.Matrix.1999.1080p.BluRay.x264-YTS.en.srt");
        assert_eq!(p.title, "The Matrix");
        assert_eq!(p.year, Some(1999));
    }

    #[test]
    fn parses_parasite() {
        let p = parse("Parasite.2019.KOREAN.1080p.BluRay.mkv");
        assert_eq!(p.title, "Parasite");
        assert_eq!(p.year, Some(2019));
    }

    #[test]
    fn parses_multi_word_small_words() {
        let p = parse(
            "The.Lord.of.the.Rings.The.Fellowship.of.the.Ring.2001.EXTENDED.1080p.BluRay.x265.mkv",
        );
        assert_eq!(p.title, "The Lord of the Rings The Fellowship of the Ring");
        assert_eq!(p.year, Some(2001));
        assert_eq!(p.edition.as_deref(), Some("Extended"));
    }

    #[test]
    fn confidence_is_high_for_rich_names() {
        let p = parse("The.Matrix.1999.1080p.BluRay.x264-YTS.mkv");
        assert!(p.confidence > 0.8, "confidence was {}", p.confidence);
    }

    #[test]
    fn confidence_is_low_for_no_year() {
        let p = parse("holiday_video_final_v2.mp4");
        assert!(p.confidence < 0.8, "confidence was {}", p.confidence);
    }

    #[test]
    fn parses_sxxeyy_with_episode_title() {
        let p = parse("The.Office.US.S01E02.The.Dundies.1080p.WEB-DL.mkv");
        assert_eq!(p.media_type, MediaType::Tv);
        assert_eq!(p.title, "The Office Us");
        assert_eq!(p.season, Some(1));
        assert_eq!(p.episodes, vec![2]);
        assert_eq!(p.episode_title.as_deref(), Some("The Dundies"));
        assert_eq!(p.quality.as_deref(), Some("1080p"));
        assert_eq!(p.source.as_deref(), Some("WEB-DL"));
    }

    #[test]
    fn parses_nxnn() {
        let p = parse("Show.Name.1x02.720p.mkv");
        assert_eq!(p.media_type, MediaType::Tv);
        assert_eq!(p.title, "Show Name");
        assert_eq!(p.season, Some(1));
        assert_eq!(p.episodes, vec![2]);
    }

    #[test]
    fn parses_season_episode_words() {
        let p = parse("Show Name Season 2 Episode 5 - The Reunion.mkv");
        assert_eq!(p.media_type, MediaType::Tv);
        assert_eq!(p.title, "Show Name");
        assert_eq!(p.season, Some(2));
        assert_eq!(p.episodes, vec![5]);
        assert_eq!(p.episode_title.as_deref(), Some("The Reunion"));
    }

    #[test]
    fn parses_multi_episode_files() {
        for name in [
            "Show.S01E02E03.mkv",
            "Show.S01E02-E03.mkv",
            "Show.S01E02-03.mkv",
        ] {
            let p = parse(name);
            assert_eq!(p.media_type, MediaType::Tv, "{name}");
            assert_eq!(p.season, Some(1), "{name}");
            assert_eq!(p.episodes, vec![2, 3], "{name}");
        }
    }

    #[test]
    fn uses_season_and_series_folders() {
        let p = parse_filename_with_context(
            "03 - The Reunion.mkv",
            Some("Season 02"),
            Some("Show Name (2020)"),
        );
        assert_eq!(p.media_type, MediaType::Tv);
        assert_eq!(p.title, "Show Name");
        assert_eq!(p.year, Some(2020));
        assert_eq!(p.season, Some(2));
        assert_eq!(p.episodes, vec![3]);
        assert_eq!(p.episode_title.as_deref(), Some("The Reunion"));
    }

    #[test]
    fn uses_grandparent_for_series_title() {
        let p = parse_filename_with_context(
            "S02E03.1080p.mkv",
            Some("Season 02"),
            Some("Show Name (2020)"),
        );
        assert_eq!(p.title, "Show Name");
        assert_eq!(p.year, Some(2020));
        assert_eq!(p.season, Some(2));
        assert_eq!(p.episodes, vec![3]);
    }

    #[test]
    fn parses_specials_folder() {
        let p = parse_filename_with_context(
            "S00E01.Christmas.Special.mkv",
            Some("Specials"),
            Some("Show Name (2020)"),
        );
        assert_eq!(p.media_type, MediaType::Tv);
        assert_eq!(p.season, Some(0));
        assert_eq!(p.episodes, vec![1]);
        assert_eq!(p.episode_title.as_deref(), Some("Christmas Special"));
    }

    #[test]
    fn episode_prefix_does_not_eat_years() {
        let p = parse_filename_with_context(
            "2019.1080p.mkv",
            Some("Season 01"),
            Some("Show Name (2019)"),
        );
        assert_eq!(p.media_type, MediaType::Tv);
        assert_eq!(p.episodes, Vec::<u16>::new());
        assert_eq!(p.title, "Show Name");
    }

    #[test]
    fn keeps_movies_as_movies() {
        let p = parse("The.Matrix.1999.1080p.BluRay.x264-YTS.mkv");
        assert_eq!(p.media_type, MediaType::Movie);
        assert!(p.season.is_none());
        assert!(p.episodes.is_empty());
    }
}
