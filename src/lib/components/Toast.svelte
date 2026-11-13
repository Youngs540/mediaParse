<script lang="ts">
  let {
    toast,
    onClose,
  }: {
    toast: { message: string; kind: "info" | "success" | "error" } | null;
    onClose: () => void;
  } = $props();
</script>

{#if toast}
  <div class="toast {toast.kind}" role="status" aria-live="polite">
    <span class="dot"></span>
    <span class="message">{toast.message}</span>
    <button type="button" class="close" aria-label="Dismiss" onclick={onClose}>
      ×
    </button>
  </div>
{/if}

<style>
  .toast {
    position: fixed;
    right: 20px;
    bottom: 78px;
    max-width: 420px;
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 12px 14px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-left: 3px solid var(--accent);
    border-radius: 10px;
    box-shadow: var(--shadow);
    font-size: 13px;
    color: var(--text);
    z-index: 60;
  }

  .toast.success {
    border-left-color: var(--green);
  }

  .toast.error {
    border-left-color: var(--red);
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
    margin-top: 5px;
    flex: none;
  }

  .toast.success .dot {
    background: var(--green);
  }

  .toast.error .dot {
    background: var(--red);
  }

  .message {
    flex: 1;
    line-height: 1.45;
  }

  .close {
    border: 0;
    background: none;
    color: var(--muted);
    font-size: 18px;
    line-height: 1;
    padding: 0 2px;
    flex: none;
  }

  .close:hover {
    color: var(--text);
  }
</style>
