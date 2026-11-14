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
