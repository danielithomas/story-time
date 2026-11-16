<script lang="ts">
  import { app, closeCompare, saveCompared } from "../lib/store.svelte";
  import type { ComparedPiece, Pick, PropertyValue } from "../lib/types";

  const labels: Record<string, string> = {
    aliases: "Other names",
    pov: "Point of view",
    used: "Led to a scene",
    target: "Target words",
  };
  const label = (key: string) => labels[key] ?? key.charAt(0).toUpperCase() + key.slice(1).replace(/[_-]+/g, " ");
  const shown = (value: PropertyValue | null) => (Array.isArray(value) ? value.join(", ") : value);

  /** Pieces without the spacing at either end, which only separates them from their neighbours. */
  function trimmed(pieces: ComparedPiece[]): ComparedPiece[] {
    const kept = pieces.map((piece) => ({ ...piece }));
    while (kept.length && !kept[0].text.trim()) kept.shift();
    while (kept.length && !kept[kept.length - 1].text.trim()) kept.pop();
    if (kept.length) {
      kept[0].text = kept[0].text.trimStart();
      kept[kept.length - 1].text = kept[kept.length - 1].text.trimEnd();
    }
    return kept;
  }

  type Shown =
    | { kind: "same"; index: number; head: string[]; folded: string[]; tail: string[] }
    | { kind: "different"; index: number; n: number; here: ComparedPiece[]; other: ComparedPiece[] };

  /** The body in order, with each difference numbered, and long stretches that read the same folded
   * down to the paragraph either side of a difference. */
  const parts = $derived.by((): Shown[] => {
    const all = app.compare?.compared.parts ?? [];
    let n = 0;
    return all.map((part, index) => {
      if (part.kind === "different") return { kind: "different", index, n: n++, here: trimmed(part.here), other: trimmed(part.other) };
      const paragraphs = part.text
        .split(/\n[ \t]*\n/)
        .map((paragraph) => paragraph.trim())
        .filter(Boolean);
      if (paragraphs.length <= 3) return { kind: "same", index, head: paragraphs, folded: [], tail: [] };
      const head = index > 0 ? paragraphs.slice(0, 1) : [];
      const tail = index < all.length - 1 ? paragraphs.slice(-1) : [];
      return { kind: "same", index, head, folded: paragraphs.slice(head.length, paragraphs.length - tail.length), tail };
    });
  });

  let unfolded = $state<number[]>([]);
  const count = $derived(app.compare?.picks.length ?? 0);
  const properties = $derived(app.compare?.compared.properties ?? []);

  function pick(n: number, choice: Pick) {
    if (app.compare) app.compare.picks[n] = choice;
  }

  function takeProperty(key: string, other: boolean) {
    if (!app.compare) return;
    const rest = app.compare.takeOther.filter((taken) => taken !== key);
    app.compare.takeOther = other ? [...rest, key] : rest;
  }

  function takeAll(side: "here" | "other") {
    if (!app.compare) return;
    app.compare.picks = app.compare.picks.map(() => side);
    app.compare.takeOther = side === "other" ? properties.map((difference) => difference.key) : [];
  }

  const summary = $derived.by(() => {
    const text = count === 1 ? "1 difference in the text" : `${count} differences in the text`;
    const props = properties.length === 1 ? "1 property set differently" : `${properties.length} properties set differently`;
    if (!count && !properties.length) return "The two versions now read the same.";
    if (!properties.length) return `${text}.`;
    if (!count) return `The text reads the same; ${props}.`;
    return `${text}, and ${props}.`;
  });
</script>

{#snippet prose(pieces: ComparedPiece[], side: "here" | "other")}{#each pieces as piece}{#if piece.differs}<mark class={side}>{#if !piece.text.trim() && piece.text.includes("\n")}¶{/if}{piece.text}</mark>{:else}{piece.text}{/if}{/each}{/snippet}

{#if app.compare && app.doc}
  {@const compare = app.compare}
  <section class="compare" aria-label="Compare the two versions">
    <div class="scroll">
      <div class="page">
        <header>
          <h2>Two versions of {app.doc.title}</h2>
          <p>
            This note was changed here and on another computer before they synced. Choose what to keep where they differ;
            everything else reads the same in both. {summary}
          </p>
          <div class="tools">
            <span class="key"><mark class="here">Here</mark> <mark class="other">On the other computer</mark></span>
            <span class="spacer"></span>
            <span class="muted">Take everything from</span>
            <button class="btn" onclick={() => takeAll("here")}>Here</button>
            <button class="btn" onclick={() => takeAll("other")}>The other computer</button>
          </div>
        </header>

        {#if properties.length}
          <section class="properties">
            <h3>Properties</h3>
            {#each properties as difference (difference.key)}
              {@const takesOther = compare.takeOther.includes(difference.key)}
              <div class="property" role="radiogroup" aria-label={label(difference.key)}>
                <span class="name">{label(difference.key)}</span>
                <label class="option" class:picked={!takesOther}>
                  <input type="radio" name="property-{difference.key}" checked={!takesOther} onchange={() => takeProperty(difference.key, false)} />
                  <span class="who">Here</span>
                  <span class="value" class:none={difference.here === null}>{shown(difference.here) ?? "Not set"}</span>
                </label>
                <label class="option" class:picked={takesOther}>
                  <input type="radio" name="property-{difference.key}" checked={takesOther} onchange={() => takeProperty(difference.key, true)} />
                  <span class="who">On the other computer</span>
                  <span class="value" class:none={difference.other === null}>{shown(difference.other) ?? "Not set"}</span>
                </label>
              </div>
            {/each}
          </section>
        {/if}

        <div class="body">
          {#each parts as part (part.index)}
            {#if part.kind === "same"}
              {#each part.head as paragraph, i (i)}<p class="same">{paragraph}</p>{/each}
              {#if part.folded.length}
                {#if unfolded.includes(part.index)}
                  {#each part.folded as paragraph, i (i)}<p class="same">{paragraph}</p>{/each}
                {:else}
                  <button class="fold" onclick={() => (unfolded = [...unfolded, part.index])}>
                    {part.folded.length === 1 ? "1 paragraph" : `${part.folded.length} paragraphs`} the same in both: show
                  </button>
                {/if}
              {/if}
              {#each part.tail as paragraph, i (i)}<p class="same">{paragraph}</p>{/each}
            {:else}
              {@const choice = compare.picks[part.n]}
              <div class="difference" role="radiogroup" aria-label="Difference {part.n + 1} of {count}">
                <span class="number">{part.n + 1} of {count}</span>
                <label class="option" class:picked={choice === "here"}>
                  <input type="radio" name="difference-{part.n}" checked={choice === "here"} onchange={() => pick(part.n, "here")} />
                  <span class="who">Here</span>
                  {#if part.here.length}
                    <span class="text">{@render prose(part.here, "here")}</span>
                  {:else}
                    <span class="text none">Not here</span>
                  {/if}
                </label>
                <label class="option" class:picked={choice === "other"}>
                  <input type="radio" name="difference-{part.n}" checked={choice === "other"} onchange={() => pick(part.n, "other")} />
                  <span class="who">On the other computer</span>
                  {#if part.other.length}
                    <span class="text">{@render prose(part.other, "other")}</span>
                  {:else}
                    <span class="text none">Not on the other computer</span>
                  {/if}
                </label>
                {#if part.here.length && part.other.length}
                  <label class="option" class:picked={choice === "both"}>
                    <input type="radio" name="difference-{part.n}" checked={choice === "both"} onchange={() => pick(part.n, "both")} />
                    <span class="who">Both</span>
                    <span class="hint">This computer's, then the other's, to edit together afterwards</span>
                  </label>
                {/if}
              </div>
            {/if}
          {/each}
        </div>
      </div>
    </div>
    <footer>
      <span class="muted">The version you save replaces this note on every computer. Both versions stay in the history.</span>
      <button class="btn" disabled={compare.saving} onclick={closeCompare}>Cancel</button>
      <button class="btn primary" disabled={compare.saving} onclick={saveCompared}>
        {compare.saving ? "Saving…" : "Save this version"}
      </button>
    </footer>
  </section>
{/if}

<style>
  .compare {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--paper);
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .page {
    max-width: 46rem;
    margin: 0 auto;
    padding: 1.5rem 1.5rem 3rem;
  }

  h2 {
    margin: 0 0 0.4rem;
    font-size: 20px;
  }

  h3 {
    margin: 0 0 0.6rem;
    font-size: 13px;
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  header p {
    margin: 0 0 0.9rem;
    color: var(--text-soft);
  }

  .tools {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
    padding-bottom: 1rem;
    border-bottom: 1px solid var(--line);
  }

  .key {
    display: inline-flex;
    gap: 0.4rem;
    font-size: 12px;
  }

  .spacer {
    flex: 1;
  }

  .muted {
    color: var(--muted);
    font-size: 12px;
  }

  mark {
    color: inherit;
    border-radius: 3px;
    padding: 0 1px;
  }

  mark.here {
    background: var(--here-soft);
  }

  mark.other {
    background: var(--other-soft);
  }

  .key mark {
    padding: 1px 6px;
  }

  .properties {
    padding: 1.1rem 0;
    border-bottom: 1px solid var(--line);
  }

  .property {
    display: grid;
    grid-template-columns: 8rem 1fr 1fr;
    gap: 0.5rem;
    align-items: stretch;
    margin-bottom: 0.5rem;
  }

  .property .name {
    padding-top: 0.55rem;
    color: var(--text-soft);
  }

  .body {
    padding-top: 1.1rem;
    font-family: var(--serif);
    font-size: 16px;
    line-height: 1.65;
  }

  .same {
    margin: 0 0 0.9rem;
    color: var(--muted);
    white-space: pre-wrap;
  }

  .fold {
    display: block;
    margin: 0 0 0.9rem;
    padding: 0.3rem 0;
    border: none;
    border-top: 1px dashed var(--line);
    border-bottom: 1px dashed var(--line);
    width: 100%;
    background: none;
    color: var(--muted);
    font-family: var(--sans);
    font-size: 12px;
    text-align: center;
    cursor: pointer;
  }

  .fold:hover {
    color: var(--text);
  }

  .difference {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    margin: 0 0 1.1rem;
  }

  .number {
    font-family: var(--sans);
    font-size: 11px;
    color: var(--muted);
  }

  .option {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: baseline;
    align-content: start;
    gap: 0.15rem 0.5rem;
    padding: 0.5rem 0.75rem;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--bg);
    cursor: pointer;
  }

  .option.picked {
    border-color: var(--accent);
    background: var(--paper);
    box-shadow: 0 0 0 1px var(--accent);
  }

  .who {
    font-family: var(--sans);
    font-size: 12px;
    font-weight: 600;
    color: var(--text-soft);
  }

  .text,
  .value,
  .hint {
    grid-column: 2;
  }

  .text {
    white-space: pre-wrap;
  }

  .option:not(.picked) .text {
    color: var(--text-soft);
  }

  .value {
    font-family: var(--serif);
    overflow-wrap: anywhere;
  }

  .hint {
    font-family: var(--sans);
    font-size: 12px;
    color: var(--muted);
  }

  .none {
    font-style: italic;
    color: var(--muted);
  }

  footer {
    flex: none;
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 8px 1.5rem;
    border-top: 1px solid var(--line);
    background: var(--bg);
  }

  footer .muted {
    flex: 1;
  }
</style>
