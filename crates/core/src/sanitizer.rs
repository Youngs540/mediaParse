use std::sync::LazyLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

const ILLEGAL: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

static EMPTY_BRACKETS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?x)\(\s*\)|\[\s*\]|\{\s*\}").expect("valid empty-bracket regex")
});
static MULTI_SPACE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[ \t]{2,}").expect("valid whitespace regex"));

pub fn normalize_nfc(input: &str) -> String {
    input.nfc().collect()
}

/// True for Windows device names that cannot be used as a file name.
pub fn is_windows_reserved(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or(name)
        .trim()
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0')
}

fn replacement_for(ch: char) -> &'static str {
    match ch {
        ':' | '/' | '\\' => " - ",
        '<' | '>' | '"' | '|' | '?' | '*' => " ",
        _ => " ",
    }
}

/// Make a string safe as a single path component on Linux, macOS and Windows.
///
/// * Normalizes to Unicode NFC.
/// * Replaces `<>:"/\|?*` and control characters.
/// * Collapses whitespace.
/// * Removes trailing dots and spaces (Windows rejects them).
/// * Escapes Windows reserved device names.
/// * Returns `Untitled` for an empty result.
pub fn sanitize_component(input: &str) -> String {
    let normalized = normalize_nfc(input);
    let mut out = String::with_capacity(normalized.len());
    for ch in normalized.chars() {
        if ch.is_control() || ILLEGAL.contains(&ch) {
            out.push_str(replacement_for(ch));
        } else {
            out.push(ch);
        }
    }

    let out = MULTI_SPACE.replace_all(&out, " ");
    let out = EMPTY_BRACKETS.replace_all(&out, " ");
    let mut cleaned = out
        .trim()
        .trim_matches(|c: char| c == '.' || c == ' ' || c == '-' || c == '_')
        .trim()
        .to_string();

    // Collapse spaces around the replacement hyphen.
    while cleaned.contains(" -  - ") {
        cleaned = cleaned.replace(" -  - ", " - ");
    }
    cleaned = cleaned.replace(" -  ", " - ").replace("  - ", " - ");
    cleaned = MULTI_SPACE.replace_all(&cleaned, " ").trim().to_string();

    if cleaned.is_empty() {
        return "Untitled".to_string();
    }
    if is_windows_reserved(&cleaned) {
        cleaned.push('_');
    }
    cleaned
}

/// Truncate on a character boundary, never leaving a trailing dot/space.
pub fn truncate_component(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let mut out: String = if input.chars().count() <= max_chars {
        input.to_string()
    } else {
        input.chars().take(max_chars).collect()
    };
    while out.ends_with('.') || out.ends_with(' ') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_illegal_characters() {
        assert_eq!(
            sanitize_component("Avatar: The Way/Water?"),
            "Avatar - The Way - Water"
        );
        assert_eq!(sanitize_component("A<B>C\"D|E*F"), "A B C D E F");
    }

    #[test]
    fn trims_trailing_dots_and_spaces() {
        assert_eq!(sanitize_component("Movie Name. . "), "Movie Name");
    }

    #[test]
    fn escapes_windows_reserved_names() {
        assert_eq!(sanitize_component("CON"), "CON_");
        assert_eq!(sanitize_component("com1.txt"), "com1.txt_");
        assert_eq!(sanitize_component("console"), "console");
    }

    #[test]
    fn handles_empty_input() {
        assert_eq!(sanitize_component(""), "Untitled");
        assert_eq!(sanitize_component("..."), "Untitled");
    }

    #[test]
    fn normalizes_nfc() {
        let decomposed = "Ame\u{0301}lie";
        let result = sanitize_component(decomposed);
        assert_eq!(result, "Am\u{00e9}lie");
    }

    #[test]
    fn truncates_on_char_boundary() {
        assert_eq!(truncate_component("abcdef", 3), "abc");
        assert_eq!(truncate_component("ab.", 3), "ab");
        assert_eq!(truncate_component("éééé", 2), "éé");
    }

    #[test]
    fn removes_empty_brackets() {
        assert_eq!(sanitize_component("Movie () []"), "Movie");
    }
}
