<script lang="ts">
  import { onMount } from "svelte";
  import Binder from "./components/Binder.svelte";
  import Board from "./components/Board.svelte";
  import Check from "./components/Check.svelte";
  import Editor from "./components/Editor.svelte";
  import Ideas from "./components/Ideas.svelte";
  import Inspector from "./components/Inspector.svelte";
  import NewProject from "./components/NewProject.svelte";
  import Progress from "./components/Progress.svelte";
  import Settings from "./components/Settings.svelte";
  import StatusBar from "./components/StatusBar.svelte";
  import Toasts from "./components/Toasts.svelte";
  import TopBar from "./components/TopBar.svelte";
  import Welcome from "./components/Welcome.svelte";
  import { app, init, openNewProject } from "./lib/store.svelte";

  onMount(() => void init());

  const layout = $derived(
    app.focus ? "focus" : app.view === "write" && app.slug ? "with-inspector" : "without-inspector",
  );
</script>

{#if !app.ready}
  <div class="loading"></div>
{:else if !app.vault || app.vault.needsUpgrade || app.pendingFolder || app.starting}
  <Welcome />
{:else}
  <div class="shell {layout}">
    {#if !app.focus}
      <TopBar />
      <Binder />
    {/if}
    <main>
      {#if app.view === "ideas"}
        <Ideas />
      {:else if app.view === "settings"}
        <Settings />
      {:else if !app.slug}
        <div class="start">
          <h1>An empty bookshelf</h1>
          <p class="muted">Start a book and begin on its first scene, or jot down an idea to come back to.</p>
          <div class="actions">
            <button class="btn primary" onclick={() => openNewProject()}>Start your first book</button>
            <button class="btn" onclick={() => (app.view = "ideas")}>Jot down an idea</button>
          </div>
        </div>
      {:else}
        <!-- The editor stays mounted so undo history and scroll position survive view changes. -->
        <div class="pane" hidden={app.view !== "write"}><Editor /></div>
        {#if app.view === "board"}
          <Board />
        {:else if app.view === "progress"}
          <Progress />
        {:else if app.view === "check"}
          <Check />
        {/if}
      {/if}
    </main>
    {#if layout === "with-inspector"}
      <Inspector />
    {/if}
    <StatusBar />
  </div>
  {#if app.newProject}
    <NewProject />
  {/if}
{/if}
<Toasts />

<style>
  .shell {
    display: grid;
    height: 100%;
    grid-template-rows: 46px minmax(0, 1fr) 30px;
  }

  .shell.with-inspector {
    grid-template-columns: 250px minmax(0, 1fr) 290px;
    grid-template-areas:
      "top top top"
      "binder main inspector"
      "status status status";
  }

  .shell.without-inspector {
    grid-template-columns: 250px minmax(0, 1fr);
    grid-template-areas:
      "top top"
      "binder main"
      "status status";
  }

  .shell.focus {
    --editor-size: 21px;
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: minmax(0, 1fr) 30px;
    grid-template-areas: "main" "status";
  }

  main {
    grid-area: main;
    min-height: 0;
    background: var(--paper);
    overflow: hidden;
    position: relative;
  }

  .pane {
    height: 100%;
  }

  .pane[hidden] {
    display: none;
  }

  .start {
    max-width: 30rem;
    margin: 18vh auto 0;
    padding: 0 2rem;
    display: grid;
    gap: 0.8rem;
  }

  .start p {
    margin: 0;
  }

  .actions {
    display: flex;
    gap: 0.5rem;
    margin-top: 0.5rem;
  }

  .loading {
    height: 100%;
    background: var(--paper);
  }
</style>
