<script lang="ts">
  import { app, chooseVault, upgradeVault } from "../lib/store.svelte";

  let upgrading = $state(false);

  async function upgrade() {
    upgrading = true;
    await upgradeVault();
    upgrading = false;
  }
</script>

<div class="welcome">
  <div class="column">
    {#if app.vault?.needsUpgrade}
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
        <button class="btn" disabled={upgrading} onclick={chooseVault}>Choose another folder</button>
      </div>
    {:else}
      <h1>Story Time</h1>
      <p class="lead">A quiet place to write.</p>
      <p class="muted">
        Your stories are kept on a bookshelf: a folder of plain Markdown files, on OneDrive, in a git repository,
        anywhere. Nothing is locked in; Obsidian and other editors can open the same folder.
      </p>
      <button class="btn primary" onclick={chooseVault}>Choose your bookshelf</button>
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

  .column {
    display: grid;
    justify-items: start;
    gap: 0.9rem;
    max-width: 34rem;
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

  .btn {
    margin-top: 0.8rem;
    padding: 8px 16px;
  }

  .actions {
    display: flex;
    gap: 0.6rem;
  }
</style>
