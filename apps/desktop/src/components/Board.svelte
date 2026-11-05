<script lang="ts">
  import { formatNumber, plainLinks, plural } from "../lib/format";
  import { addScene, app, openNote } from "../lib/store.svelte";
  import type { ScenePlace, SceneRef } from "../lib/types";

  /** Where the scene being added goes, as a key: "end", "unplaced" or "chapter:N". */
  let adding = $state<string | null>(null);
  let title = $state("");

  const key = (place: ScenePlace) => (typeof place === "object" ? `chapter:${place.chapter}` : place);

  async function submit(event: SubmitEvent, place: ScenePlace) {
    event.preventDefault();
    const value = title.trim();
    adding = null;
    title = "";
    if (value) await addScene(value, place);
  }

  function focus(node: HTMLInputElement) {
    node.focus();
  }

  const where = (scene: SceneRef) => [scene.pov, scene.location].filter(Boolean).map((s) => plainLinks(s!)).join(" · ");
</script>

{#snippet card(scene: SceneRef)}
  <button class="card" onclick={() => openNote(scene.path)}>
    <div class="row">
      <span class="when">{scene.when ?? ""}</span>
      <span class="badge" data-status={scene.status ?? ""}>{scene.status ?? "no status"}</span>
    </div>
    <h3>{scene.title}</h3>
    <p class="synopsis" class:none={!scene.synopsis}>{scene.synopsis ?? "No synopsis yet"}</p>
    <div class="row foot">
      <span>{where(scene)}</span>
      <span>{formatNumber(scene.words)} words</span>
    </div>
  </button>
{/snippet}

{#snippet addCard(place: ScenePlace, label: string)}
  {#if adding === key(place)}
    <form class="card new" onsubmit={(e) => submit(e, place)}>
      <input bind:value={title} placeholder="Scene title" use:focus onblur={() => (adding = null)} />
    </form>
  {:else}
    <button class="card new" onclick={() => (adding = key(place))}>{label}</button>
  {/if}
{/snippet}

{#if app.tree?.book}
  {@const book = app.tree.book}
  <div class="view">
    <header>
      <h1>{book.summary.title}</h1>
      <p class="muted">
        {plural(book.summary.chapters, "chapter")} · {plural(book.summary.scenes, "scene")} · {plural(book.summary.words, "word")}.
        Each card shows a scene's synopsis; set it in the inspector while writing.
      </p>
    </header>

    {#each book.parts as part, p (p)}
      {#if part.title}
        <h2 class="part">{part.title}</h2>
      {/if}
      {#each part.chapters as chapter (chapter.index)}
        <section>
          <h2>{chapter.heading} <span class="muted">· {plural(chapter.words, "word")}</span></h2>
          <div class="cards">
            {#each chapter.scenes as scene (scene.path)}
              {@render card(scene)}
            {/each}
            {@render addCard({ chapter: chapter.index }, "+ Add a scene")}
          </div>
        </section>
      {/each}
    {/each}

    <section>
      <div class="cards">
        {@render addCard("end", "+ Add a scene in a new chapter")}
      </div>
    </section>

    {#if book.unplaced.length}
      <section>
        <h2>Unplaced <span class="muted">· not in the contents yet</span></h2>
        <div class="cards">
          {#each book.unplaced as scene (scene.path)}
            {@render card(scene)}
          {/each}
          {@render addCard("unplaced", "+ Add an unplaced scene")}
        </div>
      </section>
    {/if}
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

  section {
    margin-bottom: 1.6rem;
  }

  h2 {
    font-size: 15px;
    margin-bottom: 0.7rem;
  }

  h2 .muted {
    font-weight: normal;
    font-size: 13px;
  }

  h2.part {
    font-size: 12px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
    margin: 2rem 0 0.9rem;
    padding-bottom: 0.4rem;
    border-bottom: 1px solid var(--line);
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
    min-height: 170px;
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

  .when {
    color: var(--muted);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
    min-height: 90px;
  }
</style>
