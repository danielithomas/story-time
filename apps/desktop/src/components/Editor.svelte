<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { createEditor, type ProseEditor } from "../lib/editor";
  import { app, edited, keepMine, takeTheirs } from "../lib/store.svelte";

  let host: HTMLDivElement;
  let editor: ProseEditor | undefined;

  onMount(() => {
    editor = createEditor(host, edited);
    return () => editor?.destroy();
  });

  // Load the note whenever a different one is opened (or reloaded from disk).
  $effect(() => {
    void app.docVersion;
    untrack(() => {
      editor?.load(app.doc?.body ?? "");
      if (app.view === "write") void tick().then(() => editor?.focus());
    });
  });

  const kindLabels: Record<string, string> = {
    project: "Overview",
    flow: "Story flow",
    character: "Characters",
    location: "Locations",
    world: "World",
  };

  const trail = $derived.by(() => {
    const doc = app.doc;
    if (!doc) return "";
    const section = doc.kind === "scene" ? app.tree?.book?.summary.title : kindLabels[doc.kind];
    const parts = [app.tree?.project.title, section];
    if (doc.kind !== "project" && doc.kind !== "flow") parts.push(doc.title);
    return parts.filter(Boolean).join("  ›  ");
  });
</script>

<section class="writing" class:focus={app.focus}>
  {#if app.saveState === "conflict"}
    <div class="conflict" role="alert">
      <span>This note was changed outside Story Time (in Obsidian or by sync) since you opened it.</span>
      <button class="btn" onclick={takeTheirs}>Use the other version</button>
      <button class="btn primary" onclick={keepMine}>Keep mine</button>
    </div>
  {/if}
  {#if !app.focus}
    <div class="trail">{trail}</div>
  {/if}
  <div class="editor" bind:this={host}></div>
</section>

<style>
  .writing {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .trail {
    flex: none;
    padding: 10px 1.5rem 0;
    font-size: 12px;
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .editor {
    flex: 1;
    min-height: 0;
  }

  .conflict {
    flex: none;
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 8px 1.5rem;
    background: var(--danger-soft);
    border-bottom: 1px solid var(--line);
  }

  .conflict span {
    flex: 1;
  }
</style>
