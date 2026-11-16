<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { createEditor, type ProseEditor } from "../lib/editor";
  import { app, compareVersions, decideVersion, edited, keepMine, takeTheirs } from "../lib/store.svelte";
  import Compare from "./Compare.svelte";

  let host: HTMLDivElement;
  let editor: ProseEditor | undefined;

  onMount(() => {
    editor = createEditor(host, edited);
    return () => editor?.destroy();
  });

  // Typewriter scrolling, in focus mode or always, as Settings says.
  $effect(() => {
    editor?.setTypewriter(app.typewriter === "always" || (app.typewriter === "focus" && app.focus));
  });

  // Load the note whenever a different one is opened (or reloaded from disk).
  $effect(() => {
    void app.docVersion;
    untrack(() => {
      editor?.load(app.doc?.body ?? "");
      if (app.view === "write") void tick().then(() => editor?.focus());
    });
  });

  // A note changed here and on another computer before they synced, and its two versions compared.
  const twoVersions = $derived(!!app.doc && app.sync.waiting.some((waiting) => waiting.path === app.doc?.path));
  const comparing = $derived(twoVersions && !!app.compare && app.compare.path === app.doc?.path);
  let choosing = $state(false);

  async function choose(useOther: boolean) {
    choosing = true;
    await decideVersion(useOther);
    choosing = false;
  }

  const kindLabels: Record<string, string> = {
    project: "Overview",
    flow: "Story flow",
    character: "Characters",
    location: "Locations",
    world: "World",
    timeline: "Timeline",
    research: "Research",
    scrap: "Scratchpad",
  };

  const trail = $derived.by(() => {
    const doc = app.doc;
    if (!doc) return "";
    const section = doc.kind === "scene" ? app.tree?.book?.summary.title : kindLabels[doc.kind];
    // Notes outside any project, such as the general scratchpad, have no world to name.
    const parts = [doc.project ? app.tree?.project.title : null, section];
    if (!["project", "flow", "timeline"].includes(doc.kind)) parts.push(doc.title);
    return parts.filter(Boolean).join("  ›  ");
  });
</script>

<section class="writing" class:focus={app.focus}>
  {#if app.saveState === "conflict"}
    <div class="conflict" role="alert">
      <span>This note was changed outside Story Time (by a sync service, say, or on another computer) since you opened it.</span>
      <button class="btn" onclick={takeTheirs}>Use the other version</button>
      <button class="btn primary" onclick={keepMine}>Keep mine</button>
    </div>
  {/if}
  {#if comparing}
    <Compare />
  {:else}
    {#if twoVersions && app.saveState !== "conflict"}
      <div class="conflict" role="alert">
        <span>
          This note was changed here and on another computer before they synced. Story Time kept this computer's
          version.
        </span>
        <button class="btn" onclick={compareVersions}>Compare the two</button>
        <button class="btn" disabled={choosing} onclick={() => choose(true)}>Use the other version</button>
        <button class="btn primary" disabled={choosing} onclick={() => choose(false)}>Keep this one</button>
      </div>
    {/if}
    {#if !app.focus}
      <div class="trail">{trail}</div>
    {/if}
  {/if}
  <div class="editor" class:hidden={comparing} bind:this={host}></div>
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

  .editor.hidden {
    display: none;
  }
</style>
