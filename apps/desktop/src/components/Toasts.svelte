<script lang="ts">
  import { app, dismiss } from "../lib/store.svelte";
</script>

<div class="toasts" aria-live="polite">
  {#each app.toasts as t (t.id)}
    <div class="toast" class:error={t.tone === "error"}>
      <span>{t.message}</span>
      {#if t.action}
        {@const action = t.action}
        <button class="btn" onclick={() => { action.run(); dismiss(t.id); }}>{action.label}</button>
      {/if}
      <button class="close" aria-label="Dismiss" onclick={() => dismiss(t.id)}>×</button>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: 1rem;
    bottom: 2.6rem;
    z-index: 30;
    display: grid;
    gap: 0.5rem;
    justify-items: end;
  }

  .toast {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    max-width: 28rem;
    padding: 0.6rem 0.8rem;
    background: var(--paper);
    border: 1px solid var(--line);
    border-left: 3px solid var(--accent);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.12);
  }

  .toast.error {
    border-left-color: var(--danger);
  }

  .close {
    border: none;
    background: none;
    color: var(--muted);
    font-size: 18px;
    line-height: 1;
    padding: 0 2px;
  }
</style>
