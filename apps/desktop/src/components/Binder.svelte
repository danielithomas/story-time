<script lang="ts">
  import { formatNumber } from "../lib/format";
  import { addNote, addResearch, addScene, addScrap, app, openNote } from "../lib/store.svelte";
  import type { BibleKind, NoteRef, ScenePlace, SceneRef, ScrapRef } from "../lib/types";

  const bible: { kind: BibleKind; label: string; notes: () => NoteRef[] }[] = [
    { kind: "character", label: "Characters", notes: () => app.tree?.characters ?? [] },
    { kind: "location", label: "Locations", notes: () => app.tree?.locations ?? [] },
    { kind: "world", label: "World notes", notes: () => app.tree?.world ?? [] },
  ];

  const placeholders: Record<BibleKind, string> = {
    character: "Character's name",
    location: "Place name",
    world: "What it is",
  };

  /** Research and scratchpads: the open world's, then those not tied to any world. */
  const notebooks = [
    { inProject: true, label: "Research", scratchpad: "Scratchpad", hint: "this world" },
    { inProject: false, label: "General research", scratchpad: "General scratchpad", hint: "no world in particular" },
  ];

  /** What is being added, as a key: a world-bible kind, research, or where a new scene goes. */
  let adding = $state<string | null>(null);
  let name = $state("");

  const sceneKey = (place: ScenePlace) => (typeof place === "object" ? `chapter:${place.chapter}` : place);

  function start(key: string) {
    adding = key;
    name = "";
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    const key = adding;
    const value = name.trim();
    adding = null;
    if (!key || !value) return;
    if (key === "end" || key === "unplaced") await addScene(value, key);
    else if (key.startsWith("chapter:")) await addScene(value, { chapter: Number(key.slice(8)) });
    else if (key.startsWith("research:")) await addResearch(key === "research:project", value);
    else await addNote(key as BibleKind, value);
  }

  function cancelOnEscape(event: KeyboardEvent) {
    if (event.key === "Escape") adding = null;
  }

  function focus(node: HTMLInputElement) {
    node.focus();
  }

  const isOpen = (path: string) => app.view === "write" && app.doc?.path === path;
</script>

{#snippet addForm(key: string, placeholder: string)}
  {#if adding === key}
    <form onsubmit={submit}>
      <input
        bind:value={name}
        {placeholder}
        use:focus
        onkeydown={cancelOnEscape}
        onblur={() => (adding = null)}
      />
    </form>
  {/if}
{/snippet}

{#snippet scene(scene: SceneRef)}
  <button
    class="item scene"
    class:active={isOpen(scene.path)}
    title={scene.synopsis ?? ""}
    onclick={() => openNote(scene.path)}
  >
    <span class="dot" data-status={scene.status ?? ""}></span>
    <span class="title">{scene.title}</span>
    <span class="count">{formatNumber(scene.words)}</span>
  </button>
{/snippet}

{#snippet scrap(scrap: ScrapRef)}
  <button
    class="item"
    class:active={isOpen(scrap.path)}
    title={scrap.created ? `Kept ${scrap.created}` : ""}
    onclick={() => openNote(scrap.path)}
  >
    <span class="title">{scrap.title}</span>
    <span class="count">{formatNumber(scrap.words)}</span>
  </button>
{/snippet}

<aside class="binder">
  {#if app.tree}
    {@const tree = app.tree}
    {#if tree.book}
      {@const book = tree.book}
      <section>
        <h3>Book</h3>
        <button class="item" class:active={isOpen(book.note.path)} onclick={() => openNote(book.note.path)}>
          Overview
        </button>
        {#if book.flowNote}
          {@const flow = book.flowNote}
          <button class="item" class:active={isOpen(flow.path)} onclick={() => openNote(flow.path)}>Story flow</button>
        {/if}
      </section>

      <section>
        <h3>
          Contents
          <button class="add" title="Add a scene at the end, as a chapter of its own" onclick={() => start(sceneKey("end"))}>+</button>
        </h3>
        {#each book.parts as part, p (p)}
          {#if part.title}
            <h4 class="part">{part.title}</h4>
          {/if}
          {#each part.chapters as chapter (chapter.index)}
            <div class="chapter">
              <span class="title">{chapter.heading}</span>
              <span class="count">{formatNumber(chapter.words)}</span>
              <button
                class="add small"
                title="Add a scene to {chapter.heading}"
                onclick={() => start(sceneKey({ chapter: chapter.index }))}>+</button
              >
            </div>
            {#each chapter.scenes as s (s.path)}
              {@render scene(s)}
            {:else}
              <p class="empty nested">No scenes yet</p>
            {/each}
            {@render addForm(sceneKey({ chapter: chapter.index }), "Scene title")}
          {/each}
        {/each}
        {@render addForm("end", "Scene title")}
        {#if !book.parts.some((part) => part.chapters.length)}
          <p class="empty">No scenes yet</p>
        {/if}
      </section>

      <section>
        <h3>
          Unplaced
          <button class="add" title="Add a scene outside the contents, to place later" onclick={() => start("unplaced")}>+</button>
        </h3>
        {@render addForm("unplaced", "Scene title")}
        {#each book.unplaced as s (s.path)}
          {@render scene(s)}
        {:else}
          <p class="empty">Scenes not in the contents yet</p>
        {/each}
      </section>
    {/if}

    <section>
      <h3>World bible</h3>
      <button class="item" class:active={isOpen(tree.projectNote.path)} onclick={() => openNote(tree.projectNote.path)}>
        {tree.project.title}
      </button>
      {#if tree.timeline}
        {@const timeline = tree.timeline}
        <button class="item" class:active={isOpen(timeline.path)} onclick={() => openNote(timeline.path)}>Timeline</button>
      {/if}
    </section>

    {#each bible as group (group.kind)}
      <section>
        <h3>
          {group.label}
          <button class="add" title="Add to {group.label.toLowerCase()}" onclick={() => start(group.kind)}>+</button>
        </h3>
        {@render addForm(group.kind, placeholders[group.kind])}
        {#each group.notes() as note (note.path)}
          <button class="item" class:active={isOpen(note.path)} onclick={() => openNote(note.path)}>
            <span class="title">{note.title}</span>
          </button>
        {:else}
          <p class="empty">None yet</p>
        {/each}
      </section>
    {/each}

    {#each notebooks as notebook (notebook.label)}
      {@const key = `research:${notebook.inProject ? "project" : "vault"}`}
      {@const research = notebook.inProject ? tree.research : tree.vaultResearch}
      {@const scraps = notebook.inProject ? tree.scratchpad : tree.vaultScratchpad}
      <section>
        <h3 title="What's true in the real world, for {notebook.hint}">
          {notebook.label}
          <button class="add" title="Add a research note" onclick={() => start(key)}>+</button>
        </h3>
        {@render addForm(key, "What it's about")}
        {#each research as note (note.path)}
          <button class="item" class:active={isOpen(note.path)} onclick={() => openNote(note.path)}>
            <span class="title">{note.title}</span>
          </button>
        {:else}
          <p class="empty">None yet</p>
        {/each}
      </section>
      <section>
        <h3 title="Fragments kept until they're useful, for {notebook.hint}">
          {notebook.scratchpad}
          <button class="add" title="Keep a new fragment" onclick={() => addScrap(notebook.inProject)}>+</button>
        </h3>
        {#if !notebook.inProject}
          <button class="item" class:active={app.view === "ideas"} onclick={() => (app.view = "ideas")}>Ideas</button>
        {/if}
        {#each scraps as s (s.path)}
          {@render scrap(s)}
        {:else}
          {#if notebook.inProject}
            <p class="empty">None yet</p>
          {/if}
        {/each}
      </section>
    {/each}
  {/if}
</aside>

<style>
  .binder {
    grid-area: binder;
    overflow-y: auto;
    padding: 0.75rem 0.5rem 2rem;
    border-right: 1px solid var(--line);
    background: var(--bg);
  }

  section {
    margin-bottom: 1.1rem;
  }

  h3 {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font: 600 11px/1 var(--sans);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
    padding: 0 0.5rem;
    height: 24px;
  }

  h4.part {
    font: 600 12px/1 var(--sans);
    color: var(--text-soft);
    padding: 0.7rem 0.5rem 0.3rem;
  }

  .add {
    border: none;
    background: none;
    color: var(--muted);
    font-size: 16px;
    line-height: 1;
    padding: 2px 6px;
    border-radius: 4px;
  }

  .add:hover {
    background: var(--panel);
    color: var(--text);
  }

  .chapter {
    position: relative;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 6px 0.5rem 2px;
    font-size: 12px;
    color: var(--muted);
  }

  /* The add button takes the word count's place while the pointer is over the chapter. */
  .chapter .add.small {
    position: absolute;
    right: 2px;
    bottom: 0;
    font-size: 14px;
    padding: 0 6px;
    opacity: 0;
  }

  .chapter:hover .add.small,
  .chapter .add.small:focus-visible {
    opacity: 1;
  }

  .chapter:hover .count,
  .chapter:focus-within .count {
    visibility: hidden;
  }

  .item {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    width: 100%;
    border: none;
    background: none;
    text-align: left;
    padding: 5px 0.5rem;
    border-radius: 6px;
  }

  .item.scene {
    padding-left: 1.1rem;
  }

  .item:hover {
    background: var(--panel);
  }

  .item.active {
    background: var(--accent-soft);
  }

  .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .count {
    color: var(--muted);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }

  .empty {
    margin: 0;
    padding: 3px 0.5rem;
    color: var(--muted);
    font-size: 13px;
    font-style: italic;
  }

  .empty.nested {
    padding-left: 1.1rem;
  }

  form {
    padding: 2px 0.25rem 4px;
  }
</style>
