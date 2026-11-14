<script lang="ts">
  import { capitalise, formatNumber, plainLinks, plural } from "../lib/format";
  import { app, bookScenes, cutScene, linkFrom, renameNote, setProperty, toastError, useScrap } from "../lib/store.svelte";
  import type { NoteRef } from "../lib/types";
  import DeleteFolder from "./DeleteFolder.svelte";
  import DeleteNote from "./DeleteNote.svelte";

  const doc = $derived(app.doc);

  // An empty date input shows today's date as a placeholder in WebKit, which reads as a real
  // deadline, so an unset deadline is a button until the writer chooses to set one.
  let settingDeadline = $state(false);
  const choices = $derived(app.choices);

  const headings: Record<string, string> = {
    project: "World",
    book: "Book",
    flow: "Story flow",
    scene: "Scene",
    character: "Character",
    location: "Location",
    world: "World",
    timeline: "Timeline",
    research: "Research",
    scrap: "Scrap",
    note: "Note",
  };

  function text(key: string): string {
    const value = doc?.properties[key];
    return Array.isArray(value) ? value.join(", ") : (value ?? "");
  }

  /** The choices for a select, keeping a value typed into the note by hand that isn't one of them. */
  function options(list: string[], current: string): string[] {
    return current && !list.includes(current) ? ["", ...list, current] : ["", ...list];
  }

  function setList(key: string, value: string) {
    setProperty(
      key,
      value
        .split(",")
        .map((item) => item.trim())
        .filter(Boolean),
    );
  }

  /** A value naming a note (a point of view, a location) is stored as a link to it, so Obsidian can follow it. */
  async function setLink(key: string, value: string, notes: NoteRef[]) {
    const name = value.trim();
    const note = notes.find(
      (n) => n.name.toLowerCase() === name.toLowerCase() || n.title.toLowerCase() === name.toLowerCase(),
    );
    const from = doc?.path;
    if (!note || !from) return setProperty(key, name);
    try {
      const link = await linkFrom(from, note.path);
      if (app.doc?.path === from) setProperty(key, link);
    } catch (error) {
      toastError(error);
    }
  }

  /** Kinds of note that can be deleted from here; a book's and world's own notes can't. */
  const DELETABLE = ["scene", "character", "location", "world", "research", "scrap"];
  let deleting = $state(false);
  /** Deleting the open book or world, from its own note. */
  let deletingFolder = $state<"book" | "project" | null>(null);

  /** A scrap that can be used in the open book: one from this world's scratchpad, or the bookshelf's. */
  const canUse = $derived(
    doc?.kind === "scrap" && !!app.tree?.book && (doc.project === app.slug || doc.project === null),
  );
  /** A scene that was cut, which goes back into a book as it was. */
  const isCutScene = $derived(doc?.properties.type === "scene");
  /** The name for a scrap about to become a scene, while it's being asked for. */
  let sceneName = $state<string | null>(null);

  // Moving to another note closes the form.
  $effect(() => {
    void doc?.path;
    sceneName = null;
  });

  function askSceneName() {
    const aliases = doc?.properties.aliases;
    sceneName = (Array.isArray(aliases) ? aliases[0] : aliases) ?? "";
  }

  async function useAsScene(event: SubmitEvent) {
    event.preventDefault();
    const name = sceneName?.trim();
    if (!doc || !name) return;
    sceneName = null;
    await useScrap(doc.path, name);
  }

  /** Records that the scrap led to a scene, with a link to it in `used`. */
  async function markUsed(select: HTMLSelectElement) {
    const scene = bookScenes(app.tree).find((s) => s.path === select.value);
    select.value = "";
    const from = doc?.path;
    if (!scene || !from) return;
    try {
      const link = await linkFrom(from, scene.path);
      const used = doc?.properties.used;
      const list = Array.isArray(used) ? used : used ? [used] : [];
      if (app.doc?.path === from && !list.includes(link)) setProperty("used", [...list, link]);
    } catch (error) {
      toastError(error);
    }
  }

  /** The note's other names: its aliases after the first, which is its name. */
  const otherNames = $derived.by(() => {
    const aliases = doc?.properties.aliases;
    return (Array.isArray(aliases) ? aliases : aliases ? [aliases] : []).slice(1).join(", ");
  });

  /** Renames the note, its file and every link to it. A refused name goes back to the note's own. */
  async function rename(input: HTMLInputElement) {
    if (!doc) return;
    const name = input.value.trim();
    if (!name || name === doc.title || !(await renameNote(doc.path, name))) input.value = app.doc?.title ?? "";
  }

  function setOtherNames(value: string) {
    if (!doc) return;
    const others = value
      .split(",")
      .map((item) => item.trim())
      .filter(Boolean);
    setProperty("aliases", [doc.title, ...others]);
  }

  const structureName = $derived(
    choices?.structures.find((s) => s.key === text("structure"))?.label ?? (text("structure") || "freeform"),
  );

  /** Where the open scene sits in its book: "Part One · Chapter 2 · scene 1 of 3", or null if it's unplaced. */
  const scenePosition = $derived.by(() => {
    for (const part of app.tree?.book?.parts ?? []) {
      for (const chapter of part.chapters) {
        const index = chapter.scenes.findIndex((s) => s.path === doc?.path);
        if (index >= 0) {
          const scene = `scene ${index + 1} of ${chapter.scenes.length}`;
          return [part.title, chapter.heading, scene].filter(Boolean).join(" · ");
        }
      }
    }
    return null;
  });
</script>

{#snippet nameField(label: string)}
  <label class="field">
    <span>{label}</span>
    <input
      value={doc?.title ?? ""}
      title="Renaming changes the file name and every link to this note too"
      onchange={(e) => rename(e.currentTarget)}
      onkeydown={(e) => e.key === "Enter" && e.currentTarget.blur()}
    />
  </label>
{/snippet}

{#snippet otherNamesField(placeholder: string)}
  <label class="field">
    <span>Other names</span>
    <input value={otherNames} {placeholder} onchange={(e) => setOtherNames(e.currentTarget.value)} />
  </label>
{/snippet}

<aside class="inspector">
  {#if doc && choices}
    <h2>{headings[doc.kind]}</h2>

    {#if doc.kind === "scene"}
      {#if scenePosition}
        <p class="muted">{scenePosition}</p>
      {:else}
        <p class="muted">Unplaced: not in the book's contents yet.</p>
        <p class="hint">To place it, drag it into a chapter on the Board.</p>
      {/if}
      {@render nameField("Name")}
      <label class="field">
        <span>Status</span>
        <select value={text("status")} onchange={(e) => setProperty("status", e.currentTarget.value)}>
          {#each options(choices.sceneStatuses, text("status")) as status (status)}
            <option value={status}>{status ? capitalise(status) : "—"}</option>
          {/each}
        </select>
      </label>
      <label class="field">
        <span>Point of view</span>
        <input
          list="character-names"
          value={plainLinks(text("pov"))}
          placeholder="Whose eyes?"
          onchange={(e) => setLink("pov", e.currentTarget.value, app.tree?.characters ?? [])}
        />
      </label>
      <datalist id="character-names">
        {#each app.tree?.characters ?? [] as character (character.path)}
          <option value={character.title}></option>
        {/each}
      </datalist>
      <label class="field">
        <span>Location</span>
        <input
          list="location-names"
          value={plainLinks(text("location"))}
          placeholder="Where?"
          onchange={(e) => setLink("location", e.currentTarget.value, app.tree?.locations ?? [])}
        />
      </label>
      <datalist id="location-names">
        {#each app.tree?.locations ?? [] as location (location.path)}
          <option value={location.title}></option>
        {/each}
      </datalist>
      <label class="field">
        <span>When</span>
        <input
          value={text("when")}
          placeholder="Year 3, day 12 · three days later"
          onchange={(e) => setProperty("when", e.currentTarget.value)}
        />
      </label>
      <label class="field">
        <span>Synopsis</span>
        <textarea
          rows="6"
          value={text("synopsis")}
          placeholder="A sentence or two: what happens, and what changes."
          oninput={(e) => setProperty("synopsis", e.currentTarget.value)}
        ></textarea>
      </label>
    {:else if doc.kind === "character"}
      {@render nameField("Name")}
      {@render otherNamesField("Nicknames and short names")}
      <p class="hint">Checks look for every name on the page.</p>
    {:else if doc.kind === "location"}
      {@render nameField("Name")}
      {@render otherNamesField("Other names")}
    {:else if doc.kind === "world"}
      <label class="field">
        <span>Category</span>
        <input
          list="world-categories"
          value={text("category")}
          placeholder="Technology, society, history…"
          onchange={(e) => setProperty("category", e.currentTarget.value)}
        />
      </label>
      <datalist id="world-categories">
        {#each ["technology", "society", "politics", "history", "object", "concept"] as category (category)}
          <option value={category}></option>
        {/each}
      </datalist>
      {@render nameField("Name")}
      {@render otherNamesField("Other names")}
    {:else if doc.kind === "project"}
      {@render nameField("Title")}
      <label class="field">
        <span>Genres</span>
        <input value={text("genre")} placeholder="Space opera, mystery…" onchange={(e) => setList("genre", e.currentTarget.value)} />
      </label>
      <p class="hint">The world every book in this project shares. Separate genres with commas.</p>
    {:else if doc.kind === "book"}
      {@render nameField("Title")}
      <label class="field">
        <span>Status</span>
        <select value={text("status")} onchange={(e) => setProperty("status", e.currentTarget.value)}>
          {#each options(choices.bookStatuses, text("status")) as status (status)}
            <option value={status}>{status ? capitalise(status) : "—"}</option>
          {/each}
        </select>
      </label>
      <label class="field">
        <span>Form</span>
        <select value={text("form")} onchange={(e) => setProperty("form", e.currentTarget.value)}>
          <option value="">—</option>
          {#each choices.forms as form (form.key)}
            <option value={form.key}>{capitalise(form.label)}</option>
          {/each}
        </select>
      </label>
      <div class="pair">
        <label class="field">
          <span>Target words</span>
          <input type="number" min="0" step="1000" value={text("target")} onchange={(e) => setProperty("target", e.currentTarget.value)} />
        </label>
        <div class="field">
          <span>Deadline</span>
          {#if text("deadline") || settingDeadline}
            <div class="clearable">
              <input
                type="date"
                aria-label="Deadline"
                value={text("deadline")}
                onchange={(e) => setProperty("deadline", e.currentTarget.value)}
              />
              <button
                class="clear"
                title="Remove the deadline"
                aria-label="Remove the deadline"
                onclick={() => {
                  settingDeadline = false;
                  setProperty("deadline", "");
                }}>×</button
              >
            </div>
          {:else}
            <button class="btn" onclick={() => (settingDeadline = true)}>Set a deadline</button>
          {/if}
        </div>
      </div>
    {:else if doc.kind === "timeline"}
      <p class="hint">
        The world's chronology, in the order things happened. Scenes join it through their <em>When</em>; list
        here what no scene shows, one event per line.
      </p>
    {:else if doc.kind === "research"}
      <label class="field">
        <span>Source</span>
        <input value={text("source")} placeholder="A URL or citation" onchange={(e) => setProperty("source", e.currentTarget.value)} />
      </label>
      {@render nameField("Subject")}
      {@render otherNamesField("Other names")}
      <p class="hint">What's true in the real world, for the story to use or knowingly break. It doesn't count toward your daily goal or any book.</p>
    {:else if doc.kind === "scrap"}
      {#if text("created")}
        <p class="muted">Kept {text("created")}{text("source") ? ` · from ${text("source")}` : ""}</p>
      {/if}
      {#if text("used")}
        <p class="muted">Used in {plainLinks(text("used"))}</p>
      {/if}
      <p class="hint">
        A fragment kept until it's useful. Its words count toward your daily goal, but not toward any book.
      </p>
      {#if canUse && !isCutScene && app.tree?.book}
        <label class="field">
          <span>Led to a scene</span>
          <select onchange={(e) => markUsed(e.currentTarget)}>
            <option value="">Choose a scene in {app.tree.book.summary.title}…</option>
            {#each bookScenes(app.tree) as scene (scene.path)}
              <option value={scene.path}>{scene.title}</option>
            {/each}
          </select>
        </label>
      {/if}
    {:else if doc.kind === "flow"}
      <p class="muted">Laid out as <strong>{structureName}</strong>.</p>
      <p class="hint">
        Place scenes under the beats as you plan, a line each. The beats are a guide, not a rulebook; the book's
        contents set the order it's read in.
      </p>
    {/if}

    {#if DELETABLE.includes(doc.kind)}
      <div class="note-actions">
        {#if doc.kind === "scene"}
          <button class="btn" onclick={() => cutScene(doc.path)}>Cut to the scratchpad</button>
        {/if}
        {#if canUse && isCutScene}
          <button class="btn" onclick={() => useScrap(doc.path, null)}>Put back into {app.tree?.book?.summary.title}</button>
        {:else if canUse && sceneName === null}
          <button class="btn" onclick={askSceneName}>Use as a scene in {app.tree?.book?.summary.title}…</button>
        {/if}
        <button class="btn danger" onclick={() => (deleting = true)}>Delete…</button>
      </div>
      {#if canUse && sceneName !== null}
        <form class="scene-name" onsubmit={useAsScene}>
          <label class="field">
            <span>Its name as a scene</span>
            <!-- svelte-ignore a11y_autofocus -->
            <input bind:value={sceneName} placeholder="Scene name" autofocus />
          </label>
          <p class="hint">It moves into {app.tree?.book?.summary.title} as an unplaced scene; its text stays as it is.</p>
          <div class="note-actions">
            <button type="submit" class="btn primary" disabled={!sceneName?.trim()}>Use as a scene</button>
            <button type="button" class="btn" onclick={() => (sceneName = null)}>Cancel</button>
          </div>
        </form>
      {/if}
    {/if}

    {#if (doc.kind === "book" && doc.book === app.book) || (doc.kind === "project" && doc.project === app.slug)}
      {@const kind = doc.kind}
      <div class="note-actions">
        <button class="btn danger" onclick={() => (deletingFolder = kind)}>
          {kind === "book" ? "Delete book…" : "Delete world…"}
        </button>
      </div>
    {/if}

    <div class="facts">
      <span>{plural(app.docWords, "word")}</span>
      {#if doc.kind === "book" && app.tree?.book}
        <span>{formatNumber(app.tree.book.summary.words)} in its scenes</span>
      {/if}
    </div>
    <p class="hint">
      Text between <code>%%</code> marks is a note to yourself: it isn't counted or exported.
    </p>
  {/if}
</aside>

{#if deleting && doc}
  <DeleteNote {doc} close={() => (deleting = false)} />
{/if}

{#if deletingFolder}
  <DeleteFolder kind={deletingFolder} close={() => (deletingFolder = null)} />
{/if}

<style>
  .note-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }

  .inspector {
    grid-area: inspector;
    overflow-y: auto;
    padding: 1rem 1rem 2rem;
    border-left: 1px solid var(--line);
    background: var(--bg);
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
  }

  h2 {
    font-size: 17px;
  }

  p {
    margin: 0;
  }

  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.6rem;
  }

  .clearable {
    display: flex;
    gap: 4px;
  }

  .clear {
    border: 1px solid var(--line);
    background: none;
    border-radius: 6px;
    color: var(--muted);
    padding: 0 8px;
  }

  .facts {
    display: grid;
    gap: 2px;
    padding-top: 0.8rem;
    border-top: 1px solid var(--line);
    color: var(--text-soft);
  }

  .hint {
    font-size: 12px;
    color: var(--muted);
  }

  code {
    font-family: var(--mono);
    font-size: 11px;
  }
</style>
