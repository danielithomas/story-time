<script lang="ts">
  import { api } from "../lib/api";
  import { addNote, allNotes, app, openNote, toastError } from "../lib/store.svelte";
  import type { AddableKind, CheckReport } from "../lib/types";

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

  async function create(kind: AddableKind, name: string) {
    await addNote(kind, name);
    await run();
  }

  const clean = $derived(
    !!report &&
      !report.undocumented.length &&
      !report.notOnPage.length &&
      !report.withoutStatus.length &&
      !report.withoutSynopsis.length &&
      !report.emptyChapters.length &&
      !report.unreadable.length,
  );
</script>

{#snippet links(names: string[])}
  {#each names as name, i (name)}
    {@const note = noteNamed(name)}
    {#if note}
      <button class="link" onclick={() => openNote(note.path)}>{name}</button>{:else}{name}{/if}{#if i < names.length - 1},&nbsp;{/if}
  {/each}
{/snippet}

<div class="view">
  <header>
    <h1>Story bible check</h1>
    <p class="muted">Gaps between what the chapters mention and what the notes document.</p>
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

    {#if report.notOnPage.length}
      <section>
        <h2>Documented, but not on the page yet</h2>
        <p class="muted">No chapter mentions the name, title or any alias.</p>
        <ul>
          {#each report.notOnPage as item (item.name)}
            <li><span class="kind">{item.kind}</span> {@render links([item.name])}</li>
          {/each}
        </ul>
      </section>
    {/if}

    {#each [
      { title: "Chapters without a status", names: report.withoutStatus },
      { title: "Chapters without a synopsis", names: report.withoutSynopsis },
      { title: "Empty chapters", names: report.emptyChapters },
    ] as group (group.title)}
      {#if group.names.length}
        <section>
          <h2>{group.title}</h2>
          <p>{@render links(group.names)}</p>
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
