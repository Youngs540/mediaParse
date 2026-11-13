use std::path::Path;

use mediaparse_core::{MediaKind, SidecarInfo};

/// Build the sidecar suffix/metadata from the original file name.
pub fn parse_sidecar(video_stem: &str, sidecar_file_name: &str, kind: MediaKind) -> SidecarInfo {
    let path = Path::new(sidecar_file_name);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let extension = path
        .extension()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let lower_stem = stem.to_lowercase();
    let video_lower = video_stem.to_lowercase();
    let remainder = if lower_stem.starts_with(&video_lower) {
        stem[video_lower.len()..].to_string()
    } else {
        String::new()
    };
    let tokens: Vec<String> = remainder
        .split(['.', '-', '_'])
        .filter(|token| !token.is_empty())
        .map(|token| token.to_string())
        .collect();

    match kind {
        MediaKind::Subtitle => {
            let language = tokens
                .iter()
                .find(|token| is_language(token))
                .map(|token| normalize_language(token));
            let flags = tokens
                .iter()
                .filter(|token| is_flag(token))
                .map(|token| token.to_ascii_lowercase())
                .collect::<Vec<_>>();
            let suffix = if tokens.is_empty() {
                String::new()
            } else {
                format!(".{}", tokens.join("."))
            };
            SidecarInfo {
                kind,
                language,
                flags,
                suffix,
                extension,
            }
        }
        MediaKind::Nfo => SidecarInfo {
            kind,
            language: None,
            flags: vec![],
            suffix: String::new(),
            extension,
        },
        MediaKind::Image => {
            let suffix = image_suffix(&lower_stem, &tokens);
            SidecarInfo {
                kind,
                language: None,
                flags: vec![],
                suffix,
                extension,
            }
        }
        MediaKind::Other | MediaKind::Movie => SidecarInfo {
            kind,
            language: None,
            flags: vec![],
            suffix: String::new(),
            extension,
        },
    }
}

fn image_suffix(lower_stem: &str, tokens: &[String]) -> String {
    for (needle, suffix) in [
        ("poster", "-poster"),
        ("folder", "-poster"),
        ("fanart", "-fanart"),
        ("backdrop", "-fanart"),
        ("banner", "-banner"),
        ("logo", "-logo"),
        ("landscape", "-landscape"),
        ("thumb", "-thumb"),
    ] {
        if lower_stem.contains(needle) || tokens.iter().any(|t| t.eq_ignore_ascii_case(needle)) {
            return suffix.to_string();
        }
    }
    if tokens.is_empty() {
        String::new()
    } else {
        format!("-{}", tokens.join("-"))
    }
}

fn is_language(token: &str) -> bool {
    matches!(
        token.to_ascii_lowercase().as_str(),
        "en" | "eng"
            | "english"
            | "fr"
            | "fre"
            | "fra"
            | "french"
            | "de"
            | "ger"
            | "deu"
            | "german"
            | "es"
            | "spa"
            | "spanish"
            | "it"
            | "ita"
            | "italian"
            | "pt"
            | "por"
            | "portuguese"
            | "ru"
            | "rus"
            | "russian"
            | "ja"
            | "jpn"
            | "japanese"
            | "ko"
            | "kor"
            | "korean"
            | "zh"
            | "chi"
            | "zho"
            | "chinese"
            | "hi"
            | "hin"
            | "hindi"
            | "ta"
            | "tam"
            | "tamil"
            | "te"
            | "tel"
            | "telugu"
            | "nl"
            | "dut"
            | "nld"
            | "dutch"
            | "pl"
            | "pol"
            | "polish"
            | "sv"
            | "swe"
            | "swedish"
            | "no"
            | "nor"
            | "norwegian"
            | "da"
            | "dan"
            | "danish"
            | "fi"
            | "fin"
            | "finnish"
            | "tr"
            | "tur"
            | "turkish"
            | "ar"
            | "ara"
            | "arabic"
            | "he"
            | "heb"
            | "hebrew"
    )
}

fn normalize_language(token: &str) -> String {
    match token.to_ascii_lowercase().as_str() {
        "eng" | "english" => "en",
        "fre" | "fra" | "french" => "fr",
        "ger" | "deu" | "german" => "de",
        "spa" | "spanish" => "es",
        "ita" | "italian" => "it",
        "por" | "portuguese" => "pt",
        "rus" | "russian" => "ru",
        "jpn" | "japanese" => "ja",
        "kor" | "korean" => "ko",
        "chi" | "zho" | "chinese" => "zh",
        "hin" | "hindi" => "hi",
        "tam" | "tamil" => "ta",
        "tel" | "telugu" => "te",
        "dut" | "nld" | "dutch" => "nl",
        "pol" | "polish" => "pl",
        "swe" | "swedish" => "sv",
        "nor" | "norwegian" => "no",
        "dan" | "danish" => "da",
        "fin" | "finnish" => "fi",
        "tur" | "turkish" => "tr",
        "ara" | "arabic" => "ar",
        "heb" | "hebrew" => "he",
        other => other,
    }
    .to_string()
}

fn is_flag(token: &str) -> bool {
    matches!(
        token.to_ascii_lowercase().as_str(),
        "forced" | "sdh" | "hi" | "cc" | "default" | "foreign" | "commentary"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_subtitle_language() {
        let info = parse_sidecar("movie.2020", "movie.2020.en.srt", MediaKind::Subtitle);
        assert_eq!(info.language.as_deref(), Some("en"));
        assert_eq!(info.suffix, ".en");
    }

    #[test]
    fn parses_flags() {
        let info = parse_sidecar(
            "movie.2020",
            "movie.2020.en.forced.srt",
            MediaKind::Subtitle,
        );
        assert_eq!(info.language.as_deref(), Some("en"));
        assert_eq!(info.flags, vec!["forced"]);
        assert_eq!(info.suffix, ".en.forced");
    }

    #[test]
    fn nfo_keeps_base_name() {
        let info = parse_sidecar("movie.2020", "movie.2020.nfo", MediaKind::Nfo);
        assert_eq!(info.suffix, "");
    }

    #[test]
    fn image_gets_jellyfin_suffix() {
        let info = parse_sidecar("movie.2020", "movie.2020-poster.jpg", MediaKind::Image);
        assert_eq!(info.suffix, "-poster");
    }

    #[test]
    fn generic_poster_is_detected() {
        let info = parse_sidecar("movie.2020", "poster.jpg", MediaKind::Image);
        assert_eq!(info.suffix, "-poster");
    }

    #[test]
    fn normalizes_iso639_2_codes() {
        let info = parse_sidecar("movie.2020", "movie.2020.eng.srt", MediaKind::Subtitle);
        assert_eq!(info.language.as_deref(), Some("en"));
    }
}
