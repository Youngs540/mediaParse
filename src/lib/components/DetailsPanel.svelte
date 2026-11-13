<script lang="ts">
  import type { CandidateChoice, MatchCandidate, PlanItem } from "../types";
  import {
    basename,
    candidateKey,
    candidateLabel,
    parsedSummary,
    scoreLabel,
  } from "../format";

  let {
    item,
    candidates,
    chosenKey,
    searchText,
    searchBusy,
    providerLabel,
    onChoose,
    onUse,
    onSkip,
    onSearchText,
    onSearch,
  }: {
    item: PlanItem | null;
    candidates: CandidateChoice[];
    chosenKey: string | null;
    searchText: string;
    searchBusy: boolean;
    providerLabel: string;
    onChoose: (candidate: MatchCandidate) => void;
    onUse: () => void;
    onSkip: () => void;
    onSearchText: (value: string) => void;
    onSearch: () => void;
  } = $props();
</script>

<aside class="details">
  <p class="lbl">Selected file</p>
  {#if item}
    <div class="filename" title={item.source}>{basename(item.source)}</div>
    <div class="parsed">Parsed: {parsedSummary(item.parsed)}</div>

    <p class="lbl gap">Candidates</p>
    {#if candidates.length === 0}
      <div class="empty-cand">No candidates found.</div>
    {:else}
      {#each candidates as choice (candidateKey(choice.candidate))}
        <button
          type="button"
          class="cand"
          class:top={chosenKey === candidateKey(choice.candidate)}
          onclick={() => onChoose(choice.candidate)}
        >
          <span>{candidateLabel(choice.candidate)}</span>
          <span class="cand-score">{scoreLabel(choice.score)}</span>
        </button>
      {/each}
    {/if}

    <label class="search-label" for="manual-search">
      Search {providerLabel} manually
    </label>
    <input
      id="manual-search"
      class="field"
      value={searchText}
      placeholder="Title to search"
      disabled={searchBusy}
      oninput={(event) => onSearchText(event.currentTarget.value)}
      onkeydown={(event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          onSearch();
        }
      }}
    />

    <button
      type="button"
      class="btn pri"
      onclick={onUse}
      disabled={!chosenKey || searchBusy}
    >
      Use this match
    </button>
    <button type="button" class="btn" onclick={onSkip} disabled={searchBusy}>
      Skip file
    </button>
  {:else}
    <div class="empty-cand">Select a file to see its match details.</div>
  {/if}
</aside>

<style>
  .details {
    width: 300px;
    flex: none;
    padding: 16px;
    border-left: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 10px;
    overflow-y: auto;
    background: var(--surface);
  }

  .filename {
    font-size: 13px;
    color: var(--muted);
    word-break: break-all;
  }

  .parsed {
    font-size: 13px;
    line-height: 1.45;
  }

  .gap {
    margin-top: 8px;
  }

  .cand {
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    font-size: 13px;
    color: var(--text);
    display: flex;
    justify-content: space-between;
    gap: 8px;
    background: var(--surface);
    text-align: left;
    align-items: center;
    transition: border-color 120ms ease, background 120ms ease;
  }

  .cand:hover {
    background: #faf7fc;
    border-color: var(--border-strong);
  }

  .cand.top {
    border-color: var(--accent);
    background: #f7f9ff;
  }

  .cand-score {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
    flex: none;
  }

  .cand.top .cand-score {
    color: var(--primary-text);
    font-weight: 500;
  }

  .empty-cand {
    font-size: 13px;
    color: var(--muted);
    padding: 8px 0;
  }

  .search-label {
    font-size: 12px;
    color: var(--muted);
    margin-top: 8px;
  }

  @media (max-width: 1100px) {
    .details {
      width: 270px;
      padding: 12px;
    }
  }
</style>
