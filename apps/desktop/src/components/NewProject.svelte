<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { capitalise } from "../lib/format";
  import { app, createBook, createProject } from "../lib/store.svelte";
  import type { Idea } from "../lib/types";

  const isBook = $derived(app.newProject?.mode === "book");
  const world = $derived(app.tree?.project.title ?? "");
  let title = $state(app.newProject?.title ?? "");
  let idea = $state(app.newProject?.idea ?? "");
  let worldName = $state("");
  let form = $state("novel");
  let target = $state<number | null>(null);
  let structure = $state("three-act");
  let ideas = $state<Idea[]>([]);
  let busy = $state(false);
  let error = $state("");

  onMount(() => {
    if (!isBook) api.listIdeas().then((all) => (ideas = all.filter((i) => !i.project)), () => {});
  });

  const formChoice = $derived(app.choices?.forms.find((f) => f.key === form));
  const structureChoice = $derived(app.choices?.structures.find((s) => s.key === structure));

  function pickIdea(name: string) {
    if (!title.trim() || title === idea) title = name;
    idea = name;
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    busy = true;
    error = "";
    try {
      const book = { title: title.trim(), form, target: target || null, structure };
      if (isBook) await createBook(book);
      else await createProject({ ...book, world: worldName.trim() || null, fromIdea: idea || null });
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  const close = () => (app.newProject = null);

  function focus(node: HTMLInputElement) {
    node.focus();
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && close()} />

<div class="backdrop">
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="new-project-title">
  <form onsubmit={submit}>
    <h2 id="new-project-title">{isBook ? `New book in ${world}` : "New book"}</h2>

    <label class="field">
      <span>Title</span>
      <input bind:value={title} placeholder="The book's working title" use:focus required />
    </label>
    <p class="hint">
      {isBook ? `It shares ${world}'s world bible: its characters, locations and world notes. ` : ""}It starts with
      an empty first scene, ready to write in.
    </p>

    <details open={!!idea}>
      <summary>More options</summary>
      <div class="more">
        {#if !isBook}
          <label class="field">
            <span>World</span>
            <input bind:value={worldName} placeholder="Same as the title" />
          </label>
          <p class="hint">Name the world if more books may be set in it. Books in one world share its characters, places and world notes.</p>
        {/if}

        {#if ideas.length}
          <label class="field">
            <span>Start from an idea</span>
            <select value={idea} onchange={(e) => pickIdea(e.currentTarget.value)}>
              <option value="">No: start blank</option>
              {#each ideas as item (item.name)}
                <option value={item.name}>{item.name}</option>
              {/each}
            </select>
          </label>
        {/if}

        <div class="pair">
          <label class="field">
            <span>Form</span>
            <select bind:value={form}>
              {#each app.choices?.forms ?? [] as choice (choice.key)}
                <option value={choice.key}>{capitalise(choice.label)}</option>
              {/each}
            </select>
          </label>
          <label class="field">
            <span>Target words</span>
            <input type="number" min="0" step="1000" bind:value={target} placeholder="Default for the form" />
          </label>
        </div>
        <p class="hint">{formChoice?.detail}</p>

        <label class="field">
          <span>Story structure</span>
          <select bind:value={structure}>
            {#each app.choices?.structures ?? [] as choice (choice.key)}
              <option value={choice.key}>{choice.label}</option>
            {/each}
          </select>
        </label>
        <p class="hint">{structureChoice?.detail}. It lays out the story flow note; change anything later.</p>
      </div>
    </details>

    {#if error}
      <p class="error">{error}</p>
    {/if}

    <div class="buttons">
      <button type="button" class="btn" onclick={close}>Cancel</button>
      <button class="btn primary" disabled={busy || !title.trim()}>{isBook ? "Add book" : "Create book"}</button>
    </div>
  </form>
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
    width: min(30rem, 92vw);
    padding: 1.4rem 1.5rem;
    background: var(--paper);
    border: 1px solid var(--line);
    border-radius: 12px;
    box-shadow: 0 20px 50px rgb(0 0 0 / 0.2);
  }

  form {
    display: grid;
    gap: 0.8rem;
  }

  h2 {
    font-size: 20px;
  }

  summary {
    font-size: 13px;
    color: var(--muted);
    cursor: pointer;
  }

  .more {
    display: grid;
    gap: 0.8rem;
    margin-top: 0.8rem;
  }

  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.6rem;
  }

  .hint {
    margin: -0.4rem 0 0;
    font-size: 12px;
    color: var(--muted);
  }

  .error {
    margin: 0;
    color: var(--danger);
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 0.4rem;
  }
</style>
