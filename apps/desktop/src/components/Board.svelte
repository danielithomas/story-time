<script lang="ts">
  import { chapterNumber, formatNumber, plainLinks, plural } from "../lib/format";
  import { addNote, app, openNote } from "../lib/store.svelte";

  let adding = $state(false);
  let title = $state("");

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    const value = title.trim();
    adding = false;
    title = "";
    if (value) await addNote("chapter", value);
  }

  function focus(node: HTMLInputElement) {
    node.focus();
  }
</script>

{#if app.tree?.book}
  {@const book = app.tree.book}
  <div class="view">
    <header>
      <h1>{book.summary.title}</h1>
      <p class="muted">
        {plural(book.chapters.length, "chapter")} · {plural(book.summary.words, "word")}. Each card shows a chapter's
        synopsis; set it in the inspector while writing.
      </p>
    </header>

    <div class="cards">
      {#each book.chapters as chapter (chapter.path)}
        <button class="card" onclick={() => openNote(chapter.path)}>
          <div class="row">
            <span class="number">{chapterNumber(chapter.number)}</span>
            <span class="badge" data-status={chapter.status ?? ""}>{chapter.status ?? "no status"}</span>
          </div>
          <h3>{chapter.title}</h3>
          <p class="synopsis" class:none={!chapter.synopsis}>{chapter.synopsis ?? "No synopsis yet"}</p>
          <div class="row foot">
            <span>{chapter.pov ? `POV: ${plainLinks(chapter.pov)}` : ""}</span>
            <span>{formatNumber(chapter.words)} words</span>
          </div>
        </button>
      {/each}

      {#if adding}
        <form class="card new" onsubmit={submit}>
          <input bind:value={title} placeholder="Chapter title" use:focus onblur={() => (adding = false)} />
        </form>
      {:else}
        <button class="card new" onclick={() => (adding = true)}>+ Add a chapter</button>
      {/if}
    </div>
  </div>
{/if}

<style>
  .view {
    height: 100%;
    overflow-y: auto;
    padding: 2rem 2.5rem 4rem;
  }

  header {
    margin-bottom: 1.5rem;
  }

  h1 {
    font-size: 26px;
    margin-bottom: 0.3rem;
  }

  header p {
    margin: 0;
  }

  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
    gap: 1rem;
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    min-height: 190px;
    padding: 0.9rem 1rem;
    text-align: left;
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: 10px;
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.04);
  }

  .card:hover {
    border-color: var(--accent);
  }

  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.5rem;
  }

  .number {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  h3 {
    font-size: 17px;
    line-height: 1.3;
  }

  .synopsis {
    flex: 1;
    margin: 0;
    font-family: var(--serif);
    color: var(--text-soft);
    line-height: 1.5;
    display: -webkit-box;
    -webkit-line-clamp: 6;
    line-clamp: 6;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .synopsis.none {
    color: var(--muted);
    font-style: italic;
  }

  .foot {
    font-size: 12px;
    color: var(--muted);
  }

  .card.new {
    align-items: center;
    justify-content: center;
    border-style: dashed;
    background: transparent;
    color: var(--muted);
  }
</style>
