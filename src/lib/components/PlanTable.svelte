<script lang="ts">
  import type { PlanItem } from "../types";
  import {
    basename,
    relativeTo,
    scoreLabel,
    statusClass,
    statusLabel,
  } from "../format";

  let {
    items,
    root,
    checked,
    activeId,
    emptyMessage,
    onToggle,
    onActivate,
  }: {
    items: PlanItem[];
    root: string;
    checked: Set<string>;
    activeId: string | null;
    emptyMessage: string;
    onToggle: (id: string) => void;
    onActivate: (id: string) => void;
  } = $props();

  function newName(item: PlanItem): string {
    if (item.status === "Unmatched") {
      return "No match found";
    }
    const target = relativeTo(root, item.target);
    if (item.status === "Conflict") {
      if (item.reason?.includes("exists")) {
        return `${target} already exists`;
      }
      return item.reason ? `${target} — ${item.reason}` : target;
    }
    if (item.status === "Skipped") {
      return item.reason ? `${target} — ${item.reason}` : target;
    }
    return target;
  }
</script>

<div class="table" role="table" aria-label="Rename preview">
  <div class="row head" role="row">
    <span role="columnheader"></span>
    <span role="columnheader">Status</span>
    <span class="old" role="columnheader">Original</span>
    <span role="columnheader">New name</span>
    <span class="score" role="columnheader">Score</span>
  </div>

  {#if items.length === 0}
    <div class="empty">{emptyMessage}</div>
  {:else}
    {#each items as item (item.id)}
      <div
        class="row"
        class:sel={activeId === item.id}
        role="row"
        tabindex="0"
        onclick={() => onActivate(item.id)}
        onkeydown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            onActivate(item.id);
          }
        }}
        aria-selected={activeId === item.id}
        aria-label={`${basename(item.source)} — ${statusLabel(item.status)}`}
      >
        <span role="cell">
          <input
            type="checkbox"
            aria-label={`Select ${basename(item.source)}`}
            checked={checked.has(item.id)}
            onclick={(event) => event.stopPropagation()}
            onchange={() => onToggle(item.id)}
          />
        </span>
        <span role="cell">
          <span class="chip {statusClass(item.status)}">
            {statusLabel(item.status)}
          </span>
        </span>
        <span class="old" role="cell" title={relativeTo(root, item.source)}>
          {basename(item.source)}
        </span>
        <span
          class="new"
          role="cell"
          class:muted={item.status === "Unmatched" || item.status === "Skipped"}
          title={newName(item)}
        >
          {newName(item)}
        </span>
        <span class="score" role="cell">{scoreLabel(item.score)}</span>
      </div>
    {/each}
  {/if}
</div>

<style>
  .table {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
  }

  .row {
    display: grid;
    grid-template-columns: 28px 112px 1fr 1fr 56px;
    gap: 12px;
    align-items: center;
    padding: 10px 16px;
    border-bottom: 1px solid var(--border);
    font-size: 13px;
    color: var(--text);
    min-height: 45px;
    cursor: default;
  }

  .row:not(.head):hover {
    background: #faf7fc;
  }

  .row.sel {
    background: var(--selected);
    box-shadow: inset 3px 0 0 var(--accent);
  }

  .row.sel:hover {
    background: var(--selected);
  }

  .row.head {
    font-size: 11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
    position: sticky;
    top: 0;
    background: var(--surface);
    z-index: 1;
    font-weight: 500;
    cursor: default;
  }

  .old {
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .new {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .new.muted {
    color: var(--muted);
  }

  .score {
    font-variant-numeric: tabular-nums;
    text-align: right;
  }

  .empty {
    padding: 48px 24px;
    text-align: center;
    color: var(--muted);
    font-size: 13px;
  }

  /* Narrower windows keep the new name readable by dropping the original
     path and score columns instead of squeezing both name columns. */
  @media (max-width: 1100px) {
    .row {
      grid-template-columns: 28px 112px minmax(0, 1fr);
    }

    .old,
    .score {
      display: none;
    }
  }
</style>
