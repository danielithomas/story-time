<script lang="ts">
  import { chapterNumber, formatNumber } from "../lib/format";
  import { addNote, app, openNote } from "../lib/store.svelte";
  import type { AddableKind, NoteRef } from "../lib/types";

  const bible: { kind: AddableKind; label: string; notes: () => NoteRef[] }[] = [
    { kind: "character", label: "Characters", notes: () => app.tree?.characters ?? [] },
    { kind: "location", label: "Locations", notes: () => app.tree?.locations ?? [] },
    { kind: "world", label: "World notes", notes: () => app.tree?.world ?? [] },
  ];

  const placeholders: Record<AddableKind, string> = {
    chapter: "Chapter title",
    character: "Character's name",
    location: "Place name",
    world: "What it is",
  };

  let adding = $state<AddableKind | null>(null);
  let name = $state("");

  function start(kind: AddableKind) {
    adding = kind;
    name = "";
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    const kind = adding;
    const value = name.trim();
    adding = null;
    if (kind && value) await addNote(kind, value);
  }

  function cancelOnEscape(event: KeyboardEvent) {
    if (event.key === "Escape") adding = null;
  }

  function focus(node: HTMLInputElement) {
    node.focus();
  }

  const isOpen = (path: string) => app.view === "write" && app.doc?.path === path;
</script>

{#snippet addForm(kind: AddableKind)}
  {#if adding === kind}
    <form onsubmit={submit}>
      <input
        bind:value={name}
        placeholder={placeholders[kind]}
        use:focus
        onkeydown={cancelOnEscape}
        onblur={() => (adding = null)}
      />
    </form>
  {/if}
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
          Chapters
          <button class="add" title="Add a chapter" onclick={() => start("chapter")}>+</button>
        </h3>
        {@render addForm("chapter")}
        {#each book.chapters as chapter (chapter.path)}
          <button
            class="item chapter"
            class:active={isOpen(chapter.path)}
            title={chapter.synopsis ?? ""}
            onclick={() => openNote(chapter.path)}
          >
            <span class="dot" data-status={chapter.status ?? ""}></span>
            <span class="number">{chapterNumber(chapter.number)}</span>
            <span class="title">{chapter.title}</span>
            <span class="count">{formatNumber(chapter.words)}</span>
          </button>
        {:else}
          <p class="empty">No chapters yet</p>
        {/each}
      </section>
    {/if}

    <section>
      <h3>World bible</h3>
      <button class="item" class:active={isOpen(tree.projectNote.path)} onclick={() => openNote(tree.projectNote.path)}>
        {tree.project.title}
      </button>
    </section>

    {#each bible as group (group.kind)}
      <section>
        <h3>
          {group.label}
          <button class="add" title="Add to {group.label.toLowerCase()}" onclick={() => start(group.kind)}>+</button>
        </h3>
        {@render addForm(group.kind)}
        {#each group.notes() as note (note.path)}
          <button class="item" class:active={isOpen(note.path)} onclick={() => openNote(note.path)}>
            <span class="title">{note.title}</span>
          </button>
        {:else}
          <p class="empty">None yet</p>
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

  .number,
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

  form {
    padding: 2px 0.25rem 4px;
  }
</style>
