<script lang="ts">
  import {
    app,
    chooseVault,
    compile,
    openNewBook,
    openNewProject,
    selectBook,
    selectProject,
    toggleFocus,
    VIEWS,
  } from "../lib/store.svelte";

  let exportMenu = $state<HTMLDetailsElement>();
  let newMenu = $state<HTMLDetailsElement>();

  function exportAs(format: string) {
    if (exportMenu) exportMenu.open = false;
    void compile(format);
  }

  function startNew(what: "book" | "project") {
    if (newMenu) newMenu.open = false;
    if (what === "book") openNewBook();
    else openNewProject();
  }
</script>

<header class="top">
  <div class="left">
    <button class="vault" title="{app.vault?.path} (click to change)" onclick={chooseVault}>
      {app.vault?.name}
    </button>
    {#if app.projects.length}
      <span class="slash">/</span>
      <select class="project" value={app.slug} onchange={(e) => selectProject(e.currentTarget.value)}>
        {#each app.projects as project (project.slug)}
          <option value={project.slug}>{project.title}</option>
        {/each}
      </select>
    {/if}
    {#if app.tree && app.tree.project.books.length > 1}
      <span class="slash">/</span>
      <select class="project" value={app.book} onchange={(e) => selectBook(e.currentTarget.value)}>
        {#each app.tree.project.books as book (book.slug)}
          <option value={book.slug}>{book.title}</option>
        {/each}
      </select>
    {/if}
    <details class="menu start" bind:this={newMenu}>
      <summary class="btn ghost">+ New</summary>
      <div class="menu-items">
        {#if app.tree}
          <button onclick={() => startNew("book")}>
            <strong>Book in {app.tree.project.title}</strong>
            <span>Shares this world's characters, places and world notes</span>
          </button>
        {/if}
        <button onclick={() => startNew("project")}>
          <strong>Project</strong>
          <span>A new world, with its first book</span>
        </button>
      </div>
    </details>
  </div>

  <nav>
    {#each VIEWS as view, i (view.key)}
      <button
        class:active={app.view === view.key}
        disabled={view.needsProject && !app.slug}
        title="{view.label} (Ctrl+{i + 1})"
        onclick={() => (app.view = view.key)}
      >
        {view.label}
      </button>
    {/each}
  </nav>

  <div class="right">
    {#if app.slug}
      <details class="menu" bind:this={exportMenu}>
        <summary class="btn">Export</summary>
        <div class="menu-items">
          {#each app.choices?.formats ?? [] as format (format.key)}
            <button onclick={() => exportAs(format.key)}>
              <strong>{format.label}</strong>
              <span>{format.detail}</span>
            </button>
          {/each}
        </div>
      </details>
    {/if}
    <button class="btn" title="Hide everything but the page (F11, Esc to leave)" onclick={() => toggleFocus()}>
      Focus
    </button>
  </div>
</header>

<style>
  .top {
    grid-area: top;
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: 1rem;
    padding: 0 0.75rem;
    border-bottom: 1px solid var(--line);
    background: var(--bg);
  }

  .left,
  .right {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    min-width: 0;
  }

  .right {
    justify-content: flex-end;
  }

  .vault {
    border: none;
    background: none;
    font-weight: 600;
    padding: 4px 6px;
    border-radius: 6px;
  }

  .vault:hover {
    background: var(--panel);
  }

  .slash {
    color: var(--muted);
  }

  .project {
    width: auto;
    max-width: 16rem;
    border-color: transparent;
    background-color: transparent;
    font-family: var(--serif);
    font-size: 15px;
  }

  .project:hover {
    border-color: var(--line);
  }

  nav {
    display: flex;
    gap: 2px;
    padding: 3px;
    border-radius: 8px;
    background: var(--panel);
  }

  nav button {
    border: none;
    background: none;
    padding: 4px 14px;
    border-radius: 6px;
    color: var(--text-soft);
  }

  nav button.active {
    background: var(--paper);
    color: var(--text);
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.08);
  }

  nav button:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .menu {
    position: relative;
  }

  .menu summary {
    list-style: none;
  }

  .menu summary::-webkit-details-marker {
    display: none;
  }

  .menu-items {
    position: absolute;
    right: 0;
    left: auto;
    top: calc(100% + 6px);
    z-index: 10;
    width: 18rem;
    padding: 4px;
    background: var(--paper);
    border: 1px solid var(--line);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.12);
  }

  .menu.start .menu-items {
    right: auto;
    left: 0;
  }

  .menu-items button {
    display: grid;
    width: 100%;
    gap: 1px;
    text-align: left;
    border: none;
    background: none;
    padding: 6px 8px;
    border-radius: 6px;
  }

  .menu-items button:hover {
    background: var(--panel);
  }

  .menu-items span {
    font-size: 12px;
    color: var(--muted);
  }
</style>
