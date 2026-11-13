# mediaParse

A fast, local-first desktop app that renames movie and TV episode files into
Jellyfin-friendly names. It scans a library, parses messy release names, matches
each title against TMDB or OMDb, and shows a dry-run preview before anything on
disk is touched.

The app is built as a Rust workspace with a Tauri v2 shell and a Svelte 5 user
interface. All parsing, scoring, planning, and file execution happen in Rust;
the UI only renders state and sends commands.

## Features

- **Movie and TV libraries** — separate matching for movies and series, with
  `Season 01` folders, `S00` specials, and multi-episode files such as
  `S01E02-E03`.
- **Jellyfin-compatible names** — movies become
  `Title (Year)/Title (Year).ext`; episodes become
  `Series (Year)/Season 01/Series (Year) S01E02 - Episode Title.ext`. Both can
  carry an optional `[tmdbid-12345]` suffix.
- **Robust parsing** — dots, underscores, release groups, quality/source/codec
  tags, editions, multi-part markers, TV episode patterns (`S01E02`, `1x02`,
  `Season 1 Episode 2`, multi-episode ranges), season folders, and a
  parent-folder fallback.
- **Episode titles** — recovered from the file name when present, otherwise
  fetched from the metadata provider.
- **Template placeholder picker** — use the `▾` control in a naming field to
  insert placeholders at the caret. Placeholders already in the template are
  highlighted in blue and can be removed with `×`.
- **Fuzzy matching** — normalized title similarity plus year and optional
  ffprobe runtime. Configurable auto-accept and review thresholds. Candidate
  searches are filtered by media type, so movies and series cannot cross-match.
- **Manual review** — pick a different candidate, search the provider by title,
  or skip a file.
- **Dry-run planner** — detects target collisions, existing files, and
  case-only renames before applying.
- **Crash-safe execution** — writes intent to a SQLite journal, renames on the
  same device, or copies + verifies (BLAKE3) + deletes across devices.
- **Undo** — restores the most recent run in reverse journal order.
- **Sidecars** — subtitles, artwork, and other companions follow the video name.
- **Parallel and rate-limited** — bounded concurrent metadata lookups with a
  token-bucket rate limiter; scanning and scoring use Rayon.
- **Browser demo mode** — the Svelte UI runs with fixture data when it is not
  hosted inside Tauri, which makes visual iteration fast.

## Workspace layout

```text
crates/core   Pure domain: parser, matcher, formatter, sanitizer, planner.
              No filesystem, network, database, or clock I/O.
crates/infra  Scanner, sidecar grouping, TMDB/OMDb clients, rate limiting,
              SQLite metadata cache and journal, ffprobe, executor.
crates/app    Application services (scan, build plan, rebuild, apply, undo)
              plus the Tauri command surface behind the `desktop` feature.
src-tauri     Desktop shell and Tauri configuration.
src/          Svelte 5 UI, components, API wrappers, and demo fixtures.
```

The dependency direction is `UI -> app -> infra -> core`. The core crate is
deterministic and fully unit-testable.

## Requirements

- Rust **1.77.2+** and Cargo
- Node.js **20+** and npm
- Platform prerequisites for Tauri v2:
  - **Linux:** `libwebkit2gtk-4.1-dev`, `build-essential`, `curl`, `wget`,
    `file`, `libxdo-dev`, `libssl-dev`, `libayatana-appindicator3-dev`,
    `librsvg2-dev`, and `patchelf`
  - **macOS:** Xcode Command Line Tools
  - **Windows:** Microsoft C++ Build Tools and WebView2 (installed by the
    Tauri installer on most systems)

On Debian/Ubuntu:

```bash
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev patchelf
```

## Development

Install frontend dependencies once:

```bash
npm install
```

Run the browser demo (fixture data, no Tauri required):

```bash
npm run dev
# http://127.0.0.1:1420
```

Run the real desktop app:

```bash
npm run tauri dev
```

Build the frontend only:

```bash
npm run build
```

## Tests and checks

```bash
# Full Rust workspace tests
cargo test --workspace

# Domain and infrastructure only
cargo test -p mediaparse-core -p mediaparse-infra

# Application layer without Tauri/WebKit
cargo test -p mediaparse-app --no-default-features

# Svelte type and accessibility checks
npm run check

# Frontend production build
npm run build

# Rust formatting and lints
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

The desktop shell can be type-checked with:

```bash
cargo check -p mediaparse-desktop
```

That command needs the Linux WebKitGTK development packages listed above.
The `mediaparse-app` tests intentionally use `--no-default-features` so the
application layer can be verified on machines without a GUI toolkit.

## Packaging

Tauri builds native installers from the same configuration:

```bash
# All bundles for the current OS
npm run tauri build

# Pick specific bundles
npm run tauri build -- --bundles deb,appimage
npm run tauri build -- --bundles dmg
npm run tauri build -- --bundles msi,nsis
```

Artifacts are written to `target/release/bundle/`, for example:

```text
target/release/bundle/deb/mediaParse_0.1.0_amd64.deb
target/release/bundle/appimage/mediaParse_0.1.0_amd64.AppImage
```

The GitHub Actions workflow in `.github/workflows/release.yml` builds:

| Runner | Artifacts |
| --- | --- |
| `ubuntu-22.04` | `.deb`, `.AppImage` |
| `macos-latest` | universal `.dmg` |
| `windows-latest` | `.msi`, NSIS `.exe` |

Push a version tag to publish a draft GitHub release:

```bash
git tag v0.1.0
git push origin v0.1.0
```

The workflow can also be started manually from the Actions tab. It is a draft
release by default so the assets can be checked before publishing. The workflow
does not code-sign or notarize the binaries; add the appropriate Apple and
Windows signing secrets before distributing production builds.

## Configuration

Settings are stored in a TOML file and can also be supplied through
environment variables.

| OS | Config | Journal / cache |
| --- | --- | --- |
| Linux | `~/.config/mediaparse/config.toml` | `~/.local/share/mediaparse/mediaparse.sqlite3` |
| macOS | `~/Library/Application Support/mediaparse/config.toml` | `~/Library/Application Support/mediaparse/mediaparse.sqlite3` |
| Windows | `%APPDATA%\mediaparse\config.toml` | `%APPDATA%\mediaparse\mediaparse.sqlite3` |

Environment overrides:

| Variable | Purpose |
| --- | --- |
| `TMDB_API_KEY` | TMDB v3 API key |
| `TMDB_READ_TOKEN` | TMDB v4 read access token |
| `OMDB_API_KEY` | OMDb API key |
| `MEDIAPARSE_PROVIDER` | `tmdb` or `omdb` |
| `MEDIAPARSE_LANGUAGE` | Metadata language, for example `en-US` |
| `MEDIAPARSE_OFFLINE` | `1`, `true`, or `yes` to use only cached metadata |

### Metadata provider credentials

TMDB accepts either credential:

- `tmdb_read_token` — v4 read access token, sent as a Bearer header
  (recommended).
- `tmdb_api_key` — v3 API key, sent as a query parameter (fallback).

Only one is required. If both are configured, the read access token is used.
OMDb requires `omdb_api_key`.

TMDB returns TMDB IDs, so `[tmdbid-{id}]` is correct with the TMDB provider.
OMDb returns IMDb IDs (`tt...`); when using OMDb, use `[imdbid-{id}]` or omit
the ID tag from your templates.

### Naming templates

In both the sidebar and Settings, click the `▾` control in a template field to
open the placeholder picker. Click a placeholder to insert it at the caret.
Placeholders already present are shown in blue with an `×`; click `×` to remove
that placeholder and clean up empty delimiters.

The default movie template is:

```text
{title} ({year}) [tmdbid-{id}]
```

Supported movie placeholders:

```text
{title} {year} {id} {provider} {quality} {source} {codec} {edition} {part}
```

TV files use a separate episode template:

```text
{title} ({year}) S{season}E{episode} - {episode_title}
```

In addition to the movie placeholders, the episode template supports
`{season}`, `{episode}`, and `{episode_title}`. Episode and season numbers are
zero-padded. Multi-episode files render `{episode}` as `02-E03`. Specials use
`Season 00` when no season is detected but the file is inside a `Specials`
folder.

TV series folders use their own template, defaulting to:

```text
{title} ({year}) [tmdbid-{id}]
```

The season folder is created beneath it. Set `create_tv_folders = false` to
place episodes directly beside their source instead. Episode titles come from
the file name when present and are otherwise fetched from the metadata provider.

If the movie template omits `{edition}` or `{part}`, those values are appended to
the base name. A version suffix (for example `1080p`) is added when two files map
to the same movie. TV episodes are grouped by series, season, and episode, so
different episodes never receive spurious version suffixes. Invalid characters,
trailing dots/spaces, Windows reserved names, and overlong components are
sanitized, and names are normalized to Unicode NFC.

### External IDs and Jellyfin

`[tmdbid-603]` is an optional provider-ID hint. Jellyfin recognizes it and can
match the item directly instead of relying only on title and year. It is not
required, and a wrong ID is worse than no ID because Jellyfin trusts it. For TV,
place the ID on the series folder (mediaParse does this), not on each episode.

### Matching

Candidate searches are filtered by media type, so a movie cannot be matched to a
series (or vice versa).

- `auto_accept_threshold` (default `0.85`) — scores at or above this are
  `Ready` and can be applied without review.
- `review_threshold` (default `0.55`) — scores between the two thresholds are
  `Review`; lower scores are `Unmatched`.
- `concurrency` (default `8`, clamped to 1–16) — concurrent metadata lookups.
- `requests_per_second` (default `10`, clamped to 1–50) — provider rate limit.
- `ffprobe_path` — optional path to `ffprobe` for runtime-based tie-breaking.

## Safety model

1. **Scan** reads the library and groups videos with their sidecars.
2. **Build plan** parses, matches, formats, and validates every target. It is a
   pure dry run.
3. **Review** lets you change a candidate, search manually, or skip a file.
4. **Apply** writes a journal entry for each intended rename, performs the
   operation, then marks it done. Same-device renames use `rename(2)`.
   Cross-device moves copy to a temporary target, verify the BLAKE3 hash, and
   only then delete the source.
5. **Undo** walks the last undoable run in reverse and restores each completed
   entry. Interrupted runs remain visible in the journal.

Nothing is written to the library until **Apply** is pressed. The browser demo
only shows dry-run toasts.

## Example

Input:

```text
The.Matrix.1999.1080p.BluRay.x264-YTS.mkv
inception_2010_720p_web-dl.mp4
Avatar.2009.EXTENDED.2160p.HDR.REMUX.mkv
Dune 2021 CD1.avi
The.Office.US.S01E02.1080p.WEB-DL.mkv
Breaking.Bad.S02E05E06.720p.HDTV.mkv
```

Output with the default templates:

```text
The Matrix (1999) [tmdbid-603]/The Matrix (1999) [tmdbid-603].mkv
Inception (2010) [tmdbid-27205]/Inception (2010) [tmdbid-27205].mp4
Avatar (2009) [tmdbid-19995]/Avatar (2009) [tmdbid-19995] - Extended.mkv
Dune (2021) [tmdbid-438631]/Dune (2021) [tmdbid-438631] - part1.avi
The Office (2005) [tmdbid-2316]/Season 01/The Office (2005) S01E02 - The Dundies.mkv
Breaking Bad (2008) [tmdbid-1396]/Season 02/Breaking Bad (2008) S02E05-E06 - Breakage.mkv
```

## License

MIT
