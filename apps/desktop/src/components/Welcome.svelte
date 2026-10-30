<script lang="ts">
  import { chooseVault } from "../lib/store.svelte";
</script>

<div class="welcome">
  <div class="column">
    <h1>Story Time</h1>
    <p class="lead">A quiet place to write.</p>
    <p class="muted">
      Your stories are kept as plain Markdown files in a folder you choose: on OneDrive, in a git repository,
      anywhere. Nothing is locked in; Obsidian and other editors can open the same folder.
    </p>
    <button class="btn primary" onclick={chooseVault}>Choose your stories folder</button>
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
</style>
