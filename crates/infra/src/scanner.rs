use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ignore::{WalkBuilder, WalkState};
use serde::{Deserialize, Serialize};

use mediaparse_core::{MediaKind, SidecarInfo};

use crate::sidecar::parse_sidecar;

pub const VIDEO_EXTENSIONS: &[&str] = &["mkv", "mp4", "avi", "m4v", "ts", "mov"];
pub const SUBTITLE_EXTENSIONS: &[&str] = &["srt", "ass", "ssa", "vtt", "sub"];
pub const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannedFile {
    pub path: PathBuf,
    pub file_name: String,
    pub size: u64,
    pub kind: MediaKind,
    /// Populated for sidecars once they are grouped with a video.
    pub sidecar: Option<SidecarInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannedGroup {
    pub video: ScannedFile,
    pub sidecars: Vec<ScannedFile>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScanStats {
    pub files: usize,
    pub videos: usize,
    pub sidecars: usize,
    pub groups: usize,
    pub duration_ms: u64,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub follow_links: bool,
    pub threads: usize,
    pub max_depth: Option<usize>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            follow_links: false,
            threads: std::thread::available_parallelism()
                .map(|n| n.get().clamp(2, 16))
                .unwrap_or(4),
            max_depth: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScanEvent {
    Found { path: PathBuf, kind: MediaKind },
    Progress { files: usize, current: PathBuf },
    Error { message: String },
}

#[derive(Debug, Clone)]
pub struct ScanOutput {
    pub groups: Vec<ScannedGroup>,
    pub stats: ScanStats,
    pub existing_files: Vec<PathBuf>,
    pub existing_dirs: Vec<PathBuf>,
}

struct RawScan {
    files: Vec<ScannedFile>,
    existing_files: Vec<PathBuf>,
    existing_dirs: Vec<PathBuf>,
    errors: Vec<String>,
}

/// Walk the library in parallel and group sidecars with their videos.
pub fn scan_library(
    root: &Path,
    options: &ScanOptions,
    event: impl Fn(ScanEvent) + Send + Sync + 'static,
) -> Result<ScanOutput, ScanError> {
    let started = Instant::now();
    let raw = walk(root, options, event)?;
    let mut stats = ScanStats {
        errors: raw.errors,
        ..Default::default()
    };
    let groups = group_files(raw.files, &mut stats);
    stats.groups = groups.len();
    stats.duration_ms = started.elapsed().as_millis() as u64;
    Ok(ScanOutput {
        groups,
        stats,
        existing_files: raw.existing_files,
        existing_dirs: raw.existing_dirs,
    })
}

fn walk(
    root: &Path,
    options: &ScanOptions,
    event: impl Fn(ScanEvent) + Send + Sync + 'static,
) -> Result<RawScan, ScanError> {
    if !root.exists() {
        return Err(ScanError::NotFound(root.to_path_buf()));
    }
    if !root.is_dir() {
        return Err(ScanError::NotDirectory(root.to_path_buf()));
    }

    let files: Arc<Mutex<Vec<ScannedFile>>> = Arc::new(Mutex::new(Vec::new()));
    let existing_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
    let existing_dirs: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let event = Arc::new(event);
    let counter = Arc::new(AtomicUsize::new(0));
    let last_emit = Arc::new(Mutex::new(Instant::now() - Duration::from_secs(10)));

    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .ignore(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .parents(false)
        .follow_links(options.follow_links)
        .threads(options.threads)
        .max_depth(options.max_depth)
        .filter_entry(|entry| entry.file_name() != ".git");

    let walker = builder.build_parallel();
    walker.run(|| {
        let files = Arc::clone(&files);
        let existing_files = Arc::clone(&existing_files);
        let existing_dirs = Arc::clone(&existing_dirs);
        let errors = Arc::clone(&errors);
        let event = Arc::clone(&event);
        let counter = Arc::clone(&counter);
        let last_emit = Arc::clone(&last_emit);

        Box::new(move |entry| {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    let message = err.to_string();
                    if let Ok(mut errors) = errors.lock() {
                        errors.push(message.clone());
                    }
                    event(ScanEvent::Error { message });
                    return WalkState::Continue;
                }
            };

            let path = entry.path().to_path_buf();
            let file_type = entry.file_type();
            if file_type.map(|t| t.is_dir()).unwrap_or(false) {
                if path != root {
                    if let Ok(mut dirs) = existing_dirs.lock() {
                        dirs.push(path);
                    }
                }
                return WalkState::Continue;
            }
            if !file_type.map(|t| t.is_file()).unwrap_or(false) {
                return WalkState::Continue;
            }

            if let Ok(mut paths) = existing_files.lock() {
                paths.push(path.clone());
            }

            let Some(extension) = extension_of(&path) else {
                return WalkState::Continue;
            };
            let kind = classify_extension(&extension);
            if kind == MediaKind::Other {
                return WalkState::Continue;
            }

            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            let file_name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            let scanned = ScannedFile {
                path: path.clone(),
                file_name,
                size,
                kind,
                sidecar: None,
            };
            if let Ok(mut files) = files.lock() {
                files.push(scanned);
            }
            event(ScanEvent::Found {
                path: path.clone(),
                kind,
            });

            let count = counter.fetch_add(1, Ordering::Relaxed) + 1;
            let should_emit = count.is_multiple_of(64)
                || last_emit
                    .lock()
                    .map(|mut last| {
                        if last.elapsed() >= Duration::from_millis(120) {
                            *last = Instant::now();
                            true
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false);
            if should_emit {
                event(ScanEvent::Progress {
                    files: count,
                    current: path,
                });
            }
            WalkState::Continue
        })
    });

    let files = files.lock().map(|f| f.clone()).unwrap_or_default();
    let existing_files = existing_files.lock().map(|f| f.clone()).unwrap_or_default();
    let existing_dirs = existing_dirs.lock().map(|f| f.clone()).unwrap_or_default();
    let errors = errors.lock().map(|e| e.clone()).unwrap_or_default();
    Ok(RawScan {
        files,
        existing_files,
        existing_dirs,
        errors,
    })
}

/// Group sidecars with the video whose stem they extend.
pub fn group_files(files: Vec<ScannedFile>, stats: &mut ScanStats) -> Vec<ScannedGroup> {
    let mut videos: Vec<ScannedFile> = Vec::new();
    let mut sidecars: Vec<ScannedFile> = Vec::new();
    for file in files {
        match file.kind {
            MediaKind::Movie => videos.push(file),
            MediaKind::Subtitle | MediaKind::Nfo | MediaKind::Image => sidecars.push(file),
            MediaKind::Other => {}
        }
    }

    let mut video_by_stem: HashMap<(PathBuf, String), usize> = HashMap::new();
    let mut videos_by_dir: HashMap<PathBuf, Vec<usize>> = HashMap::new();
    for (index, video) in videos.iter().enumerate() {
        let parent = video
            .path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let stem = file_stem_lower(&video.path);
        video_by_stem.entry((parent.clone(), stem)).or_insert(index);
        videos_by_dir.entry(parent).or_default().push(index);
    }

    let mut grouped: Vec<Vec<ScannedFile>> = vec![Vec::new(); videos.len()];
    for sidecar in sidecars {
        let parent = sidecar
            .path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let stem = file_stem_lower(&sidecar.path);
        let owner = find_owner(
            &video_by_stem,
            &videos_by_dir,
            &parent,
            &stem,
            &sidecar.file_name,
        );
        if let Some(index) = owner {
            let video_stem = file_stem_lower(&videos[index].path);
            let mut sidecar = sidecar;
            sidecar.sidecar = Some(parse_sidecar(&video_stem, &sidecar.file_name, sidecar.kind));
            grouped[index].push(sidecar);
        }
    }

    let mut groups = Vec::with_capacity(videos.len());
    for (video, mut sidecar_files) in videos.into_iter().zip(grouped) {
        sidecar_files.sort_by(|a, b| a.path.cmp(&b.path));
        groups.push(ScannedGroup {
            video,
            sidecars: sidecar_files,
        });
    }
    groups.sort_by(|a, b| a.video.path.cmp(&b.video.path));

    stats.videos = groups.len();
    stats.sidecars = groups.iter().map(|g| g.sidecars.len()).sum();
    stats.files = stats.videos + stats.sidecars;
    groups
}

fn find_owner(
    video_by_stem: &HashMap<(PathBuf, String), usize>,
    videos_by_dir: &HashMap<PathBuf, Vec<usize>>,
    parent: &Path,
    sidecar_stem: &str,
    sidecar_name: &str,
) -> Option<usize> {
    let mut candidate = sidecar_stem.to_string();
    loop {
        if let Some(index) = video_by_stem.get(&(parent.to_path_buf(), candidate.clone())) {
            return Some(*index);
        }
        let Some(position) = candidate.rfind(['.', '-', '_']) else {
            break;
        };
        if position == 0 {
            break;
        }
        candidate.truncate(position);
    }

    // Folder-level artwork: poster.jpg, fanart.jpg, folder.jpg, ...
    let lower = sidecar_name.to_lowercase();
    let generic = [
        "poster",
        "fanart",
        "folder",
        "backdrop",
        "logo",
        "banner",
        "thumb",
        "landscape",
    ]
    .iter()
    .any(|name| lower.starts_with(name));
    if generic {
        if let Some(indices) = videos_by_dir.get(parent) {
            if indices.len() == 1 {
                return Some(indices[0]);
            }
        }
    }
    None
}

pub fn extension_of(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
}

pub fn classify_extension(extension: &str) -> MediaKind {
    if VIDEO_EXTENSIONS.contains(&extension) {
        MediaKind::Movie
    } else if SUBTITLE_EXTENSIONS.contains(&extension) {
        MediaKind::Subtitle
    } else if extension == "nfo" {
        MediaKind::Nfo
    } else if IMAGE_EXTENSIONS.contains(&extension) {
        MediaKind::Image
    } else {
        MediaKind::Other
    }
}

fn file_stem_lower(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("path does not exist: {0}")]
    NotFound(PathBuf),
    #[error("path is not a directory: {0}")]
    NotDirectory(PathBuf),
    #[error("scan error: {0}")]
    Walk(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(dir: &Path, name: &str) {
        fs::write(dir.join(name), b"x").unwrap();
    }

    #[test]
    fn scans_and_groups_sidecars() {
        let dir = tempfile::tempdir().unwrap();
        let movies = dir.path().join("movies");
        fs::create_dir_all(&movies).unwrap();
        write(&movies, "The.Matrix.1999.1080p.BluRay.x264-YTS.mkv");
        write(&movies, "The.Matrix.1999.1080p.BluRay.x264-YTS.en.srt");
        write(&movies, "The.Matrix.1999.1080p.BluRay.x264-YTS.nfo");
        write(&movies, "poster.jpg");
        write(&movies, "readme.txt");

        let output = scan_library(&movies, &ScanOptions::default(), |_| {}).unwrap();
        assert_eq!(output.groups.len(), 1);
        let group = &output.groups[0];
        assert_eq!(group.video.kind, MediaKind::Movie);
        assert_eq!(group.sidecars.len(), 3);
        assert_eq!(output.stats.videos, 1);
        assert_eq!(output.stats.sidecars, 3);
        assert!(output.existing_files.len() >= 5);
    }

    #[test]
    fn groups_language_and_flags() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "Movie.2020.mkv");
        write(dir.path(), "Movie.2020.en.forced.srt");
        let output = scan_library(dir.path(), &ScanOptions::default(), |_| {}).unwrap();
        let sidecar = &output.groups[0].sidecars[0];
        let info = sidecar.sidecar.as_ref().unwrap();
        assert_eq!(info.language.as_deref(), Some("en"));
        assert!(info.flags.contains(&"forced".to_string()));
        assert_eq!(info.suffix, ".en.forced");
    }

    #[test]
    fn handles_nested_folders() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("A").join("B");
        fs::create_dir_all(&nested).unwrap();
        write(&nested, "Movie.2020.mp4");
        let output = scan_library(dir.path(), &ScanOptions::default(), |_| {}).unwrap();
        assert_eq!(output.groups.len(), 1);
        assert!(output.existing_dirs.len() >= 2);
    }

    #[test]
    fn extension_filter_is_case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "Movie.MKV");
        let output = scan_library(dir.path(), &ScanOptions::default(), |_| {}).unwrap();
        assert_eq!(output.groups.len(), 1);
    }

    #[test]
    fn missing_root_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let result = scan_library(&dir.path().join("nope"), &ScanOptions::default(), |_| {});
        assert!(result.is_err());
    }
}
