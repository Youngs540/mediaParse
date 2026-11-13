export type PlanStatus = "Ready" | "Review" | "Unmatched" | "Conflict" | "Skipped";

export type MediaType = "movie" | "tv";

export interface PartMarker {
  index: number;
  label: string;
}

export interface ParsedMedia {
  title: string;
  year: number | null;
  media_type: MediaType;
  season: number | null;
  episodes: number[];
  episode_title: string | null;
  quality: string | null;
  source: string | null;
  codec: string | null;
  edition: string | null;
  part: PartMarker | null;
  release_group: string | null;
  confidence: number;
}

export interface MatchCandidate {
  provider: string;
  id: string;
  title: string;
  year: number | null;
  media_type: MediaType;
  runtime_minutes: number | null;
  popularity: number | null;
  overview: string | null;
}

export interface SidecarInfo {
  kind: string;
  language: string | null;
  flags: string[];
  suffix: string;
  extension: string;
}

export interface PlanItem {
  id: string;
  parent_id: string | null;
  source: string;
  target: string;
  status: PlanStatus;
  score: number | null;
  parsed: ParsedMedia;
  candidate: MatchCandidate | null;
  sidecar: SidecarInfo | null;
  is_sidecar: boolean;
  case_only: boolean;
  reason: string | null;
}

export interface PlanSummary {
  total: number;
  ready: number;
  review: number;
  unmatched: number;
  conflicts: number;
  skipped: number;
  sidecars: number;
}

export interface RenamePlan {
  root: string;
  items: PlanItem[];
  created_at: number;
  summary: PlanSummary;
}

export interface PlanResponse {
  plan_id: string;
  plan: RenamePlan;
}

export type MetadataProviderKind = "tmdb" | "omdb";

export interface Config {
  provider: MetadataProviderKind;
  tmdb_api_key: string;
  tmdb_read_token: string;
  omdb_api_key: string;
  language: string;
  cache_ttl_days: number;
  offline: boolean;
  auto_accept_threshold: number;
  review_threshold: number;
  concurrency: number;
  requests_per_second: number;
  template: string;
  tv_template: string;
  series_folder_template: string;
  create_movie_folders: boolean;
  create_tv_folders: boolean;
  move_sidecars: boolean;
  max_component_len: number;
  ffprobe_path: string | null;
}

export interface BuildOptions {
  template?: string | null;
  tv_template?: string | null;
  series_folder_template?: string | null;
  create_movie_folders?: boolean | null;
  create_tv_folders?: boolean | null;
  move_sidecars?: boolean | null;
  offline?: boolean | null;
  concurrency?: number | null;
  max_component_len?: number | null;
}

export interface ScanSummary {
  root: string;
  files: number;
  videos: number;
  sidecars: number;
  groups: number;
  duration_ms: number;
  errors: string[];
}

export interface PlanProgress {
  phase: string;
  done: number;
  total: number;
  current: string;
}

export interface ApplyReport {
  run_id: number;
  renamed: number;
  failed: number;
  skipped: number;
  errors: string[];
}

export interface UndoReport {
  run_id: number;
  restored: number;
  failed: number;
  errors: string[];
}

export interface RunSummary {
  id: number;
  root: string;
  started_at: number;
  finished_at: number | null;
  status: string;
  total: number;
  renamed: number;
  failed: number;
}

export interface CandidateChoice {
  candidate: MatchCandidate;
  score: number | null;
}

export type FilterKey = "all" | "matched" | "review" | "unmatched" | "conflicts";

export interface ScanEvent {
  type: "found" | "progress" | "error";
  path?: string;
  kind?: string;
  files?: number;
  current?: string;
  message?: string;
}
