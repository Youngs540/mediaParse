use std::path::PathBuf;
use std::sync::LazyLock;

use regex::Regex;

use crate::model::{MatchCandidate, MediaType, ParsedMedia, SidecarInfo};
use crate::sanitizer::{sanitize_component, truncate_component};

static EMPTY_ID_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\[\s*tmdbid\s*-\s*\]|\[\s*\]|\(\s*\)|\{\s*\}").expect("empty id regex")
});
static MULTI_SPACE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s{2,}").expect("space regex"));
static DANGLING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s*-\s*$|^\s*-\s*").expect("dangling separator regex"));

#[derive(Debug, Clone)]
pub struct FormatOptions {
    /// Movie placeholders: `{title}`, `{year}`, `{id}`, `{quality}`,
    /// `{edition}`, `{part}`, `{source}`, `{codec}`, `{provider}`.
    pub template: String,
    /// TV episode file placeholders additionally include `{season}`,
    /// `{episode}`, and `{episode_title}`.
    pub tv_template: String,
    /// TV series folder template. The season folder is created beneath it.
    pub series_folder_template: String,
    pub create_movie_folders: bool,
    pub create_tv_folders: bool,
    pub max_component_len: usize,
    pub append_edition: bool,
    pub append_part: bool,
    /// Used by the planner when two files map to the same movie or episode.
    /// Example: `1080p` produces `Title (Year) - 1080p.mkv`.
    pub version_suffix: Option<String>,
    pub fallback_title: String,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            template: "{title} ({year}) [tmdbid-{id}]".to_string(),
            tv_template: "{title} ({year}) S{season}E{episode} - {episode_title}".to_string(),
            series_folder_template: "{title} ({year}) [tmdbid-{id}]".to_string(),
            create_movie_folders: true,
            create_tv_folders: true,
            max_component_len: 180,
            append_edition: true,
            append_part: true,
            version_suffix: None,
            fallback_title: "Untitled".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormattedName {
    /// Folder (or nested series/season folders) when folder creation is
    /// enabled.
    pub folder: Option<PathBuf>,
    /// Final file name including extension.
    pub file_name: String,
    /// File stem, used to derive sidecar names.
    pub base: String,
}

pub struct NameFormatter<'a> {
    options: &'a FormatOptions,
}

impl<'a> NameFormatter<'a> {
    pub fn new(options: &'a FormatOptions) -> Self {
        Self { options }
    }

    pub fn format_video(
        &self,
        parsed: &ParsedMedia,
        candidate: Option<&MatchCandidate>,
        extension: &str,
    ) -> FormattedName {
        let title = candidate
            .map(|c| c.title.as_str())
            .filter(|t| !t.trim().is_empty())
            .unwrap_or(&parsed.title);
        let title = if title.trim().is_empty() {
            &self.options.fallback_title
        } else {
            title
        };
        let year = candidate
            .and_then(|c| c.year)
            .or(parsed.year)
            .map(|y| y.to_string())
            .unwrap_or_default();
        let id = candidate.map(|c| c.id.as_str()).unwrap_or_default();
        let provider = candidate.map(|c| c.provider.as_str()).unwrap_or_default();
        let part = parsed
            .part
            .as_ref()
            .map(|p| format!("part{}", p.index))
            .unwrap_or_default();
        let quality = parsed.quality.clone().unwrap_or_default();
        let source = parsed.source.clone().unwrap_or_default();
        let codec = parsed.codec.clone().unwrap_or_default();
        let edition = parsed.edition.clone().unwrap_or_default();
        let season = parsed.season.unwrap_or(1);
        let season_value = format!("{season:02}");
        let episode_value = episode_placeholder(&parsed.episodes);
        let episode_title = parsed.episode_title.clone().unwrap_or_default();
        let extension = safe_extension(extension);

        if parsed.media_type == MediaType::Tv && !parsed.episodes.is_empty() {
            let rendered = self.render(
                &self.options.tv_template,
                &[
                    ("title", title),
                    ("year", &year),
                    ("id", id),
                    ("provider", provider),
                    ("quality", &quality),
                    ("source", &source),
                    ("codec", &codec),
                    ("edition", &edition),
                    ("part", &part),
                    ("season", &season_value),
                    ("episode", &episode_value),
                    ("episode_title", &episode_title),
                ],
            );

            let mut base = cleanup_rendered(&rendered);
            if base.is_empty() {
                base = sanitize_component(&self.options.fallback_title);
            }
            self.append_extras(
                &mut base,
                parsed,
                &edition,
                &part,
                &self.options.tv_template,
            );
            base = cleanup_rendered(&base);
            base = self.fit_component(&base, &extension);

            let folder = if self.options.create_tv_folders {
                let series_rendered = self.render(
                    &self.options.series_folder_template,
                    &[
                        ("title", title),
                        ("year", &year),
                        ("id", id),
                        ("provider", provider),
                        ("quality", &quality),
                        ("source", &source),
                        ("codec", &codec),
                        ("edition", &edition),
                        ("part", &part),
                        ("season", &season_value),
                        ("episode", &episode_value),
                        ("episode_title", &episode_title),
                    ],
                );
                let series = cleanup_rendered(&series_rendered);
                let series = if series.is_empty() {
                    sanitize_component(title)
                } else {
                    series
                };
                Some(PathBuf::from(series).join(format!("Season {season:02}")))
            } else {
                None
            };

            return FormattedName {
                folder,
                file_name: format!("{base}.{extension}"),
                base,
            };
        }

        let rendered = self.render(
            &self.options.template,
            &[
                ("title", title),
                ("year", &year),
                ("id", id),
                ("provider", provider),
                ("quality", &quality),
                ("source", &source),
                ("codec", &codec),
                ("edition", &edition),
                ("part", &part),
            ],
        );

        let mut base = cleanup_rendered(&rendered);
        if base.is_empty() {
            base = sanitize_component(&self.options.fallback_title);
        }
        self.append_extras(&mut base, parsed, &edition, &part, &self.options.template);
        base = cleanup_rendered(&base);
        base = self.fit_component(&base, &extension);

        let folder = if self.options.create_movie_folders {
            Some(PathBuf::from(base.clone()))
        } else {
            None
        };

        FormattedName {
            folder,
            file_name: format!("{base}.{extension}"),
            base,
        }
    }

    pub fn format_sidecar(&self, video: &FormattedName, sidecar: &SidecarInfo) -> FormattedName {
        let suffix = sidecar.suffix.trim();
        let base = format!("{}{}", video.base, suffix);
        let extension = safe_extension(&sidecar.extension);
        let reserve = extension.chars().count() + 1;
        let max_base = self.options.max_component_len.saturating_sub(reserve);
        let base = truncate_component(&base, max_base);
        FormattedName {
            folder: video.folder.clone(),
            file_name: format!("{base}.{extension}"),
            base,
        }
    }

    fn append_extras(
        &self,
        base: &mut String,
        parsed: &ParsedMedia,
        edition: &str,
        part: &str,
        template: &str,
    ) {
        if self.options.append_edition && !edition.is_empty() && !template.contains("{edition}") {
            base.push_str(" - ");
            base.push_str(&sanitize_component(edition));
        }
        if self.options.append_part && parsed.part.is_some() && !template.contains("{part}") {
            base.push_str(" - ");
            base.push_str(part);
        }
        if let Some(version) = &self.options.version_suffix {
            if !version.trim().is_empty() && !template.contains("{quality}") {
                base.push_str(" - ");
                base.push_str(&sanitize_component(version));
            }
        }
    }

    fn fit_component(&self, base: &str, extension: &str) -> String {
        let reserve = extension.chars().count() + 1;
        let max_base = self.options.max_component_len.saturating_sub(reserve);
        let mut fitted = truncate_component(base, max_base);
        if fitted.is_empty() {
            fitted = sanitize_component(&self.options.fallback_title);
        }
        fitted
    }

    fn render(&self, template: &str, values: &[(&str, &str)]) -> String {
        let mut rendered = template.to_string();
        for (key, value) in values {
            rendered = rendered.replace(&format!("{{{key}}}"), value);
        }
        rendered
    }
}

fn episode_placeholder(episodes: &[u16]) -> String {
    match episodes {
        [] => String::new(),
        [single] => format!("{single:02}"),
        [first, rest @ ..] => {
            let last = rest.last().copied().unwrap_or(*first);
            format!("{first:02}-E{last:02}")
        }
    }
}

fn cleanup_rendered(input: &str) -> String {
    let mut out = EMPTY_ID_RE.replace_all(input, " ").to_string();
    out = MULTI_SPACE_RE.replace_all(&out, " ").to_string();
    out = DANGLING_RE.replace_all(&out, " ").to_string();
    sanitize_component(&out)
}

fn safe_extension(extension: &str) -> String {
    let ext: String = extension
        .trim()
        .trim_start_matches('.')
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    if ext.is_empty() {
        "mkv".to_string()
    } else {
        ext
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{MediaKind, PartMarker};

    fn parsed() -> ParsedMedia {
        ParsedMedia {
            title: "The Matrix".into(),
            year: Some(1999),
            quality: Some("1080p".into()),
            ..Default::default()
        }
    }

    fn candidate() -> MatchCandidate {
        MatchCandidate::new("tmdb", "603", "The Matrix", Some(1999))
    }

    #[test]
    fn default_template_with_id() {
        let options = FormatOptions::default();
        let f = NameFormatter::new(&options);
        let result = f.format_video(&parsed(), Some(&candidate()), "mkv");
        assert_eq!(
            result
                .folder
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
            Some("The Matrix (1999) [tmdbid-603]".to_string())
        );
        assert_eq!(result.file_name, "The Matrix (1999) [tmdbid-603].mkv");
    }

    #[test]
    fn default_template_without_id_removes_empty_tag() {
        let options = FormatOptions::default();
        let f = NameFormatter::new(&options);
        let result = f.format_video(&parsed(), None, "mkv");
        assert_eq!(
            result
                .folder
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
            Some("The Matrix (1999)".to_string())
        );
        assert_eq!(result.file_name, "The Matrix (1999).mkv");
    }

    #[test]
    fn appends_edition_when_template_does_not_include_it() {
        let options = FormatOptions::default();
        let f = NameFormatter::new(&options);
        let mut p = parsed();
        p.edition = Some("Extended".into());
        let result = f.format_video(&p, Some(&candidate()), "mkv");
        assert_eq!(
            result.file_name,
            "The Matrix (1999) [tmdbid-603] - Extended.mkv"
        );
    }

    #[test]
    fn appends_part_marker() {
        let options = FormatOptions::default();
        let f = NameFormatter::new(&options);
        let mut p = parsed();
        p.part = Some(PartMarker {
            index: 1,
            label: "part".into(),
        });
        let result = f.format_video(&p, Some(&candidate()), "avi");
        assert_eq!(
            result.file_name,
            "The Matrix (1999) [tmdbid-603] - part1.avi"
        );
    }

    #[test]
    fn appends_version_suffix() {
        let options = FormatOptions {
            version_suffix: Some("1080p".into()),
            ..FormatOptions::default()
        };
        let f = NameFormatter::new(&options);
        let result = f.format_video(&parsed(), Some(&candidate()), "mkv");
        assert_eq!(
            result.file_name,
            "The Matrix (1999) [tmdbid-603] - 1080p.mkv"
        );
    }

    #[test]
    fn can_disable_movie_folders() {
        let options = FormatOptions {
            create_movie_folders: false,
            ..FormatOptions::default()
        };
        let f = NameFormatter::new(&options);
        let result = f.format_video(&parsed(), Some(&candidate()), "mkv");
        assert_eq!(result.folder, None);
    }

    #[test]
    fn sidecar_keeps_video_base_name() {
        let options = FormatOptions::default();
        let f = NameFormatter::new(&options);
        let video = f.format_video(&parsed(), Some(&candidate()), "mkv");
        let sidecar = SidecarInfo {
            kind: MediaKind::Subtitle,
            language: Some("en".into()),
            flags: vec![],
            suffix: ".en".into(),
            extension: "srt".into(),
        };
        let result = f.format_sidecar(&video, &sidecar);
        assert_eq!(result.file_name, "The Matrix (1999) [tmdbid-603].en.srt");
        assert_eq!(result.folder, video.folder);
    }

    #[test]
    fn sanitizes_illegal_title_characters() {
        let options = FormatOptions {
            template: "{title} ({year})".into(),
            ..FormatOptions::default()
        };
        let f = NameFormatter::new(&options);
        let mut p = parsed();
        p.title = "Avatar: The Way/Water".into();
        let result = f.format_video(&p, None, "mkv");
        assert_eq!(result.file_name, "Avatar - The Way - Water (1999).mkv");
    }

    #[test]
    fn truncates_long_titles() {
        let options = FormatOptions {
            max_component_len: 30,
            ..FormatOptions::default()
        };
        let f = NameFormatter::new(&options);
        let mut p = parsed();
        p.title = "A".repeat(200);
        let result = f.format_video(&p, None, "mkv");
        assert!(result.file_name.chars().count() <= 30);
    }

    #[test]
    fn missing_year_is_removed_cleanly() {
        let options = FormatOptions::default();
        let f = NameFormatter::new(&options);
        let mut p = parsed();
        p.year = None;
        let result = f.format_video(&p, None, "mkv");
        assert_eq!(result.file_name, "The Matrix.mkv");
    }

    #[test]
    fn custom_template_placeholders() {
        let options = FormatOptions {
            template: "{title} ({year}) [{quality}]".into(),
            ..FormatOptions::default()
        };
        let f = NameFormatter::new(&options);
        let result = f.format_video(&parsed(), None, "mkv");
        assert_eq!(result.file_name, "The Matrix (1999) [1080p].mkv");
    }

    fn tv_parsed() -> ParsedMedia {
        ParsedMedia {
            title: "The Office".into(),
            year: Some(2005),
            media_type: MediaType::Tv,
            season: Some(1),
            episodes: vec![2],
            episode_title: Some("The Dundies".into()),
            ..Default::default()
        }
    }

    fn tv_candidate() -> MatchCandidate {
        MatchCandidate::new_tv("tmdb", "2316", "The Office", Some(2005))
    }

    #[test]
    fn formats_tv_episode_path() {
        let options = FormatOptions::default();
        let f = NameFormatter::new(&options);
        let result = f.format_video(&tv_parsed(), Some(&tv_candidate()), "mkv");
        assert_eq!(
            result.folder,
            Some(PathBuf::from("The Office (2005) [tmdbid-2316]").join("Season 01"))
        );
        assert_eq!(
            result.file_name,
            "The Office (2005) S01E02 - The Dundies.mkv"
        );
    }

    #[test]
    fn formats_tv_multi_episode() {
        let options = FormatOptions::default();
        let f = NameFormatter::new(&options);
        let mut p = tv_parsed();
        p.episodes = vec![2, 3];
        let result = f.format_video(&p, Some(&tv_candidate()), "mkv");
        assert_eq!(
            result.file_name,
            "The Office (2005) S01E02-E03 - The Dundies.mkv"
        );
    }

    #[test]
    fn formats_tv_specials() {
        let options = FormatOptions::default();
        let f = NameFormatter::new(&options);
        let mut p = tv_parsed();
        p.season = Some(0);
        p.episodes = vec![1];
        p.episode_title = Some("Christmas Special".into());
        let result = f.format_video(&p, Some(&tv_candidate()), "mkv");
        assert_eq!(
            result.folder,
            Some(PathBuf::from("The Office (2005) [tmdbid-2316]").join("Season 00"))
        );
        assert_eq!(
            result.file_name,
            "The Office (2005) S00E01 - Christmas Special.mkv"
        );
    }

    #[test]
    fn tv_missing_episode_title_drops_separator() {
        let options = FormatOptions::default();
        let f = NameFormatter::new(&options);
        let mut p = tv_parsed();
        p.episode_title = None;
        let result = f.format_video(&p, Some(&tv_candidate()), "mkv");
        assert_eq!(result.file_name, "The Office (2005) S01E02.mkv");
    }

    #[test]
    fn can_disable_tv_folders() {
        let options = FormatOptions {
            create_tv_folders: false,
            ..FormatOptions::default()
        };
        let f = NameFormatter::new(&options);
        let result = f.format_video(&tv_parsed(), Some(&tv_candidate()), "mkv");
        assert_eq!(result.folder, None);
    }
}
