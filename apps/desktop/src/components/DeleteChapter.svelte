<script lang="ts">
  import { formatNumber, plural } from "../lib/format";
  import { deleteChapter } from "../lib/store.svelte";
  import type { ChapterRef } from "../lib/types";

  let { chapter, close }: { chapter: ChapterRef; close: () => void } = $props();

  /** Delete the chapter's scenes too, which only goes ahead once DELETE is typed. */
  let withScenes = $state(false);
  let typed = $state("");
  let busy = $state(false);

  const CONFIRM = "DELETE";
  const ready = $derived(!withScenes || typed.trim() === CONFIRM);
  const scenes = $derived(plural(chapter.scenes.length, "scene"));

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!ready || busy) return;
    busy = true;
    await deleteChapter(chapter, withScenes);
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
    aria-labelledby="delete-chapter-title"
    aria-describedby="delete-chapter-what"
  >
    <form onsubmit={submit}>
      <h2 id="delete-chapter-title">Delete {chapter.heading}?</h2>
      <p id="delete-chapter-what">
        Its {scenes} stay in the book as unplaced scenes, ready to go into another chapter. You can undo this.
      </p>

      <label class="check">
        <input type="checkbox" bind:checked={withScenes} />
        Delete its scenes too
      </label>

      {#if withScenes}
        <div class="warning">
          <p>
            These go to the system trash, where you can restore them, but Story Time can't undo it:
          </p>
          <ul>
            {#each chapter.scenes as scene (scene.path)}
              <li>{scene.title} <span class="muted">· {formatNumber(scene.words)} words</span></li>
            {/each}
          </ul>
          <label class="field">
            <span>Type {CONFIRM} to confirm</span>
            <input bind:value={typed} autocomplete="off" spellcheck="false" use:focus />
          </label>
        </div>
      {/if}

      <div class="actions">
        <button type="button" class="btn" onclick={close} disabled={busy} use:focus>Cancel</button>
        <button type="submit" class="btn danger" disabled={busy || !ready}>
          {withScenes ? `Delete chapter and ${scenes}` : "Delete chapter"}
        </button>
      </div>
    </form>
  </div>
</div>

<style>
  .check {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .warning {
    margin-top: 0.9rem;
    padding: 0.8rem 0.9rem;
    border-radius: 8px;
    background: var(--danger-soft);
  }

  .warning p {
    margin-bottom: 0.4rem;
  }

  ul {
    margin: 0 0 0.8rem;
    padding-left: 1.2rem;
  }

  .field {
    display: grid;
    gap: 0.3rem;
  }

  .field span {
    font-size: 13px;
  }
</style>
