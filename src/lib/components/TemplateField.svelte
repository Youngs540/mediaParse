<script lang="ts">
  import { tick } from "svelte";

  let {
    id,
    label,
    value,
    placeholders,
    hint,
    onInput,
  }: {
    id: string;
    label: string;
    value: string;
    placeholders: string[];
    hint?: string;
    onInput: (value: string) => void;
  } = $props();

  let inputEl = $state<HTMLInputElement | null>(null);
  let wrapper = $state<HTMLDivElement | null>(null);
  let open = $state(false);

  function toggleMenu() {
    open = !open;
  }

  function closeIfOutside(event: MouseEvent) {
    if (!open) return;
    if (wrapper && event.composedPath().includes(wrapper)) {
      return;
    }
    open = false;
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape" && open) {
      event.stopPropagation();
      open = false;
      inputEl?.focus();
    }
  }

  async function insert(token: string) {
    const el = inputEl;
    if (!el) {
      if (!value.includes(token)) {
        onInput(`${value}${token}`);
      }
      return;
    }

    if (el.value.includes(token)) {
      el.focus();
      return;
    }

    const start = el.selectionStart ?? el.value.length;
    const end = el.selectionEnd ?? start;
    onInput(`${el.value.slice(0, start)}${token}${el.value.slice(end)}`);

    await tick();
    el.focus();
    const caret = start + token.length;
    el.setSelectionRange(caret, caret);
  }

  async function remove(token: string) {
    onInput(cleanTemplate(value.split(token).join("")));

    await tick();
    // Keep the picker open after removing a token, even if focus briefly
    // leaves the chip while the selected state re-renders.
    open = true;
    const el = inputEl;
    if (el) {
      el.focus();
      const end = el.value.length;
      el.setSelectionRange(end, end);
    }
  }

  function cleanTemplate(input: string): string {
    return input
      .replace(/\[\s*tmdbid\s*-\s*\]/gi, " ")
      .replace(/\(\s*\)/g, " ")
      .replace(/\[\s*\]/g, " ")
      .replace(/\{\s*\}/g, " ")
      .replace(/\s*-\s*$/, " ")
      .replace(/^\s*-\s*/, " ")
      .replace(/\s{2,}/g, " ")
      .trim();
  }
</script>

<svelte:window onclick={closeIfOutside} />

<div
  class="template-field"
  bind:this={wrapper}
  onfocusout={(event) => {
    const next = event.relatedTarget as Node | null;
    if (open && next && !event.currentTarget.contains(next)) {
      open = false;
    }
  }}
>
  <label class="field-label" for={id}>{label}</label>
  <div class="input-row">
    <input
      id={id}
      class="field template-input"
      value={value}
      spellcheck="false"
      autocomplete="off"
      oninput={(event) => onInput(event.currentTarget.value)}
      onkeydown={handleKeydown}
    />
    <button
      type="button"
      class="chev"
      class:open
      aria-label={`Insert placeholder into ${label}`}
      aria-expanded={open}
      aria-controls={`${id}-placeholders`}
      title="Insert placeholder"
      onclick={toggleMenu}
      onkeydown={handleKeydown}
    >
      ▾
    </button>
  </div>

  {#if open}
    <div
      class="menu"
      id={`${id}-placeholders`}
      role="group"
      aria-label="Placeholders"
    >
      <p class="menu-label">Insert placeholder</p>
      <div class="tokens">
        {#each placeholders as token (token)}
          {@const selected = value.includes(token)}
          <span class="token-wrap" class:selected>
            <button
              type="button"
              class="token"
              aria-pressed={selected}
              title={selected
                ? `${token} is already in the template`
                : `Insert ${token}`}
              onclick={() => insert(token)}
              onkeydown={handleKeydown}
            >
              {token}
            </button>
            {#if selected}
              <button
                type="button"
                class="token-remove"
                aria-label={`Remove ${token} from template`}
                title={`Remove ${token}`}
                onmousedown={(event) => event.preventDefault()}
                onclick={() => remove(token)}
                onkeydown={handleKeydown}
              >
                ×
              </button>
            {/if}
          </span>
        {/each}
      </div>
    </div>
  {/if}

  {#if hint}
    <p class="hint">{hint}</p>
  {/if}
</div>

<style>
  .template-field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    position: relative;
  }

  .field-label {
    font-size: 12px;
    color: var(--muted);
  }

  .input-row {
    position: relative;
    display: flex;
    align-items: center;
  }

  .template-input {
    width: 100%;
    padding-right: 36px;
    font-size: 13px;
  }

  .chev {
    position: absolute;
    right: 4px;
    top: 50%;
    transform: translateY(-50%);
    width: 28px;
    height: 28px;
    border: 0;
    background: transparent;
    color: var(--muted);
    border-radius: 6px;
    font-size: 12px;
    line-height: 1;
    cursor: pointer;
  }

  .chev:hover,
  .chev.open {
    background: #f2edf7;
    color: var(--text);
  }

  .menu {
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 8px;
    background: var(--surface);
    box-shadow: var(--shadow);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .menu-label {
    margin: 0;
    font-size: 10px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
    font-weight: 500;
  }

  .tokens {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .token-wrap {
    display: inline-flex;
    align-items: stretch;
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
    background: #faf7fc;
  }

  .token-wrap:not(.selected):hover {
    border-color: var(--accent);
    background: #f7f9ff;
  }

  .token-wrap.selected {
    border-color: var(--primary);
    background: var(--primary);
  }

  .token {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 11px;
    color: var(--text);
    background: transparent;
    border: 0;
    padding: 4px 7px;
    cursor: pointer;
  }

  .token-wrap.selected .token {
    color: #fff;
  }

  .token-remove {
    border: 0;
    border-left: 1px solid rgba(255, 255, 255, 0.4);
    background: transparent;
    color: #fff;
    font-size: 13px;
    line-height: 1;
    padding: 0 6px;
    cursor: pointer;
  }

  .token-remove:hover {
    background: rgba(0, 0, 0, 0.15);
  }

  .hint {
    margin: 0;
    font-size: 11px;
    color: var(--muted);
    line-height: 1.6;
  }
</style>
