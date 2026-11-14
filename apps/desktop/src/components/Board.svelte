<script lang="ts">
  import { onMount, tick } from "svelte";
  import { flip } from "svelte/animate";
  import { formatNumber, plainLinks, plural } from "../lib/format";
  import { addScene, app, deleteChapter, moveChapter, moveScene, openNote, refreshTree } from "../lib/store.svelte";
  import type { ChapterRef, ChapterSpot, ScenePlace, SceneRef, SceneSpot } from "../lib/types";
  import DeleteChapter from "./DeleteChapter.svelte";

  /** Where the scene being added goes, as a key: "end", "unplaced" or "chapter:N". */
  let adding = $state<string | null>(null);
  let title = $state("");

  const key = (place: ScenePlace) => (typeof place === "object" ? `chapter:${place.chapter}` : place);

  async function submit(event: SubmitEvent, place: ScenePlace) {
    event.preventDefault();
    const value = title.trim();
    adding = null;
    title = "";
    if (value) await addScene(value, place);
  }

  function focus(node: HTMLInputElement) {
    node.focus();
  }

  const where = (scene: SceneRef) => [scene.pov, scene.location].filter(Boolean).map((s) => plainLinks(s!)).join(" · ");

  // Show the contents as they are now, including edits to the book note made just before opening the Board. Leaving
  // the Board mid-drag drops the drag.
  onMount(() => {
    void refreshTree();
    return stop;
  });

  // ---- Moving scenes and chapters ----
  //
  // Pressing a scene card or a chapter's heading and moving a few pixels starts a drag. This uses pointer events
  // rather than HTML drag and drop, which Tauri's handling of files dropped on the window gets in the way of. A copy
  // follows the pointer, a bar shows where it would go, and Escape cancels. Alt with the arrow keys moves the
  // focused card or chapter one step instead.

  type Dragged = { kind: "scene"; scene: SceneRef } | { kind: "chapter"; chapter: ChapterRef };

  /** What shows where a drop would go. */
  type Mark =
    | { at: "card"; path: string; side: "before" | "after" }
    | { at: "chapter"; index: number; side: "before" | "after" | "end" }
    | { at: "part"; index: number }
    | { at: "end" }
    | { at: "unplaced" };

  type Aim = { mark: Mark; spot: SceneSpot | ChapterSpot };

  const DRAG_AFTER = 6;
  const SCROLL_EDGE = 60;

  let view = $state<HTMLElement>();
  let dragging = $state<Dragged | null>(null);
  let mark = $state<Mark | null>(null);
  let ghost = $state({ x: 0, y: 0, dx: 0, dy: 0, width: 0 });
  let busy = $state(false);
  let press: { dragged: Dragged; x: number; y: number; dx: number; dy: number; width: number } | null = null;
  let spot: SceneSpot | ChapterSpot | null = null;
  let frame = 0;
  let swallowClick = false;

  const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  const chapters = () => app.tree?.book?.parts.flatMap((part) => part.chapters) ?? [];

  /** The position of the part each chapter is in, by the chapter's index. */
  const partOf = (index: number) => app.tree?.book?.parts.findIndex((part) => part.chapters.some((c) => c.index === index)) ?? -1;

  /** The chapter a scene is in, and its position there, or null for an unplaced scene. */
  function placeOf(path: string) {
    for (const chapter of chapters()) {
      const position = chapter.scenes.findIndex((scene) => scene.path === path);
      if (position >= 0) return { chapter, position };
    }
    return null;
  }

  function pressed(event: PointerEvent, dragged: Dragged) {
    if (event.button !== 0 || busy || press) return;
    const source = event.currentTarget as HTMLElement;
    const rect = source.getBoundingClientRect();
    const scene = dragged.kind === "scene";
    press = {
      dragged,
      x: event.clientX,
      y: event.clientY,
      dx: scene ? event.clientX - rect.left : 14,
      dy: scene ? event.clientY - rect.top : 14,
      width: scene ? rect.width : 0,
    };
    window.addEventListener("pointermove", pointerMoved);
    window.addEventListener("pointerup", released);
    window.addEventListener("pointercancel", stop);
    window.addEventListener("blur", stop);
    window.addEventListener("keydown", keyDuringDrag, true);
  }

  function pointerMoved(event: PointerEvent) {
    if (!press) return;
    if (!dragging) {
      if (Math.hypot(event.clientX - press.x, event.clientY - press.y) < DRAG_AFTER) return;
      dragging = press.dragged;
      ghost = { x: event.clientX, y: event.clientY, dx: press.dx, dy: press.dy, width: press.width };
      document.body.classList.add("moving");
      frame = requestAnimationFrame(scrollNearEdges);
    }
    ghost.x = event.clientX;
    ghost.y = event.clientY;
    aim();
  }

  function released() {
    const drop = dragging && spot ? { dragged: dragging, spot } : null;
    if (dragging) {
      // The click that ends a drag over a card shouldn't open the scene.
      swallowClick = true;
      setTimeout(() => (swallowClick = false));
    }
    stop();
    if (drop) void run(drop.dragged, drop.spot);
  }

  function keyDuringDrag(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    event.preventDefault();
    event.stopPropagation();
    stop();
  }

  function stop() {
    press = null;
    dragging = null;
    mark = null;
    spot = null;
    cancelAnimationFrame(frame);
    document.body.classList.remove("moving");
    window.removeEventListener("pointermove", pointerMoved);
    window.removeEventListener("pointerup", released);
    window.removeEventListener("pointercancel", stop);
    window.removeEventListener("blur", stop);
    window.removeEventListener("keydown", keyDuringDrag, true);
  }

  /** Scrolls the Board while the pointer is held near its top or bottom edge, so a card can go anywhere in a long book. */
  function scrollNearEdges() {
    if (!dragging || !view) return;
    const rect = view.getBoundingClientRect();
    const above = rect.top + SCROLL_EDGE - ghost.y;
    const below = ghost.y - (rect.bottom - SCROLL_EDGE);
    const speed = above > 0 ? -Math.min(above / 3, 20) : below > 0 ? Math.min(below / 3, 20) : 0;
    if (speed) {
      view.scrollTop += speed;
      aim();
    }
    frame = requestAnimationFrame(scrollNearEdges);
  }

  /** Works out where a drop under the pointer would go. Nowhere, if that is where it already is. */
  function aim() {
    const target = document.elementFromPoint(ghost.x, ghost.y);
    const found =
      target instanceof Element && dragging
        ? dragging.kind === "scene"
          ? aimScene(target, dragging.scene)
          : aimChapter(target, dragging.chapter)
        : null;
    mark = found?.mark ?? null;
    spot = found?.spot ?? null;
  }

  function aimScene(target: Element, scene: SceneRef): Aim | null {
    const card = target.closest<HTMLElement>("[data-scene]");
    const section = target.closest<HTMLElement>("[data-chapter]");
    const zone = target.closest<HTMLElement>("[data-zone]")?.dataset.zone;
    let found: { mark: Mark; spot: SceneSpot } | null = null;
    if (card && placeOf(card.dataset.scene!)) {
      const path = card.dataset.scene!;
      const rect = card.getBoundingClientRect();
      const side = ghost.x < rect.left + rect.width / 2 ? "before" : "after";
      found = { mark: { at: "card", path, side }, spot: beside(path, side) };
    } else if (section) {
      const index = Number(section.dataset.chapter);
      found = { mark: { at: "chapter", index, side: "end" }, spot: { chapter: index } };
    } else if (zone === "end") {
      found = { mark: { at: "end" }, spot: "end" };
    } else if (zone === "unplaced") {
      found = { mark: { at: "unplaced" }, spot: "unplaced" };
    }
    return found && !alreadyThere(scene, found.spot) ? found : null;
  }

  /** Where a scene dropped on one side of a placed card goes. */
  function beside(path: string, side: "before" | "after"): SceneSpot {
    if (side === "before") return { before: path };
    const { chapter, position } = placeOf(path)!;
    const next = chapter.scenes[position + 1];
    return next ? { before: next.path } : { chapter: chapter.index };
  }

  function alreadyThere(scene: SceneRef, to: SceneSpot) {
    const at = placeOf(scene.path);
    if (!at) return to === "unplaced";
    const { chapter, position } = at;
    const last = position === chapter.scenes.length - 1;
    if (to === "end") {
      const all = chapters();
      return chapter.title === null && chapter.scenes.length === 1 && chapter.index === all[all.length - 1].index;
    }
    if (to === "unplaced") return false;
    if ("chapter" in to) return to.chapter === chapter.index && last;
    return to.before === scene.path || to.before === chapter.scenes[position + 1]?.path;
  }

  function aimChapter(target: Element, chapter: ChapterRef): Aim | null {
    const section = target.closest<HTMLElement>("[data-chapter]");
    const part = target.closest<HTMLElement>("[data-part]");
    const zone = target.closest<HTMLElement>("[data-zone]")?.dataset.zone;
    let found: { mark: Mark; spot: ChapterSpot } | null = null;
    if (section) {
      const index = Number(section.dataset.chapter);
      const rect = section.getBoundingClientRect();
      const side = ghost.y < rect.top + rect.height / 2 ? "before" : "after";
      found = { mark: { at: "chapter", index, side }, spot: side === "before" ? { before: index } : { after: index } };
    } else if (part) {
      const index = Number(part.dataset.part);
      found = { mark: { at: "part", index }, spot: { part: index } };
    } else if (zone) {
      const all = chapters();
      const index = all[all.length - 1]?.index;
      if (index !== undefined) found = { mark: { at: "chapter", index, side: "after" }, spot: { after: index } };
    }
    return found && !chapterAlreadyThere(chapter.index, found.spot) ? found : null;
  }

  function chapterAlreadyThere(from: number, to: ChapterSpot) {
    const samePart = (n: number) => partOf(n) === partOf(from);
    if ("before" in to) return to.before === from || (to.before === from + 1 && samePart(to.before));
    if ("after" in to) return to.after === from || (to.after === from - 1 && samePart(to.after));
    return app.tree?.book?.parts[to.part]?.chapters[0]?.index === from;
  }

  async function run(dragged: Dragged, to: SceneSpot | ChapterSpot) {
    busy = true;
    try {
      if (dragged.kind === "scene") await moveScene(dragged.scene, to as SceneSpot);
      else await moveChapter(dragged.chapter, to as ChapterSpot);
    } finally {
      busy = false;
    }
  }

  // ---- Deleting a chapter ----

  /** The chapter whose deletion is being confirmed. An empty one goes at once, since Undo brings it back. */
  let deleting = $state<ChapterRef | null>(null);

  function askToDelete(chapter: ChapterRef) {
    if (busy) return;
    if (chapter.scenes.length) deleting = chapter;
    else void deleteChapter(chapter, false);
  }

  // ---- Moving with the keyboard ----

  /** Alt with an arrow key moves a scene one place earlier or later, across chapters. */
  async function sceneKey(event: KeyboardEvent, scene: SceneRef) {
    const later = event.key === "ArrowRight" || event.key === "ArrowDown";
    if (!event.altKey || busy || !(later || event.key === "ArrowLeft" || event.key === "ArrowUp")) return;
    event.preventDefault();
    const to = sceneStep(scene, later);
    if (!to) return;
    await run({ kind: "scene", scene }, to);
    await tick();
    document.querySelector<HTMLElement>(`[data-scene="${CSS.escape(scene.path)}"]`)?.focus();
  }

  function sceneStep(scene: SceneRef, later: boolean): SceneSpot | null {
    const all = chapters();
    const at = placeOf(scene.path);
    if (!at) return later ? null : all.length ? { chapter: all[all.length - 1].index } : "end";
    const { chapter, position } = at;
    const scenes = chapter.scenes;
    const c = all.findIndex((ch) => ch.index === chapter.index);
    if (!later) {
      if (position > 0) return { before: scenes[position - 1].path };
      return c > 0 ? { chapter: all[c - 1].index } : null;
    }
    if (position + 1 < scenes.length) {
      return position + 2 < scenes.length ? { before: scenes[position + 2].path } : { chapter: chapter.index };
    }
    const next = all[c + 1];
    if (!next) return null;
    return next.scenes.length ? { before: next.scenes[0].path } : { chapter: next.index };
  }

  /** Alt with the up or down arrow moves a chapter one place, into the next part at either end of its own. */
  async function chapterKey(event: KeyboardEvent, chapter: ChapterRef) {
    if (event.key === "Delete" || event.key === "Backspace") {
      event.preventDefault();
      askToDelete(chapter);
      return;
    }
    const up = event.key === "ArrowUp" || event.key === "ArrowLeft";
    if (!event.altKey || busy || !(up || event.key === "ArrowDown" || event.key === "ArrowRight")) return;
    event.preventDefault();
    const from = chapter.index;
    const count = chapters().length;
    const same = (n: number) => partOf(n) === partOf(from);
    let to: ChapterSpot;
    if (up) {
      if (from === 0) return;
      to = same(from - 1) ? { before: from - 1 } : { after: from - 1 };
    } else {
      if (from + 1 >= count) return;
      to = same(from + 1) ? { after: from + 1 } : { before: from + 1 };
    }
    await run({ kind: "chapter", chapter }, to);
    await tick();
    const now = "before" in to ? (to.before < from ? to.before : to.before - 1) : to.after < from ? to.after + 1 : to.after;
    document.querySelector<HTMLElement>(`[data-grip="${now}"]`)?.focus();
  }

  const lifted = (path: string) => dragging?.kind === "scene" && dragging.scene.path === path;
  const cardSide = (path: string) => (mark?.at === "card" && mark.path === path ? mark.side : null);
  const chapterSide = (index: number) => (mark?.at === "chapter" && mark.index === index ? mark.side : null);
  const movingScene = $derived(dragging?.kind === "scene");
</script>

{#snippet face(scene: SceneRef)}
  <div class="row">
    <span class="when">{scene.when ?? ""}</span>
    <span class="badge" data-status={scene.status ?? ""}>{scene.status ?? "no status"}</span>
  </div>
  <h3>{scene.title}</h3>
  <p class="synopsis" class:none={!scene.synopsis}>{scene.synopsis ?? "No synopsis yet"}</p>
  <div class="row foot">
    <span>{where(scene)}</span>
    <span>{formatNumber(scene.words)} words</span>
  </div>
{/snippet}

{#snippet card(scene: SceneRef)}
  <button
    class="card scene"
    class:lifted={lifted(scene.path)}
    class:drop-before={cardSide(scene.path) === "before"}
    class:drop-after={cardSide(scene.path) === "after"}
    data-scene={scene.path}
    aria-keyshortcuts="Alt+ArrowLeft Alt+ArrowRight"
    onpointerdown={(e) => pressed(e, { kind: "scene", scene })}
    onclick={() => swallowClick || openNote(scene.path)}
    onkeydown={(e) => sceneKey(e, scene)}
  >
    {@render face(scene)}
  </button>
{/snippet}

{#snippet addCard(place: ScenePlace, label: string, dropLabel: string)}
  {#if adding === key(place)}
    <form class="card new" onsubmit={(e) => submit(e, place)}>
      <input bind:value={title} placeholder="Scene title" use:focus onblur={() => (adding = null)} />
    </form>
  {:else}
    <button class="card new" onclick={() => (adding = key(place))}>{movingScene ? dropLabel : label}</button>
  {/if}
{/snippet}

{#if app.tree?.book}
  {@const book = app.tree.book}
  <div class="view" bind:this={view}>
    <header>
      <h1>{book.summary.title}</h1>
      <p class="muted">
        {plural(book.summary.chapters, "chapter")} · {plural(book.summary.scenes, "scene")} · {plural(book.summary.words, "word")}.
        Each card shows a scene's synopsis; set it in the inspector while writing. Drag a card to move its scene, or a
        chapter's heading to move the chapter. Alt and the arrow keys move the one selected.
      </p>
    </header>

    {#each book.parts as part, p (p)}
      {#if part.title}
        <h2 class="part" class:drop-into={mark?.at === "part" && mark.index === p} data-part={p}>{part.title}</h2>
      {/if}
      {#each part.chapters as chapter (chapter.index)}
        <section
          class:lifted={dragging?.kind === "chapter" && dragging.chapter.index === chapter.index}
          class:drop-before={chapterSide(chapter.index) === "before"}
          class:drop-after={chapterSide(chapter.index) === "after"}
          class:drop-end={chapterSide(chapter.index) === "end"}
          data-chapter={chapter.index}
        >
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
          <h2 class="chapter" onpointerdown={(e) => pressed(e, { kind: "chapter", chapter })}>
            <button
              class="grip"
              data-grip={chapter.index}
              aria-label="Move {chapter.heading}"
              aria-keyshortcuts="Alt+ArrowUp Alt+ArrowDown Delete"
              title="Drag to move this chapter, or use Alt and the arrow keys"
              onkeydown={(e) => chapterKey(e, chapter)}
            ></button>
            {chapter.heading} <span class="muted">· {plural(chapter.words, "word")}</span>
            <button
              class="remove"
              aria-label="Delete {chapter.heading}"
              title={chapter.scenes.length ? "Delete this chapter; its scenes stay in the book" : "Delete this empty chapter"}
              onpointerdown={(e) => e.stopPropagation()}
              onclick={() => askToDelete(chapter)}
            >Delete</button>
          </h2>
          <div class="cards">
            {#each chapter.scenes as scene (scene.path)}
              <div class="slot" animate:flip={{ duration: reduceMotion ? 0 : 180 }}>
                {@render card(scene)}
              </div>
            {/each}
            {@render addCard({ chapter: chapter.index }, "+ Add a scene", `Move to the end of ${chapter.heading}`)}
          </div>
        </section>
      {/each}
    {/each}

    <section class:drop-end={mark?.at === "end"} data-zone="end">
      <div class="cards">
        {@render addCard("end", "+ Add a scene in a new chapter", "Move into a new chapter at the end")}
      </div>
    </section>

    {#if book.unplaced.length || movingScene}
      <section class:drop-end={mark?.at === "unplaced"} data-zone="unplaced">
        <h2>Unplaced <span class="muted">· not in the contents yet</span></h2>
        <div class="cards">
          {#each book.unplaced as scene (scene.path)}
            <div class="slot">{@render card(scene)}</div>
          {/each}
          {@render addCard("unplaced", "+ Add an unplaced scene", "Take it out of the contents")}
        </div>
      </section>
    {/if}
  </div>

  {#if deleting}
    <DeleteChapter chapter={deleting} close={() => (deleting = null)} />
  {/if}

  {#if dragging}
    <div
      class="ghost"
      style:left="{ghost.x - ghost.dx}px"
      style:top="{ghost.y - ghost.dy}px"
      style:width={dragging.kind === "scene" ? `${ghost.width}px` : null}
      aria-hidden="true"
    >
      {#if dragging.kind === "scene"}
        <div class="card">{@render face(dragging.scene)}</div>
      {:else}
        <div class="chapter-ghost">
          {dragging.chapter.heading} <span class="muted">· {plural(dragging.chapter.scenes.length, "scene")}</span>
        </div>
      {/if}
    </div>
  {/if}
{/if}

<style>
  .view {
    height: 100%;
    overflow-y: auto;
    padding: 2rem 2.5rem 4rem;
  }

  header {
    margin-bottom: 1.5rem;
  }

  h1 {
    font-size: 26px;
    margin-bottom: 0.3rem;
  }

  header p {
    margin: 0;
    max-width: 75ch;
  }

  section {
    position: relative;
    margin-bottom: 1.6rem;
    border-radius: 12px;
    transition: opacity 120ms;
  }

  h2 {
    font-size: 15px;
    margin-bottom: 0.7rem;
  }

  h2 .muted {
    font-weight: normal;
    font-size: 13px;
  }

  h2.chapter {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    margin-left: -1.25rem;
    cursor: grab;
    user-select: none;
  }

  /* Two columns of dots, shown on hover or focus: the handle for moving a chapter. */
  .grip {
    flex: none;
    width: 0.9rem;
    height: 1.1rem;
    padding: 0;
    border: none;
    border-radius: 3px;
    background-color: transparent;
    background-image: radial-gradient(circle, var(--muted) 1.4px, transparent 1.6px);
    background-size: 5px 5px;
    background-position: 1px 1px;
    opacity: 0;
    cursor: grab;
  }

  h2.chapter:hover .grip,
  .grip:focus-visible {
    opacity: 1;
  }

  .remove {
    margin-left: 0.4rem;
    padding: 0 0.4rem;
    border: none;
    border-radius: 4px;
    background: none;
    color: var(--muted);
    font-size: 12px;
    font-weight: normal;
    opacity: 0;
    cursor: pointer;
  }

  h2.chapter:hover .remove,
  .remove:focus-visible {
    opacity: 1;
  }

  .remove:hover {
    color: var(--danger);
    background: var(--danger-soft);
  }

  h2.part {
    font-size: 12px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
    margin: 2rem 0 0.9rem;
    padding-bottom: 0.4rem;
    border-bottom: 1px solid var(--line);
  }

  h2.part.drop-into {
    color: var(--accent);
    border-bottom: 3px solid var(--accent);
    padding-bottom: calc(0.4rem - 2px);
  }

  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
    gap: 1rem;
  }

  .slot {
    display: grid;
  }

  .card {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    min-height: 170px;
    padding: 0.9rem 1rem;
    text-align: left;
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: 10px;
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.04);
  }

  .card:hover {
    border-color: var(--accent);
  }

  .card.scene {
    user-select: none;
  }

  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.5rem;
  }

  .when {
    color: var(--muted);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  h3 {
    font-size: 17px;
    line-height: 1.3;
  }

  .synopsis {
    flex: 1;
    margin: 0;
    font-family: var(--serif);
    color: var(--text-soft);
    line-height: 1.5;
    display: -webkit-box;
    -webkit-line-clamp: 6;
    line-clamp: 6;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .synopsis.none {
    color: var(--muted);
    font-style: italic;
  }

  .foot {
    font-size: 12px;
    color: var(--muted);
  }

  .card.new {
    align-items: center;
    justify-content: center;
    border-style: dashed;
    background: transparent;
    color: var(--muted);
    min-height: 90px;
  }

  /* ---- Moving ---- */

  .card.lifted,
  section.lifted {
    opacity: 0.35;
  }

  .card.lifted {
    border-style: dashed;
    box-shadow: none;
  }

  /* A bar in the gap beside a card, or above or below a chapter, where the drop would go. */
  .card.drop-before::before,
  .card.drop-after::after,
  section.drop-before::before,
  section.drop-after::after {
    content: "";
    position: absolute;
    background: var(--accent);
    border-radius: 2px;
    pointer-events: none;
  }

  .card.drop-before::before,
  .card.drop-after::after {
    top: 0.4rem;
    bottom: 0.4rem;
    width: 3px;
  }

  .card.drop-before::before {
    left: calc(-0.5rem - 2px);
  }

  .card.drop-after::after {
    right: calc(-0.5rem - 2px);
  }

  section.drop-before::before,
  section.drop-after::after {
    left: -0.5rem;
    right: 0;
    height: 3px;
  }

  section.drop-before::before {
    top: calc(-0.8rem - 2px);
  }

  section.drop-after::after {
    bottom: calc(-0.8rem - 2px);
  }

  section.drop-end .card.new {
    border-color: var(--accent);
    border-style: solid;
    color: var(--accent);
    background: var(--accent-soft);
  }

  .ghost {
    position: fixed;
    z-index: 50;
    pointer-events: none;
    transform: rotate(1.5deg);
  }

  .ghost .card {
    border-color: var(--accent);
    box-shadow: 0 10px 28px rgb(0 0 0 / 0.18);
  }

  .chapter-ghost {
    padding: 0.5rem 0.85rem;
    font-weight: 600;
    white-space: nowrap;
    background: var(--paper);
    border: 1px solid var(--accent);
    border-radius: 8px;
    box-shadow: 0 10px 28px rgb(0 0 0 / 0.18);
  }

  .chapter-ghost .muted {
    font-weight: normal;
  }

  :global(body.moving),
  :global(body.moving *) {
    cursor: grabbing !important;
    user-select: none;
  }

  @media (prefers-reduced-motion: reduce) {
    .ghost {
      transform: none;
    }
  }
</style>
