<script lang="ts">
  import TemplateField from "./TemplateField.svelte";
  import { MOVIE_PLACEHOLDERS, TV_PLACEHOLDERS } from "../templates";
  import type { Config } from "../types";

  const movieHint =
    "Placeholders: {title} {year} {id} {provider} {quality} {source} {codec} {edition} {part}";
  const tvHint =
    "Additional placeholders: {season} {episode} {episode_title}. Multi-episode files render {episode} as 02-E03.";
  const seriesHint =
    "The season folder (Season 01, or Season 00 for specials) is always created beneath this folder.";

  let {
    open,
    config,
    saving,
    demo,
    onSave,
    onClose,
  }: {
    open: boolean;
    config: Config;
    saving: boolean;
    demo: boolean;
    onSave: (config: Config) => void;
    onClose: () => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  let draft = $state<Config>({ ...config });

  $effect(() => {
    if (open) {
      draft = { ...config };
    }
  });

  function submit(event: SubmitEvent) {
    event.preventDefault();
    onSave({ ...draft });
  }
</script>

<svelte:window
  onkeydown={(event) => {
    if (open && event.key === "Escape") {
      onClose();
    }
  }}
/>

{#if open}
  <div
    class="overlay"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) {
        onClose();
      }
    }}
  >
    <!-- svelte-ignore a11y_no_noninteractive_element_to_interactive_role -->
    <form
      class="panel"
      role="dialog"
      aria-modal="true"
      aria-labelledby="settings-title"
      onsubmit={submit}
    >
      <div class="head">
        <div>
          <h2 id="settings-title">Settings</h2>
          <p class="sub">Metadata, matching, and naming defaults</p>
        </div>
        <button
          type="button"
          class="close"
          aria-label="Close settings"
          onclick={onClose}
        >
          ×
        </button>
      </div>

      <section>
        <p class="lbl">Metadata</p>
        <div class="grid two">
          <label class="stack">
            <span>Provider</span>
            <select class="field" bind:value={draft.provider}>
              <option value="tmdb">TMDB</option>
              <option value="omdb">OMDb</option>
            </select>
          </label>
          <label class="stack">
            <span>Language</span>
            <input class="field" bind:value={draft.language} spellcheck="false" />
          </label>
        </div>

        {#if draft.provider === "tmdb"}
          <label class="stack">
            <span>TMDB read access token</span>
            <input
              class="field"
              type="password"
              bind:value={draft.tmdb_read_token}
              autocomplete="off"
              spellcheck="false"
            />
          </label>
          <label class="stack">
            <span>TMDB API key (alternative)</span>
            <input
              class="field"
              type="password"
              bind:value={draft.tmdb_api_key}
              autocomplete="off"
              spellcheck="false"
            />
          </label>
        {:else}
          <label class="stack">
            <span>OMDb API key</span>
            <input
              class="field"
              type="password"
              bind:value={draft.omdb_api_key}
              autocomplete="off"
              spellcheck="false"
            />
          </label>
        {/if}

        <label class="check">
          <input type="checkbox" bind:checked={draft.offline} />
          Offline mode (use cached metadata only)
        </label>
      </section>

      <section>
        <p class="lbl">Matching</p>
        <div class="grid two">
          <label class="stack">
            <span>Auto-accept threshold</span>
            <input
              class="field"
              type="number"
              min="0.01"
              max="1"
              step="0.01"
              bind:value={draft.auto_accept_threshold}
            />
          </label>
          <label class="stack">
            <span>Review threshold</span>
            <input
              class="field"
              type="number"
              min="0.01"
              max="1"
              step="0.01"
              bind:value={draft.review_threshold}
            />
          </label>
          <label class="stack">
            <span>Concurrent lookups</span>
            <input
              class="field"
              type="number"
              min="1"
              max="16"
              step="1"
              bind:value={draft.concurrency}
            />
          </label>
          <label class="stack">
            <span>Requests per second</span>
            <input
              class="field"
              type="number"
              min="1"
              max="50"
              step="1"
              bind:value={draft.requests_per_second}
            />
          </label>
          <label class="stack">
            <span>Cache TTL (days)</span>
            <input
              class="field"
              type="number"
              min="1"
              step="1"
              bind:value={draft.cache_ttl_days}
            />
          </label>
          <label class="stack">
            <span>ffprobe path (optional)</span>
            <input
              class="field"
              bind:value={draft.ffprobe_path}
              placeholder="ffprobe"
              spellcheck="false"
            />
          </label>
        </div>
      </section>

      <section>
        <p class="lbl">Naming</p>
        <TemplateField
          id="settings-movie-template"
          label="Movie folder and file template"
          value={draft.template}
          placeholders={MOVIE_PLACEHOLDERS}
          hint={movieHint}
          onInput={(value) => (draft.template = value)}
        />
        <TemplateField
          id="settings-tv-template"
          label="TV episode file template"
          value={draft.tv_template}
          placeholders={TV_PLACEHOLDERS}
          hint={tvHint}
          onInput={(value) => (draft.tv_template = value)}
        />
        <TemplateField
          id="settings-series-template"
          label="TV series folder template"
          value={draft.series_folder_template}
          placeholders={TV_PLACEHOLDERS}
          hint={seriesHint}
          onInput={(value) => (draft.series_folder_template = value)}
        />
        <div class="grid two">
          <label class="stack">
            <span>Max component length</span>
            <input
              class="field"
              type="number"
              min="32"
              max="240"
              step="1"
              bind:value={draft.max_component_len}
            />
          </label>
        </div>
        <label class="check">
          <input type="checkbox" bind:checked={draft.create_movie_folders} />
          Create movie folders
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={draft.create_tv_folders} />
          Create TV series and season folders
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={draft.move_sidecars} />
          Move sidecar files
        </label>
      </section>

      {#if demo}
        <p class="meta">
          Browser demo: settings are saved to local storage only.
        </p>
      {/if}

      <div class="actions">
        <button type="button" class="btn" onclick={onClose} disabled={saving}>
          Cancel
        </button>
        <button type="submit" class="btn pri" disabled={saving}>
          {saving ? "Saving…" : "Save settings"}
        </button>
      </div>
    </form>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(42, 23, 51, 0.42);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
    z-index: 50;
  }

  .panel {
    width: 620px;
    max-width: 100%;
    max-height: 90vh;
    overflow-y: auto;
    background: var(--surface);
    border-radius: 14px;
    box-shadow: var(--shadow);
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }

  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
  }

  .sub {
    margin: 4px 0 0;
    font-size: 12px;
    color: var(--muted);
  }

  .close {
    border: 1px solid var(--border);
    background: var(--surface);
    border-radius: 8px;
    width: 34px;
    height: 34px;
    font-size: 20px;
    line-height: 1;
    color: var(--muted);
    flex: none;
  }

  .close:hover {
    background: #f7f2fa;
  }

  section {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .grid {
    display: grid;
    gap: 12px;
  }

  .grid.two {
    grid-template-columns: 1fr 1fr;
  }

  .stack {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 12px;
    color: var(--muted);
  }

  .stack .field {
    color: var(--text);
  }

  .check {
    font-size: 13px;
    display: flex;
    gap: 8px;
    align-items: center;
    min-height: 30px;
    cursor: pointer;
    user-select: none;
  }

  .meta {
    margin: 0;
    font-size: 11px;
    color: var(--muted);
    line-height: 1.6;
    word-break: break-all;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    border-top: 1px solid var(--border);
    padding-top: 16px;
  }

  @media (max-width: 640px) {
    .grid.two {
      grid-template-columns: 1fr;
    }
  }
</style>
