export const MOVIE_PLACEHOLDERS: string[] = [
  "{title}",
  "{year}",
  "{id}",
  "{provider}",
  "{quality}",
  "{source}",
  "{codec}",
  "{edition}",
  "{part}",
];

export const TV_PLACEHOLDERS: string[] = [
  ...MOVIE_PLACEHOLDERS,
  "{season}",
  "{episode}",
  "{episode_title}",
];
