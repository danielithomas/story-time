<script lang="ts">
  import { api } from "../lib/api";
  import { capitalise, duration, formatNumber, plural, signed } from "../lib/format";
  import { app, toastError } from "../lib/store.svelte";
  import type { ProjectProgress } from "../lib/types";

  const DAYS = 30;
  let progress = $state<ProjectProgress | null>(null);

  // Reload when the project changes, and after saves refresh today's count.
  $effect(() => {
    const slug = app.slug;
    void app.today;
    if (slug) api.projectProgress(slug, DAYS).then((p) => (progress = p), toastError);
  });

  const fraction = $derived(
    progress?.project.target ? Math.min(1, progress.project.words / progress.project.target) : 0,
  );
  const scale = $derived(Math.max(progress?.goal ?? 0, ...(progress?.history.map((d) => d.words) ?? [0]), 1));
  const totalChapters = $derived(progress?.statuses.reduce((sum, s) => sum + s.chapters, 0) ?? 0);

  const day = (date: string) =>
    new Date(`${date}T12:00:00`).toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" });
</script>

{#if progress}
  {@const p = progress}
  <div class="view">
    <header>
      <h1>{p.project.title}</h1>
      <p class="muted">
        {[p.project.status, p.project.formLabel, p.project.genre].filter(Boolean).map((s) => capitalise(s!)).join(" · ")}
      </p>
    </header>

    <section class="headline">
      <div class="big">
        <strong>{formatNumber(p.project.words)}</strong>
        <span class="muted">{p.project.target ? `of ${formatNumber(p.project.target)} words` : "words"}</span>
      </div>
      {#if p.project.target}
        <div class="meter"><div style:width="{fraction * 100}%"></div></div>
        <p class="muted">
          {Math.floor(fraction * 100)}% ·
          {p.project.words >= p.project.target
            ? "target reached"
            : `${formatNumber(p.project.target - p.project.words)} to go`}
        </p>
      {/if}
    </section>

    <section class="tiles">
      <div class="tile">
        <span class="label">Today</span>
        <strong class:met={p.goal > 0 && p.today >= p.goal}>{signed(p.today)}</strong>
        <span class="muted">daily goal {formatNumber(p.goal)}</span>
      </div>
      <div class="tile">
        <span class="label">Deadline</span>
        {#if p.project.deadline}
          <strong>{p.daysLeft != null && p.daysLeft > 0 ? plural(p.daysLeft, "day") : "Passed"}</strong>
          <span class="muted">
            {p.project.deadline}{#if p.wordsPerDay} · {formatNumber(p.wordsPerDay)} words a day{/if}
          </span>
        {:else}
          <strong>—</strong>
          <span class="muted">Set one in the project overview</span>
        {/if}
      </div>
      <div class="tile">
        <span class="label">Length so far</span>
        <strong>{p.lengthLabel ? capitalise(p.lengthLabel) : "—"}</strong>
        <span class="muted">{p.lengthRange ?? "No words yet"}</span>
      </div>
      <div class="tile">
        <span class="label">As a manuscript</span>
        <strong>{plural(p.pages, "page")}</strong>
        <span class="muted">about {duration(p.readingMinutes)} to read</span>
      </div>
      <div class="tile">
        <span class="label">Story bible</span>
        <strong>{p.characters + p.locations + p.world} notes</strong>
        <span class="muted">
          {plural(p.characters, "character")} · {plural(p.locations, "location")} · {plural(p.world, "world note")}
        </span>
      </div>
    </section>

    {#if totalChapters > 0}
      <section>
        <h2>Chapters by status</h2>
        <div class="stack">
          {#each p.statuses as s (s.status)}
            <div class="part" data-status={s.status} style:flex-grow={s.chapters} title="{capitalise(s.status)}: {plural(s.chapters, 'chapter')}"></div>
          {/each}
        </div>
        <div class="legend">
          {#each p.statuses as s (s.status)}
            <span><span class="dot" data-status={s.status}></span>{capitalise(s.status)} {s.chapters} · {formatNumber(s.words)} words</span>
          {/each}
        </div>
      </section>
    {/if}

    <section>
      <h2>Last {DAYS} days</h2>
      <div class="chart" role="img" aria-label="Words written per day over the last {DAYS} days">
        {#if p.goal > 0}
          <div class="goal" style:bottom="{(p.goal / scale) * 100}%" title="Daily goal {formatNumber(p.goal)}"></div>
        {/if}
        {#each p.history as d (d.date)}
          <div class="col" title="{day(d.date)}: {signed(d.words)} words">
            <div class="bar" class:met={p.goal > 0 && d.words >= p.goal} style:height="{(Math.max(0, d.words) / scale) * 100}%"></div>
          </div>
        {/each}
      </div>
      <div class="axis muted">
        <span>{day(p.history[0]?.date ?? "")}</span>
        <span>Today</span>
      </div>
      <p class="hint">
        Counted from saved word totals. Words written while Story Time is closed count toward the next day it's open.
      </p>
    </section>
  </div>
{/if}

<style>
  .view {
    height: 100%;
    overflow-y: auto;
    padding: 2rem 2.5rem 4rem;
    display: grid;
    gap: 2rem;
    align-content: start;
  }

  .view > * {
    max-width: 960px;
  }

  h1 {
    font-size: 26px;
    margin-bottom: 0.3rem;
  }

  h2 {
    font-size: 16px;
    margin-bottom: 0.8rem;
  }

  p {
    margin: 0;
  }

  .big {
    display: flex;
    align-items: baseline;
    gap: 0.6rem;
  }

  .big strong {
    font: 600 40px/1 var(--serif);
  }

  .meter {
    height: 10px;
    margin: 0.8rem 0 0.4rem;
    border-radius: 99px;
    background: var(--panel);
    overflow: hidden;
  }

  .meter div {
    height: 100%;
    background: var(--accent);
    border-radius: 99px;
  }

  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(170px, 1fr));
    gap: 0.8rem;
  }

  .tile {
    display: grid;
    gap: 2px;
    padding: 0.8rem 0.9rem;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--bg);
  }

  .tile strong {
    font: 600 20px/1.3 var(--serif);
  }

  .tile .muted {
    font-size: 12px;
  }

  .label {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
  }

  .met {
    color: var(--final);
  }

  .stack {
    display: flex;
    gap: 2px;
    height: 14px;
    border-radius: 99px;
    overflow: hidden;
  }

  .part {
    flex-basis: 0;
    background: var(--tone, var(--outline));
  }

  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem 1.2rem;
    margin-top: 0.6rem;
    font-size: 12px;
    color: var(--text-soft);
  }

  .legend > span {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
  }

  .chart {
    position: relative;
    height: 160px;
    display: flex;
    align-items: flex-end;
    gap: 3px;
    border-bottom: 1px solid var(--line);
  }

  .col {
    flex: 1;
    height: 100%;
    display: flex;
    align-items: flex-end;
  }

  .bar {
    width: 100%;
    min-height: 0;
    background: var(--accent);
    opacity: 0.55;
    border-radius: 3px 3px 0 0;
  }

  .bar.met {
    opacity: 1;
  }

  .goal {
    position: absolute;
    left: 0;
    right: 0;
    border-top: 1px dashed var(--muted);
  }

  .axis {
    display: flex;
    justify-content: space-between;
    font-size: 11px;
    margin-top: 4px;
  }

  .hint {
    margin-top: 0.6rem;
    font-size: 12px;
    color: var(--muted);
  }
</style>
