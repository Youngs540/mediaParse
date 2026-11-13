<script lang="ts">
  import TemplateField from "./TemplateField.svelte";
  import { MOVIE_PLACEHOLDERS, TV_PLACEHOLDERS } from "../templates";
  import type { FilterKey, PlanSummary } from "../types";

  let {
    counts,
    filter,
    template,
    tvTemplate,
    seriesFolderTemplate,
    createFolders,
    createTvFolders,
    moveSidecars,
    offline,
    onFilter,
    onTemplate,
    onTvTemplate,
    onSeriesFolderTemplate,
    onToggleCreate,
    onToggleCreateTv,
    onToggleSidecars,
    onToggleOffline,
  }: {
    counts: PlanSummary;
    filter: FilterKey;
    template: string;
    tvTemplate: string;
    seriesFolderTemplate: string;
    createFolders: boolean;
    createTvFolders: boolean;
    moveSidecars: boolean;
    offline: boolean;
    onFilter: (key: FilterKey) => void;
    onTemplate: (value: string) => void;
    onTvTemplate: (value: string) => void;
    onSeriesFolderTemplate: (value: string) => void;
    onToggleCreate: (value: boolean) => void;
    onToggleCreateTv: (value: boolean) => void;
    onToggleSidecars: (value: boolean) => void;
    onToggleOffline: (value: boolean) => void;
  } = $props();

  const filters = $derived<Array<{ key: FilterKey; label: string; count: number }>>([
    { key: "all", label: "All", count: counts.total },
    { key: "matched", label: "Matched", count: counts.ready },
    { key: "review", label: "Needs review", count: counts.review },
    { key: "unmatched", label: "Unmatched", count: counts.unmatched },
    { key: "conflicts", label: "Conflicts", count: counts.conflicts },
  ]);
</script>

<aside class="sidebar">
  <p class="lbl">Filter</p>
  {#each filters as entry (entry.key)}
    <button
      class="nav"
      class:on={filter === entry.key}
      onclick={() => onFilter(entry.key)}
      aria-pressed={filter === entry.key}
    >
      <span>{entry.label}</span>
      <span class="count">{entry.count}</span>
    </button>
  {/each}

  <p class="lbl spaced">Template</p>
  <TemplateField
    id="sidebar-movie-template"
    label="Movie folder and file name"
    value={template}
    placeholders={MOVIE_PLACEHOLDERS}
    onInput={onTemplate}
  />
  <TemplateField
    id="sidebar-tv-template"
    label="TV episode file name"
    value={tvTemplate}
    placeholders={TV_PLACEHOLDERS}
    onInput={onTvTemplate}
  />
  <TemplateField
    id="sidebar-series-template"
    label="TV series folder"
    value={seriesFolderTemplate}
    placeholders={TV_PLACEHOLDERS}
    onInput={onSeriesFolderTemplate}
  />

  <p class="lbl spaced">Options</p>
  <label class="check">
    <input
      type="checkbox"
      checked={createFolders}
      onchange={(event) => onToggleCreate(event.currentTarget.checked)}
    />
    Create movie folders
  </label>
  <label class="check">
    <input
      type="checkbox"
      checked={createTvFolders}
      onchange={(event) => onToggleCreateTv(event.currentTarget.checked)}
    />
    Create TV series folders
  </label>
  <label class="check">
    <input
      type="checkbox"
      checked={moveSidecars}
      onchange={(event) => onToggleSidecars(event.currentTarget.checked)}
    />
    Move sidecar files
  </label>
  <label class="check">
    <input
      type="checkbox"
      checked={offline}
      onchange={(event) => onToggleOffline(event.currentTarget.checked)}
    />
    Offline (cache only)
  </label>
</aside>

<style>
  .sidebar {
    width: 220px;
    flex: none;
    padding: 16px;
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 4px;
    overflow-y: auto;
    background: var(--surface);
  }

  .spaced {
    margin-top: 20px;
  }

  .nav {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-radius: 8px;
    font-size: 13px;
    color: var(--text);
    border: 0;
    background: none;
    width: 100%;
    text-align: left;
    min-height: 34px;
  }

  .nav:hover {
    background: #f6f1f9;
  }

  .nav.on {
    background: var(--selected);
    box-shadow: inset 3px 0 0 var(--accent);
    font-weight: 500;
  }

  .count {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .nav.on .count {
    color: var(--primary-text);
  }

  .check {
    font-size: 13px;
    display: flex;
    gap: 8px;
    align-items: center;
    min-height: 32px;
    cursor: pointer;
    user-select: none;
  }

  @media (max-width: 1100px) {
    .sidebar {
      width: 200px;
      padding: 12px;
    }
  }
</style>
