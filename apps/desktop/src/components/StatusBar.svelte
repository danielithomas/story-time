<script lang="ts">
  import { formatNumber, plural, signed } from "../lib/format";
  import { app, openWaiting, save, syncNow, type SaveState } from "../lib/store.svelte";

  const labels: Record<SaveState, string> = {
    saved: "Saved",
    unsaved: "Unsaved changes",
    saving: "Saving…",
    conflict: "Changed elsewhere: not saved",
    error: "Couldn't save",
  };

  const met = $derived(!!app.today && app.today.goal > 0 && app.today.words >= app.today.goal);

  const syncLabel = $derived.by(() => {
    const sync = app.sync;
    if (sync.state === "syncing") return "Syncing…";
    if (sync.state === "failed") return "Not synced";
    if (sync.state === "synced" && sync.at) {
      const time = new Date(sync.at).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
      return `Synced ${time}`;
    }
    return "Sync";
  });
</script>

<footer class="status" class:focus={app.focus}>
  <button class="save" data-state={app.saveState} title="Save now (Ctrl+S)" onclick={() => save()}>
    {labels[app.saveState]}
  </button>
  {#if app.doc}
    <span>{plural(app.docWords, "word")}</span>
  {/if}
  {#if app.focus}
    <span>Esc to leave focus</span>
  {/if}
  <span class="spacer"></span>
  {#if app.sync.waiting.length}
    <button class="waiting" title="Changed here and on another computer: choose which version to keep" onclick={() => openWaiting(app.sync.waiting[0].path)}>
      {app.sync.waiting.length === 1 ? "1 note has two versions" : `${app.sync.waiting.length} notes have two versions`}
    </button>
  {/if}
  {#if app.vault?.syncAddress && !app.focus}
    <button
      class="sync"
      data-state={app.sync.state}
      title={app.sync.state === "failed" ? app.sync.error : "Sync with your other computers now"}
      disabled={app.sync.state === "syncing"}
      onclick={() => syncNow()}
    >
      {syncLabel}
    </button>
  {/if}
  {#if app.tree?.book}
    {@const book = app.tree.book.summary}
    <span>
      {book.title}: {formatNumber(book.words)}{#if book.target}&nbsp;of {formatNumber(book.target)}{/if}
    </span>
  {/if}
  {#if app.today}
    <span class:met>
      Today {signed(app.today.words)}{#if app.today.goal}&nbsp;of {formatNumber(app.today.goal)}{/if}{#if met}&nbsp;✓{/if}
    </span>
    {#if app.today.streak > 0}
      <span>{app.today.streak}-day streak</span>
    {/if}
  {/if}
</footer>

<style>
  .status {
    grid-area: status;
    display: flex;
    align-items: center;
    gap: 1.25rem;
    padding: 0 0.9rem;
    font-size: 12px;
    color: var(--muted);
    border-top: 1px solid var(--line);
    background: var(--bg);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .status.focus {
    background: var(--paper);
    border-top-color: transparent;
    opacity: 0.35;
    transition: opacity 0.3s;
  }

  .status.focus:hover {
    opacity: 1;
  }

  .spacer {
    flex: 1;
  }

  .save {
    border: none;
    background: none;
    padding: 0;
    color: inherit;
  }

  .save[data-state="conflict"],
  .save[data-state="error"] {
    color: var(--danger);
    font-weight: 600;
  }

  .met {
    color: var(--final);
  }

  .sync,
  .waiting {
    border: none;
    background: none;
    padding: 0;
    color: inherit;
    cursor: pointer;
  }

  .sync[data-state="failed"],
  .waiting {
    color: var(--danger);
    font-weight: 600;
  }
</style>
