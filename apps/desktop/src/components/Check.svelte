<script lang="ts">
  import { api } from "../lib/api";
  import { addNote, allNotes, app, openNote, renameNote, toastError } from "../lib/store.svelte";
  import type { BibleKind, CheckReport } from "../lib/types";

  let report = $state<CheckReport | null>(null);

  async function run() {
    if (!app.slug) return;
    try {
      report = await api.checkProject(app.slug);
    } catch (error) {
      toastError(error);
    }
  }

  $effect(() => {
    void app.slug;
    void run();
  });

  /** Opens a note by name, if the project has one. */
  function noteNamed(name: string) {
    return app.tree ? allNotes(app.tree).find((note) => note.name === name) : undefined;
  }

  async function create(kind: BibleKind, name: string) {
    await addNote(kind, name);
    await run();
  }

  /** Gives a note's file the name it should have, updating every link to it. */
  async function fixName(path: string) {
    if (await renameNote(path, null)) await run();
  }

  const clean = $derived(
    !!report &&
      !report.undocumented.length &&
      !report.ambiguousLinks.length &&
      !report.aliasLinks.length &&
      !report.brokenEntries.length &&
      !report.repeated.length &&
      !report.notOnPage.length &&
      !report.misnamed.length &&
      !report.drifted.length &&
      !report.withoutStatus.length &&
      !report.withoutSynopsis.length &&
      !report.emptyScenes.length &&
      !report.unreadable.length,
  );
</script>

{#snippet links(names: string[])}
  {#each names as name, i (name)}
    {@const note = noteNamed(name)}
    {#if note}
      <button class="link" onclick={() => openNote(note.path)}>{note.title}</button>{:else}{name}{/if}{#if i < names.length - 1},&nbsp;{/if}
  {/each}
{/snippet}

<div class="view">
  <header>
    <h1>World bible check</h1>
    <p class="muted">Gaps between what this world's scenes mention and what its notes document, and contents that don't match the scenes.</p>
    <button class="btn" onclick={run}>Check again</button>
  </header>

  {#if report}
    {#if clean}
      <p class="clean">Nothing to report.</p>
    {/if}

    {#if report.undocumented.length}
      <section>
        <h2>Linked, but no note yet</h2>
        <p class="muted">Names in <code>[[links]]</code> with no note by that name. Create one to start its entry.</p>
        <ul>
          {#each report.undocumented as item (item.name)}
            <li>
              <strong>{item.name}</strong>
              <span class="muted">in {@render links(item.sources)}</span>
              <span class="actions">
                <button class="btn" onclick={() => create("character", item.name)}>+ Character</button>
                <button class="btn" onclick={() => create("location", item.name)}>+ Location</button>
                <button class="btn" onclick={() => create("world", item.name)}>+ World</button>
              </span>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#each [
      {
        title: "Links that could mean more than one note",
        about: "Several notes share the file name, so Obsidian may open the wrong one.",
        items: report.ambiguousLinks,
      },
      {
        title: "Links Obsidian can't follow",
        about: "They name a note by its name rather than its file name. Story Time follows them; Obsidian doesn't.",
        items: report.aliasLinks,
      },
      {
        title: "Listed, but not a book or scene here",
        about: "Entries in a world's list of books or a book's contents that don't link to one of its books or scenes.",
        items: report.brokenEntries,
      },
    ] as group (group.title)}
      {#if group.items.length}
        <section>
          <h2>{group.title}</h2>
          <p class="muted">{group.about}</p>
          <ul>
            {#each group.items as item (item.name)}
              <li>
                <code>[[{item.name}]]</code>
                <span class="muted">in {@render links(item.sources)}</span>
              </li>
            {/each}
          </ul>
        </section>
      {/if}
    {/each}

    {#if report.notOnPage.length}
      <section>
        <h2>Documented, but not on the page yet</h2>
        <p class="muted">No scene mentions the name, title or any alias.</p>
        <ul>
          {#each report.notOnPage as item (item.name)}
            <li><span class="kind">{item.kind}</span> {@render links([item.name])}</li>
          {/each}
        </ul>
      </section>
    {/if}

    {#each [
      { title: "Listed more than once in the contents", names: report.repeated },
      { title: "Scenes without a status", names: report.withoutStatus },
      { title: "Scenes without a synopsis", names: report.withoutSynopsis },
      { title: "Empty scenes", names: report.emptyScenes },
    ] as group (group.title)}
      {#if group.names.length}
        <section>
          <h2>{group.title}</h2>
          <p>{@render links(group.names)}</p>
        </section>
      {/if}
    {/each}

    {#each [
      { title: "Names that aren't kebab-case", items: report.misnamed },
      { title: "File names that no longer match the note's name", items: report.drifted },
    ] as group (group.title)}
      {#if group.items.length}
        <section>
          <h2>{group.title}</h2>
          <ul>
            {#each group.items as item (item.path)}
              <li>
                <strong>{item.path}</strong> <span class="muted">rename to {item.to}</span>
                {#if item.note}
                  {@const note = item.note}
                  <span class="actions"><button class="btn" onclick={() => fixName(note)}>Rename</button></span>
                {/if}
              </li>
            {/each}
          </ul>
        </section>
      {/if}
    {/each}

    {#if report.unreadable.length}
      <section>
        <h2>Properties that couldn't be read</h2>
        <ul>
          {#each report.unreadable as item (item.path)}
            <li><strong>{item.path}</strong> <span class="muted">{item.error}</span></li>
          {/each}
        </ul>
      </section>
    {/if}
  {/if}
</div>

<style>
  .view {
    height: 100%;
    overflow-y: auto;
    padding: 2rem 2.5rem 4rem;
    display: grid;
    gap: 1.8rem;
    align-content: start;
  }

  .view > * {
    max-width: 900px;
  }

  header {
    display: grid;
    gap: 0.4rem;
    justify-items: start;
  }

  h1 {
    font-size: 26px;
  }

  h2 {
    font-size: 16px;
    margin-bottom: 0.3rem;
  }

  p {
    margin: 0;
  }

  ul {
    list-style: none;
    padding: 0;
    margin: 0.6rem 0 0;
    display: grid;
    gap: 0.4rem;
  }

  li {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.6rem;
  }

  .actions {
    display: flex;
    gap: 0.3rem;
    margin-left: auto;
  }

  .actions .btn {
    font-size: 12px;
    padding: 2px 8px;
  }

  .kind {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
    width: 5.5rem;
  }

  .link {
    border: none;
    background: none;
    padding: 0;
    color: var(--accent);
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .clean {
    font-family: var(--serif);
    font-size: 18px;
  }

  code {
    font-family: var(--mono);
    font-size: 12px;
  }
</style>
