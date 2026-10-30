<script lang="ts">
  import { capitalise, formatNumber, plainLinks, plural } from "../lib/format";
  import { app, setProperty } from "../lib/store.svelte";

  const doc = $derived(app.doc);

  // An empty date input shows today's date as a placeholder in WebKit, which reads as a real
  // deadline, so an unset deadline is a button until the writer chooses to set one.
  let settingDeadline = $state(false);
  const choices = $derived(app.choices);

  const headings: Record<string, string> = {
    project: "Project",
    flow: "Story flow",
    chapter: "Chapter",
    character: "Character",
    location: "Location",
    world: "World",
    note: "Note",
  };

  function text(key: string): string {
    const value = doc?.properties[key];
    return Array.isArray(value) ? value.join(", ") : (value ?? "");
  }

  /** The choices for a select, keeping a value typed elsewhere (in Obsidian, say) that isn't one of them. */
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

  /** A point of view matching a character note is stored as a link to it, so Obsidian can follow it. */
  function setPov(value: string) {
    const name = value.trim();
    const character = app.tree?.characters.find(
      (c) => c.name.toLowerCase() === name.toLowerCase() || c.title.toLowerCase() === name.toLowerCase(),
    );
    setProperty("pov", character ? `[[${character.name}]]` : name);
  }

  const structureName = $derived(
    choices?.structures.find((s) => s.key === text("structure"))?.label ?? (text("structure") || "freeform"),
  );

  const chapterPosition = $derived.by(() => {
    const chapters = app.tree?.chapters ?? [];
    const index = chapters.findIndex((c) => c.path === doc?.path);
    return index < 0 ? "" : `Chapter ${index + 1} of ${chapters.length}`;
  });
</script>

<aside class="inspector">
  {#if doc && choices}
    <h2>{headings[doc.kind]}</h2>

    {#if doc.kind === "chapter"}
      <p class="muted">{chapterPosition}</p>
      <label class="field">
        <span>Status</span>
        <select value={text("status")} onchange={(e) => setProperty("status", e.currentTarget.value)}>
          {#each options(choices.chapterStatuses, text("status")) as status (status)}
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
          onchange={(e) => setPov(e.currentTarget.value)}
        />
      </label>
      <datalist id="character-names">
        {#each app.tree?.characters ?? [] as character (character.path)}
          <option value={character.title}></option>
        {/each}
      </datalist>
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
      <label class="field">
        <span>Role</span>
        <select value={text("role")} onchange={(e) => setProperty("role", e.currentTarget.value)}>
          {#each options(choices.roles, text("role")) as role (role)}
            <option value={role}>{role ? capitalise(role) : "—"}</option>
          {/each}
        </select>
      </label>
      <label class="field">
        <span>Also known as</span>
        <input value={text("aliases")} placeholder="Nicknames, short names" onchange={(e) => setList("aliases", e.currentTarget.value)} />
      </label>
      <p class="hint">Separate names with commas. Checks look for these on the page.</p>
    {:else if doc.kind === "location"}
      <label class="field">
        <span>Also known as</span>
        <input value={text("aliases")} placeholder="Other names" onchange={(e) => setList("aliases", e.currentTarget.value)} />
      </label>
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
      <label class="field">
        <span>Also known as</span>
        <input value={text("aliases")} placeholder="Other names" onchange={(e) => setList("aliases", e.currentTarget.value)} />
      </label>
    {:else if doc.kind === "project"}
      <label class="field">
        <span>Title</span>
        <input value={text("title")} onchange={(e) => setProperty("title", e.currentTarget.value)} />
      </label>
      <label class="field">
        <span>Status</span>
        <select value={text("status")} onchange={(e) => setProperty("status", e.currentTarget.value)}>
          {#each options(choices.projectStatuses, text("status")) as status (status)}
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
      <label class="field">
        <span>Genre</span>
        <input value={text("genre")} onchange={(e) => setProperty("genre", e.currentTarget.value)} />
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
    {:else if doc.kind === "flow"}
      <p class="muted">Laid out as <strong>{structureName}</strong>.</p>
      <p class="hint">
        Place chapters under the beats as you plan, one line per scene. The beats are a guide, not a rulebook.
      </p>
    {/if}

    <div class="facts">
      <span>{plural(app.docWords, "word")}</span>
      {#if doc.kind === "project" && app.tree}
        <span>{formatNumber(app.tree.project.words)} in the chapters</span>
      {/if}
    </div>
    <p class="hint">
      Text between <code>%%</code> marks is a note to yourself: it isn't counted or exported.
    </p>
  {/if}
</aside>

<style>
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
