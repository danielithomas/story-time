<script lang="ts">
  import { api } from "../lib/api";
  import { app, chooseBookshelfPlace, chooseVault, setUpPendingFolder, startBookshelf, upgradeVault } from "../lib/store.svelte";

  let upgrading = $state(false);
  let name = $state("Stories");
  let place = $state("");
  let example = $state(true);
  let busy = $state(false);
  let error = $state("");

  $effect(() => {
    if (app.starting && !place) api.defaultBookshelfPlace().then((found) => (place = found), () => {});
  });

  async function upgrade() {
    upgrading = true;
    await upgradeVault();
    upgrading = false;
  }

  async function changePlace() {
    place = (await chooseBookshelfPlace(place)) ?? place;
  }

  async function start(event: SubmitEvent) {
    event.preventDefault();
    busy = true;
    error = "";
    try {
      await startBookshelf(place, name, example);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  /** What goes before item `i` of `n` in a list: nothing, a comma, or "and". */
  const separator = (i: number, n: number) => (i === 0 ? "" : i === n - 1 ? " and " : ", ");

  function select(node: HTMLInputElement) {
    node.select();
  }
</script>

<div class="welcome">
  <div class="column">
    {#if app.pendingFolder}
      {@const folder = app.pendingFolder}
      <h1>Set up {folder.name}?</h1>
      <p class="lead">{folder.name} already holds other files.</p>
      <p class="muted">
        To use it as a bookshelf, Story Time adds
        {#if folder.adds.length}
          {#each folder.adds as add, i (add)}{separator(i, folder.adds.length)}<code>{add}</code>{/each}
        {:else}
          its settings
        {/if}
        to it. Everything already there stays as it is. If this isn't the folder you meant, cancel and choose another.
      </p>
      <div class="actions">
        <button class="btn primary" onclick={setUpPendingFolder}>Set it up</button>
        <button class="btn" onclick={() => (app.pendingFolder = null)}>Cancel</button>
      </div>
    {:else if app.vault?.needsUpgrade}
      <h1>Upgrade {app.vault.name}</h1>
      <p class="lead">This bookshelf is laid out the way story-time 0.1 did it.</p>
      <p class="muted">
        Story Time can rearrange it for this version: lower-case file names, and in each project, its
        characters, places and world notes in a world bible and its chapters as the scenes of a book. Your
        writing stays as it is. The whole bookshelf is copied into <code>.story-time/backups</code> first.
      </p>
      <div class="actions">
        <button class="btn primary" disabled={upgrading} onclick={upgrade}>
          {upgrading ? "Upgrading…" : "Upgrade"}
        </button>
        <button class="btn" disabled={upgrading} onclick={chooseVault}>Open another bookshelf</button>
      </div>
    {:else if app.starting}
      <form onsubmit={start}>
        <h1>Start a bookshelf</h1>
        <p class="lead">A folder for your books, their worlds and your ideas.</p>
        <label class="field">
          <span>Name</span>
          <input bind:value={name} use:select required />
        </label>
        <div class="field">
          <span>Where</span>
          <div class="place">
            <code title={place}>{place || "…"}</code>
            <button type="button" class="btn" onclick={changePlace}>Change…</button>
          </div>
        </div>
        <p class="hint">Put it in a OneDrive, Dropbox or iCloud folder to have it on all your computers.</p>
        <label class="check">
          <input type="checkbox" bind:checked={example} />
          <span>Add an example book to look around in. Its scenes explain how Story Time works; delete it when you're done.</span>
        </label>
        {#if error}
          <p class="error">{error}</p>
        {/if}
        <div class="actions">
          <button class="btn primary" disabled={busy || !name.trim() || !place}>Start</button>
          <button type="button" class="btn" onclick={() => (app.starting = false)}>Back</button>
        </div>
      </form>
    {:else}
      <h1>Story Time</h1>
      <p class="lead">A quiet place to write.</p>
      <p class="muted">
        Your stories are kept on a bookshelf: a folder of plain Markdown files, on OneDrive, in a git repository,
        anywhere. Nothing is locked in; Obsidian and other editors can open the same folder.
      </p>
      <div class="actions">
        <button class="btn primary" onclick={() => (app.starting = true)}>Start a bookshelf</button>
        <button class="btn" onclick={chooseVault}>Open a bookshelf</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .welcome {
    height: 100%;
    display: grid;
    place-items: center;
    padding: 2rem;
    background: var(--paper);
  }

  .column,
  form {
    display: grid;
    justify-items: start;
    gap: 0.9rem;
    max-width: 34rem;
  }

  form {
    justify-items: stretch;
    width: min(34rem, 100%);
  }

  h1 {
    font-size: 40px;
  }

  p {
    margin: 0;
    line-height: 1.6;
  }

  .lead {
    font-family: var(--serif);
    font-size: 20px;
    color: var(--text-soft);
  }

  .place {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }

  .place code {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .hint {
    margin-top: -0.4rem;
    font-size: 12px;
    color: var(--muted);
  }

  .check {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    font-size: 14px;
  }

  .error {
    color: var(--danger);
  }

  .actions {
    display: flex;
    gap: 0.6rem;
    margin-top: 0.8rem;
  }

  .actions .btn {
    padding: 8px 16px;
  }
</style>
