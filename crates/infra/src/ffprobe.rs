use std::path::Path;

#[derive(Debug, Clone)]
pub struct Ffprobe {
    executable: String,
}

impl Ffprobe {
    pub fn new(executable: impl Into<String>) -> Self {
        Self {
            executable: executable.into(),
        }
    }

    /// Returns the media duration in minutes, rounded to the nearest minute.
    pub async fn duration_minutes(&self, path: &Path) -> Option<u32> {
        let output = tokio::process::Command::new(&self.executable)
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=noprint_wrappers=1:nokey=1",
            ])
            .arg(path)
            .output()
            .await
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        parse_duration(&text)
    }
}

fn parse_duration(text: &str) -> Option<u32> {
    let seconds: f64 = text.trim().parse().ok()?;
    if !seconds.is_finite() || seconds <= 0.0 {
        return None;
    }
    Some((seconds / 60.0).round() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ffprobe_duration() {
        assert_eq!(parse_duration("8160.123\n"), Some(136));
        assert_eq!(parse_duration("0"), None);
        assert_eq!(parse_duration("not a number"), None);
    }
}
