<script lang="ts">
  import { formatNumber, plural } from "../lib/format";
  import { app, deleteBook, deleteProject } from "../lib/store.svelte";

  /** What to delete: the open book, or the open world with everything in it. */
  let { kind, close }: { kind: "book" | "project"; close: () => void } = $props();

  let withScenes = $state(false);
  let typed = $state("");
  let busy = $state(false);

  const CONFIRM = "DELETE";
  const ready = $derived(typed.trim() === CONFIRM);

  const world = $derived(app.projects.find((project) => project.slug === app.slug));
  const book = $derived(app.tree?.book?.summary);
  const title = $derived((kind === "book" ? book?.title : world?.title) ?? "");
  const books = $derived(world?.books ?? []);
  const scenes = $derived(kind === "book" ? (book?.scenes ?? 0) : books.reduce((n, b) => n + b.scenes, 0));
  const words = $derived(kind === "book" ? (book?.words ?? 0) : books.reduce((n, b) => n + b.words, 0));

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!ready || busy) return;
    busy = true;
    if (kind === "book") await deleteBook(withScenes);
    else await deleteProject();
    close();
  }

  function focus(node: HTMLElement) {
    node.focus();
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && !busy && close()} />

<div class="backdrop">
  <div
    class="dialog"
    role="alertdialog"
    aria-modal="true"
    aria-labelledby="delete-folder-title"
    aria-describedby="delete-folder-what"
  >
    <form onsubmit={submit}>
      <h2 id="delete-folder-title">Delete {title}?</h2>
      <div id="delete-folder-what">
        {#if kind === "project"}
          <p>
            The whole world goes to the system trash in one piece: {plural(books.length, "book")}
            ({plural(scenes, "scene")}, {formatNumber(words)} words), its world bible, research and scratchpad.
          </p>
        {:else if scenes && !withScenes}
          <p>
            Its {plural(scenes, "scene")} ({formatNumber(words)} words) move to {world?.title}'s scratchpad, keeping
            their properties, so you can put them back into a book. The rest of the book, its overview and story flow,
            goes to the system trash.
          </p>
        {:else if scenes}
          <p class="warning">
            The whole book goes to the system trash: its overview, story flow and {plural(scenes, "scene")}
            ({formatNumber(words)} words).
          </p>
        {:else}
          <p>It has no scenes. Its overview and story flow go to the system trash.</p>
        {/if}
        <p class="muted">You can restore it from the system trash, but Story Time can't undo this.</p>
      </div>

      {#if kind === "book" && scenes}
        <label class="check">
          <input type="checkbox" bind:checked={withScenes} />
          Delete its scenes too
        </label>
      {/if}

      <label class="field">
        <span>Type {CONFIRM} to confirm</span>
        <input bind:value={typed} autocomplete="off" spellcheck="false" use:focus />
      </label>

      <div class="actions">
        <button type="button" class="btn" onclick={close} disabled={busy}>Cancel</button>
        <button type="submit" class="btn danger" disabled={busy || !ready}>
          {kind === "book" ? "Delete book" : "Delete world"}
        </button>
      </div>
    </form>
  </div>
</div>

<style>
  .warning {
    padding: 0.6rem 0.8rem;
    border-radius: 8px;
    background: var(--danger-soft);
  }

  .check {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 0.9rem;
  }

  .field {
    display: grid;
    gap: 0.3rem;
  }

  .field span {
    font-size: 13px;
  }
</style>
