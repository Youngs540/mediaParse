use mediaparse_core::{
    parse_filename, sanitize_component, FormatOptions, MatchCandidate, MatchThresholds,
    NameFormatter, ParsedMedia,
};

struct Fixture {
    input: &'static str,
    title: &'static str,
    year: Option<u16>,
    quality: Option<&'static str>,
    edition: Option<&'static str>,
    part: Option<u32>,
}

const FIXTURES: &[Fixture] = &[
    Fixture {
        input: "The.Matrix.1999.1080p.BluRay.x264-YTS.mkv",
        title: "The Matrix",
        year: Some(1999),
        quality: Some("1080p"),
        edition: None,
        part: None,
    },
    Fixture {
        input: "inception_2010_720p_web-dl.mp4",
        title: "Inception",
        year: Some(2010),
        quality: Some("720p"),
        edition: None,
        part: None,
    },
    Fixture {
        input: "Avatar.2009.EXTENDED.2160p.HDR.REMUX.mkv",
        title: "Avatar",
        year: Some(2009),
        quality: Some("2160p"),
        edition: Some("Extended"),
        part: None,
    },
    Fixture {
        input: "Dune 2021 CD1.avi",
        title: "Dune",
        year: Some(2021),
        quality: None,
        edition: None,
        part: Some(1),
    },
    Fixture {
        input: "Parasite.2019.KOREAN.1080p.BluRay.mkv",
        title: "Parasite",
        year: Some(2019),
        quality: Some("1080p"),
        edition: None,
        part: None,
    },
    Fixture {
        input: "holiday_video_final_v2.mp4",
        title: "Holiday Video Final V2",
        year: None,
        quality: None,
        edition: None,
        part: None,
    },
    Fixture {
        input: "Blade.Runner.2049.2017.1080p.BluRay.x264.mkv",
        title: "Blade Runner 2049",
        year: Some(2017),
        quality: Some("1080p"),
        edition: None,
        part: None,
    },
    Fixture {
        input: "1917.2019.2160p.WEB-DL.mkv",
        title: "1917",
        year: Some(2019),
        quality: Some("2160p"),
        edition: None,
        part: None,
    },
    Fixture {
        input:
            "The.Lord.of.the.Rings.The.Fellowship.of.the.Ring.2001.EXTENDED.1080p.BluRay.x265.mkv",
        title: "The Lord of the Rings The Fellowship of the Ring",
        year: Some(2001),
        quality: Some("1080p"),
        edition: Some("Extended"),
        part: None,
    },
    Fixture {
        input: "Spider-Man.No.Way.Home.2021.2160p.WEB-DL.DDP5.1.Atmos.HDR.mkv",
        title: "Spider-Man No Way Home",
        year: Some(2021),
        quality: Some("2160p"),
        edition: None,
        part: None,
    },
    Fixture {
        input: "A.Movie.2020.Part.2.1080p.mkv",
        title: "A Movie",
        year: Some(2020),
        quality: Some("1080p"),
        edition: None,
        part: Some(2),
    },
    Fixture {
        input: "Some.Movie.2021.Directors.Cut.REMASTERED.1080p.mkv",
        title: "Some Movie",
        year: Some(2021),
        quality: Some("1080p"),
        edition: Some("Director's Cut"),
        part: None,
    },
    Fixture {
        input: "The.Matrix.1999.1080p.BluRay.x264-YTS.en.srt",
        title: "The Matrix",
        year: Some(1999),
        quality: Some("1080p"),
        edition: None,
        part: None,
    },
];

#[test]
fn parses_real_world_fixture_set() {
    for fixture in FIXTURES {
        let parsed = parse_filename(fixture.input, None);
        assert_eq!(parsed.title, fixture.title, "title for {}", fixture.input);
        assert_eq!(parsed.year, fixture.year, "year for {}", fixture.input);
        assert_eq!(
            parsed.quality.as_deref(),
            fixture.quality,
            "quality for {}",
            fixture.input
        );
        assert_eq!(
            parsed.edition.as_deref(),
            fixture.edition,
            "edition for {}",
            fixture.input
        );
        assert_eq!(
            parsed.part.as_ref().map(|p| p.index),
            fixture.part,
            "part for {}",
            fixture.input
        );
    }
}

#[test]
fn parent_folder_supplies_missing_year() {
    let parsed = parse_filename(
        "The.Matrix.1080p.BluRay.x264-YTS.mkv",
        Some("The Matrix (1999)"),
    );
    assert_eq!(parsed.title, "The Matrix");
    assert_eq!(parsed.year, Some(1999));
}

#[test]
fn formatter_matches_jellyfin_shape() {
    let parsed = ParsedMedia {
        title: "The Matrix".into(),
        year: Some(1999),
        quality: Some("1080p".into()),
        ..Default::default()
    };
    let candidate = MatchCandidate::new("tmdb", "603", "The Matrix", Some(1999));
    let options = FormatOptions::default();
    let formatted = NameFormatter::new(&options).format_video(&parsed, Some(&candidate), "mkv");
    assert_eq!(formatted.file_name, "The Matrix (1999) [tmdbid-603].mkv");
    assert_eq!(
        formatted
            .folder
            .as_ref()
            .map(|path| path.to_string_lossy().to_string()),
        Some("The Matrix (1999) [tmdbid-603]".to_string())
    );
}

#[test]
fn sanitizer_is_windows_safe() {
    let name = sanitize_component("Avatar: The Way of Water?");
    assert!(!name.contains(':'));
    assert!(!name.contains('?'));
    assert_eq!(name, "Avatar - The Way of Water");
}

#[test]
fn matcher_prefers_exact_title_and_year() {
    let parsed = ParsedMedia {
        title: "Avatar".into(),
        year: Some(2009),
        ..Default::default()
    };
    let candidates = vec![
        MatchCandidate::new("tmdb", "19995", "Avatar", Some(2009)),
        MatchCandidate::new("tmdb", "76600", "Avatar: The Way of Water", Some(2022)),
    ];
    let result =
        mediaparse_core::best_match(&parsed, &candidates, None, MatchThresholds::default());
    assert_eq!(result.candidate.unwrap().id, "19995");
    assert_eq!(result.decision, mediaparse_core::MatchDecision::AutoAccept);
}
