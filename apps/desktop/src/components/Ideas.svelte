<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { inlineMarkdown } from "../lib/format";
  import { app, openNewProject, selectProject, toast, toastError } from "../lib/store.svelte";
  import type { Idea } from "../lib/types";

  let ideas = $state<Idea[]>([]);
  let text = $state("");
  let nonFiction = $state(false);

  async function load() {
    try {
      ideas = await api.listIdeas();
    } catch (error) {
      toastError(error);
    }
  }

  onMount(() => void load());

  async function capture(event: SubmitEvent) {
    event.preventDefault();
    const idea = text.trim();
    if (!idea) return;
    try {
      await api.captureIdea(idea, nonFiction);
      text = "";
      await load();
      toast("Idea saved.");
    } catch (error) {
      toastError(error);
    }
  }

  const sections = $derived([...new Set(ideas.map((idea) => idea.section))]);
  const projectTitle = (slug: string) => app.projects.find((p) => p.slug === slug)?.title ?? slug;
</script>

<div class="view">
  <header>
    <h1>Ideas</h1>
    <p class="muted">Get it down now, decide later. Nothing here is ever deleted.</p>
  </header>

  <form onsubmit={capture}>
    <textarea bind:value={text} rows="3" placeholder="Name: what if…"></textarea>
    <div class="row">
      <label><input type="checkbox" bind:checked={nonFiction} /> Non-fiction</label>
      <button class="btn primary" disabled={!text.trim()}>Save idea</button>
    </div>
  </form>

  {#each sections as section (section)}
    <section>
      <h2>{section}</h2>
      {#each ideas.filter((idea) => idea.section === section) as idea, i (i)}
        <article>
          <p>
            <strong>{idea.name}</strong>{@html inlineMarkdown(idea.text.slice(idea.name.length))}
          </p>
          <!-- An idea whose world was deleted keeps its link, since the ideas file is only added to. -->
          {#if idea.project && app.projects.some((project) => project.slug === idea.project)}
            {@const slug = idea.project}
            <button class="btn" onclick={() => selectProject(slug).then(() => (app.view = "write"))}>
              Open {projectTitle(slug)}
            </button>
          {:else}
            <button class="btn" onclick={() => openNewProject(idea.name, idea.name)}>Start a project</button>
          {/if}
        </article>
      {/each}
    </section>
  {:else}
    <p class="muted">No ideas yet.</p>
  {/each}
</div>

<style>
  .view {
    height: 100%;
    overflow-y: auto;
    padding: 2rem 2.5rem 4rem;
    display: grid;
    gap: 1.6rem;
    align-content: start;
  }

  .view > * {
    max-width: 820px;
  }

  h1 {
    font-size: 26px;
    margin-bottom: 0.3rem;
  }

  h2 {
    font-size: 16px;
    margin-bottom: 0.6rem;
  }

  p {
    margin: 0;
  }

  form {
    display: grid;
    gap: 0.5rem;
  }

  textarea {
    font-family: var(--serif);
    font-size: 16px;
  }

  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  label {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    color: var(--text-soft);
  }

  label input {
    width: auto;
  }

  article {
    display: flex;
    gap: 1rem;
    align-items: flex-start;
    padding: 0.8rem 0;
    border-top: 1px solid var(--line);
  }

  article p {
    flex: 1;
    font-family: var(--serif);
    line-height: 1.6;
    color: var(--text-soft);
  }

  article strong {
    color: var(--text);
  }

  article :global(code) {
    font-family: var(--mono);
    font-size: 0.85em;
  }
</style>
