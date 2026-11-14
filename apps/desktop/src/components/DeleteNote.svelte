<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { plural } from "../lib/format";
  import { app, deleteNote } from "../lib/store.svelte";
  import type { NoteDoc, NoteLinks } from "../lib/types";

  let { doc, close }: { doc: NoteDoc; close: () => void } = $props();

  let links = $state<NoteLinks | null>(null);
  let busy = $state(false);

  onMount(() => {
    api.linksToNote(doc.path).then(
      (found) => (links = found),
      () => {},
    );
  });

  const book = $derived(doc.kind === "scene" && doc.book === app.book ? app.tree?.book?.summary.title : null);

  async function confirm() {
    busy = true;
    await deleteNote(doc.path);
    close();
  }

  function focus(node: HTMLElement) {
    node.focus();
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && !busy && close()} />

<div class="backdrop">
  <div class="dialog" role="alertdialog" aria-modal="true" aria-labelledby="delete-title" aria-describedby="delete-what">
    <h2 id="delete-title">Delete {doc.title}?</h2>
    <p id="delete-what">
      It goes to the system trash, where you can restore it.{#if book}&nbsp;It also comes out of {book}'s contents.{/if}
    </p>
    {#if links?.links}
      <p class="muted">
        {plural(links.links, "link")} in {plural(links.notes, "note")}
        {links.links === 1 ? "leads" : "lead"} to it, and will lead nowhere unless it's restored.
      </p>
    {/if}
    <div class="actions">
      <button class="btn" onclick={close} disabled={busy} use:focus>Cancel</button>
      <button class="btn danger" onclick={confirm} disabled={busy}>Move to trash</button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 20;
    display: grid;
    place-items: center;
    background: rgb(0 0 0 / 0.25);
  }

  .dialog {
    width: min(26rem, 92vw);
    padding: 1.4rem 1.5rem;
    background: var(--paper);
    border: 1px solid var(--line);
    border-radius: 12px;
    box-shadow: 0 20px 50px rgb(0 0 0 / 0.2);
  }

  h2 {
    font-size: 18px;
    margin-bottom: 0.8rem;
  }

  p {
    margin: 0 0 0.7rem;
    line-height: 1.5;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 1.2rem;
  }
</style>
