import type {
  CandidateChoice,
  Config,
  MatchCandidate,
  MediaType,
  ParsedMedia,
  PlanItem,
  RenamePlan,
} from "./types";

export const DEMO_ROOT = "/media/movies";

function parsed(overrides: Partial<ParsedMedia> = {}): ParsedMedia {
  return {
    title: "",
    year: null,
    media_type: "movie",
    season: null,
    episodes: [],
    episode_title: null,
    quality: null,
    source: null,
    codec: null,
    edition: null,
    part: null,
    release_group: null,
    confidence: 0.8,
    ...overrides,
  };
}

function candidate(
  id: string,
  title: string,
  year: number | null,
  popularity = 20,
  mediaType: MediaType = "movie",
): MatchCandidate {
  return {
    provider: "tmdb",
    id,
    title,
    year,
    media_type: mediaType,
    runtime_minutes: null,
    popularity,
    overview: null,
  };
}

function tvCandidate(
  id: string,
  title: string,
  year: number | null,
  popularity = 20,
): MatchCandidate {
  return candidate(id, title, year, popularity, "tv");
}

const MATRIX = candidate("603", "The Matrix", 1999, 42);
const INCEPTION = candidate("27205", "Inception", 2010, 39);
const AVATAR = candidate("19995", "Avatar", 2009, 51);
const AVATAR2 = candidate("76600", "Avatar: The Way of Water", 2022, 48);
const AVATAR_SERIES = candidate("114695", "Avatar", 2011, 12);
const DUNE = candidate("438631", "Dune", 2021, 44);
const DUNE_1984 = candidate("841", "Dune", 1984, 18);
const DUNE2 = candidate("693134", "Dune: Part Two", 2024, 41);
const PARASITE = candidate("496243", "Parasite", 2019, 35);
const HEAT = candidate("949", "Heat", 1995, 30);
const THE_OFFICE = tvCandidate("2316", "The Office", 2005, 40);
const BREAKING_BAD = tvCandidate("1396", "Breaking Bad", 2008, 38);

function item(
  id: string,
  source: string,
  target: string,
  status: PlanItem["status"],
  score: number | null,
  parsedMedia: ParsedMedia,
  match: MatchCandidate | null,
  reason: string | null = null,
): PlanItem {
  return {
    id,
    parent_id: null,
    source: `${DEMO_ROOT}/${source}`,
    target: target.startsWith("/") ? target : `${DEMO_ROOT}/${target}`,
    status,
    score,
    parsed: parsedMedia,
    candidate: match,
    sidecar: null,
    is_sidecar: false,
    case_only: false,
    reason,
  };
}

const ITEMS: PlanItem[] = [
  item(
    "v1",
    "The.Matrix.1999.1080p.BluRay.x264-YTS.mkv",
    "The Matrix (1999) [tmdbid-603]/The Matrix (1999) [tmdbid-603].mkv",
    "Ready",
    0.98,
    parsed({
      title: "The Matrix",
      year: 1999,
      quality: "1080p",
      source: "BluRay",
      codec: "x264",
      release_group: "YTS",
      confidence: 0.98,
    }),
    MATRIX,
  ),
  item(
    "v2",
    "inception_2010_720p_web-dl.mp4",
    "Inception (2010) [tmdbid-27205]/Inception (2010) [tmdbid-27205].mp4",
    "Ready",
    0.96,
    parsed({
      title: "Inception",
      year: 2010,
      quality: "720p",
      source: "WEB-DL",
      confidence: 0.94,
    }),
    INCEPTION,
  ),
  item(
    "v3",
    "Avatar.2009.EXTENDED.2160p.HDR.REMUX.mkv",
    "Avatar (2009) [tmdbid-19995] - Extended.mkv",
    "Review",
    0.74,
    parsed({
      title: "Avatar",
      year: 2009,
      quality: "2160p",
      source: "REMUX",
      edition: "Extended",
      confidence: 0.9,
    }),
    AVATAR,
  ),
  item(
    "v4",
    "Dune 2021 CD1.avi",
    "Dune (2021) [tmdbid-438631] - part1.avi",
    "Review",
    0.71,
    parsed({
      title: "Dune",
      year: 2021,
      part: { index: 1, label: "cd" },
      confidence: 0.72,
    }),
    DUNE,
  ),
  item(
    "v5",
    "Parasite.2019.KOREAN.1080p.BluRay.mkv",
    "Parasite (2019) [tmdbid-496243]/Parasite (2019) [tmdbid-496243].mkv",
    "Ready",
    0.97,
    parsed({
      title: "Parasite",
      year: 2019,
      quality: "1080p",
      source: "BluRay",
      confidence: 0.96,
    }),
    PARASITE,
  ),
  item(
    "v6",
    "Heat.1995.1080p.mkv",
    "Heat (1995) [tmdbid-949].mkv",
    "Conflict",
    0.95,
    parsed({ title: "Heat", year: 1995, quality: "1080p", confidence: 0.93 }),
    HEAT,
    "target already exists",
  ),
  item(
    "v7",
    "holiday_video_final_v2.mp4",
    "Holiday Video Final V2.mp4",
    "Unmatched",
    null,
    parsed({ title: "Holiday Video Final V2", confidence: 0.35 }),
    null,
    "no metadata candidates",
  ),
  item(
    "v8",
    "The.Office.US.S01E02.1080p.WEB-DL.mkv",
    "The Office (2005) [tmdbid-2316]/Season 01/The Office (2005) S01E02 - The Dundies.mkv",
    "Ready",
    0.97,
    parsed({
      title: "The Office",
      year: 2005,
      media_type: "tv",
      season: 1,
      episodes: [2],
      episode_title: "The Dundies",
      quality: "1080p",
      source: "WEB-DL",
      confidence: 0.96,
    }),
    THE_OFFICE,
  ),
  item(
    "v9",
    "Breaking.Bad.S02E05E06.720p.HDTV.mkv",
    "Breaking Bad (2008) [tmdbid-1396]/Season 02/Breaking Bad (2008) S02E05-E06 - Breakage.mkv",
    "Ready",
    0.95,
    parsed({
      title: "Breaking Bad",
      year: 2008,
      media_type: "tv",
      season: 2,
      episodes: [5, 6],
      episode_title: "Breakage",
      quality: "720p",
      source: "HDTV",
      confidence: 0.94,
    }),
    BREAKING_BAD,
  ),
  item(
    "v10",
    "the.office.s01.1080p.bluray.mkv",
    "The Office (2005) [tmdbid-2316]/The Office (2005) [tmdbid-2316].mkv",
    "Review",
    0.68,
    parsed({
      title: "The Office",
      year: 2005,
      media_type: "tv",
      season: 1,
      episodes: [],
      quality: "1080p",
      source: "BluRay",
      confidence: 0.61,
    }),
    THE_OFFICE,
    "no episode number detected",
  ),
];

export function demoPlan(): RenamePlan {
  return {
    root: DEMO_ROOT,
    items: ITEMS.map((entry) => ({ ...entry, parsed: { ...entry.parsed } })),
    created_at: Date.now(),
    summary: {
      total: 128,
      ready: 104,
      review: 17,
      unmatched: 7,
      conflicts: 2,
      skipped: 0,
      sidecars: 14,
    },
  };
}

const CHOICES: Record<string, CandidateChoice[]> = {
  v1: [
    { candidate: MATRIX, score: 0.98 },
    { candidate: candidate("604", "The Matrix Reloaded", 2003, 31), score: 0.44 },
    { candidate: candidate("605", "The Matrix Revolutions", 2003, 29), score: 0.4 },
  ],
  v2: [
    { candidate: INCEPTION, score: 0.96 },
    { candidate: candidate("27206", "Inception: The Cobol Job", 2010, 14), score: 0.48 },
    { candidate: candidate("1111", "Inception", 2014, 8), score: 0.3 },
  ],
  v3: [
    { candidate: AVATAR, score: 0.74 },
    { candidate: AVATAR2, score: 0.52 },
    { candidate: AVATAR_SERIES, score: 0.31 },
  ],
  v4: [
    { candidate: DUNE, score: 0.71 },
    { candidate: DUNE_1984, score: 0.58 },
    { candidate: DUNE2, score: 0.46 },
  ],
  v5: [
    { candidate: PARASITE, score: 0.97 },
    { candidate: candidate("2222", "The Parasite", 2004, 9), score: 0.39 },
    { candidate: candidate("3333", "Parasite", 2014, 7), score: 0.22 },
  ],
  v6: [
    { candidate: HEAT, score: 0.95 },
    { candidate: candidate("950", "Heat", 1986, 11), score: 0.51 },
    { candidate: candidate("951", "Heat", 2022, 13), score: 0.28 },
  ],
  v7: [],
  v8: [
    { candidate: THE_OFFICE, score: 0.97 },
    { candidate: candidate("2317", "The Office", 2001, 21), score: 0.41 },
    { candidate: candidate("2318", "Office", 2015, 8), score: 0.25 },
  ],
  v9: [
    { candidate: BREAKING_BAD, score: 0.95 },
    { candidate: candidate("1397", "Breaking Bad", 2019, 9), score: 0.32 },
  ],
  v10: [
    { candidate: THE_OFFICE, score: 0.68 },
    { candidate: candidate("2317", "The Office", 2001, 21), score: 0.46 },
  ],
};

export function demoChoices(itemId: string, item?: PlanItem): CandidateChoice[] {
  if (CHOICES[itemId]) {
    return CHOICES[itemId];
  }
  if (item?.candidate) {
    return [{ candidate: item.candidate, score: item.score ?? 0.5 }];
  }
  return [];
}

const ALL_CANDIDATES: MatchCandidate[] = [
  MATRIX,
  INCEPTION,
  AVATAR,
  AVATAR2,
  DUNE,
  DUNE_1984,
  DUNE2,
  PARASITE,
  HEAT,
  THE_OFFICE,
  BREAKING_BAD,
  candidate("11", "Star Wars", 1977, 55),
  candidate("12", "The Godfather", 1972, 53),
  candidate("13", "Pulp Fiction", 1994, 47),
];

export function demoSearch(query: string): MatchCandidate[] {
  const needle = query.trim().toLowerCase();
  if (!needle) {
    return [];
  }
  return ALL_CANDIDATES.filter((entry) =>
    entry.title.toLowerCase().includes(needle),
  );
}

export const DEMO_CONFIG: Config = {
  provider: "tmdb",
  tmdb_api_key: "",
  tmdb_read_token: "",
  omdb_api_key: "",
  language: "en-US",
  cache_ttl_days: 30,
  offline: false,
  auto_accept_threshold: 0.85,
  review_threshold: 0.55,
  concurrency: 8,
  requests_per_second: 10,
  template: "{title} ({year}) [tmdbid-{id}]",
  tv_template: "{title} ({year}) S{season}E{episode} - {episode_title}",
  series_folder_template: "{title} ({year}) [tmdbid-{id}]",
  create_movie_folders: true,
  create_tv_folders: true,
  move_sidecars: true,
  max_component_len: 180,
  ffprobe_path: null,
};
