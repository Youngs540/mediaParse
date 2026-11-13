import type { MatchCandidate, ParsedMedia, PlanStatus } from "./types";

export function normalizePath(path: string): string {
  return path.replace(/\\/g, "/");
}

export function basename(path: string): string {
  const normalized = normalizePath(path).replace(/\/+$/, "");
  const index = normalized.lastIndexOf("/");
  return index >= 0 ? normalized.slice(index + 1) : normalized;
}

export function relativeTo(root: string, path: string): string {
  const normalizedRoot = normalizePath(root).replace(/\/+$/, "");
  const normalizedPath = normalizePath(path);
  if (normalizedPath.startsWith(`${normalizedRoot}/`)) {
    return normalizedPath.slice(normalizedRoot.length + 1);
  }
  return normalizedPath;
}

export function parsedSummary(parsed: ParsedMedia): string {
  const parts: string[] = [];
  if (parsed.title) {
    parts.push(parsed.title);
  }
  if (parsed.year) {
    parts.push(String(parsed.year));
  }
  if (parsed.media_type === "tv") {
    const code = episodeCode(parsed.season, parsed.episodes);
    if (code) {
      parts.push(code);
    }
    if (parsed.episode_title) {
      parts.push(parsed.episode_title);
    }
  }
  if (parsed.quality) {
    parts.push(parsed.quality);
  }
  if (parsed.edition) {
    parts.push(parsed.edition);
  }
  if (parsed.part) {
    parts.push(`part ${parsed.part.index}`);
  }
  return parts.length > 0 ? parts.join(" · ") : "Nothing detected";
}

export function episodeCode(
  season: number | null,
  episodes: number[],
): string {
  if (season == null || episodes.length === 0) {
    return "";
  }
  const pad = (value: number) => String(value).padStart(2, "0");
  const first = episodes[0];
  const last = episodes[episodes.length - 1];
  const episode =
    episodes.length > 1 ? `${pad(first)}-E${pad(last)}` : pad(first);
  return `S${pad(season)}E${episode}`;
}

export function statusLabel(status: PlanStatus): string {
  switch (status) {
    case "Ready":
      return "Matched";
    case "Review":
      return "Review";
    case "Unmatched":
      return "Unmatched";
    case "Conflict":
      return "Conflict";
    case "Skipped":
      return "Skipped";
  }
}

export function statusClass(status: PlanStatus): string {
  switch (status) {
    case "Ready":
      return "ok";
    case "Review":
      return "rev";
    case "Unmatched":
    case "Conflict":
      return "bad";
    case "Skipped":
      return "skip";
  }
}

export function scoreLabel(score: number | null): string {
  if (score == null || !Number.isFinite(score)) {
    return "–";
  }
  return `${Math.round(score * 100)}%`;
}

export function candidateLabel(candidate: MatchCandidate): string {
  return candidate.year
    ? `${candidate.title} (${candidate.year})`
    : candidate.title;
}

export function candidateKey(candidate: MatchCandidate): string {
  return `${candidate.provider}:${candidate.id}`;
}
