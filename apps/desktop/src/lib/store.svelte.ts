// App state and the actions that change it.
//
// The open note's text lives in the editor and in `body` below rather than in reactive state, so
// typing doesn't re-render the interface. Saves happen a moment after typing stops, run one at a
// time, and never overwrite a file that changed on disk since it was read: that becomes a
// conflict for the writer to resolve.

import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { api } from "./api";
import { formatNumber, plural } from "./format";
import type {
  BibleKind,
  ChapterRef,
  ChapterSpot,
  Choices,
  FolderInfo,
  NoteDoc,
  NoteRef,
  ProjectSummary,
  ProjectTree,
  PropertyValue,
  SceneRef,
  ScenePlace,
  SceneSpot,
  TodayInfo,
  VaultInfo,
} from "./types";

/** The views; Settings isn't one of the tabs, and opens from the bookshelf's name or Ctrl+,. */
export type View = "write" | "board" | "progress" | "check" | "ideas" | "settings";

/** The app's colours: the system's, or always light or dark. Kept for this computer only. */
export type Theme = "system" | "light" | "dark";
const THEME_KEY = "story-time:theme";

export const VIEWS: { key: View; label: string; needsProject: boolean }[] = [
  { key: "write", label: "Write", needsProject: true },
  { key: "board", label: "Board", needsProject: true },
  { key: "progress", label: "Progress", needsProject: true },
  { key: "check", label: "Check", needsProject: true },
  { key: "ideas", label: "Ideas", needsProject: false },
];

export type SaveState = "saved" | "unsaved" | "saving" | "conflict" | "error";

interface Toast {
  id: number;
  message: string;
  tone: "info" | "error";
  action?: { label: string; run: () => void };
}

export const app = $state({
  ready: false,
  theme: (localStorage.getItem(THEME_KEY) ?? "system") as Theme,
  vault: null as VaultInfo | null,
  /** The welcome screen's form for starting a bookshelf is showing. */
  starting: false,
  /** A folder holding other things, waiting for the writer to say it may be set up. */
  pendingFolder: null as FolderInfo | null,
  choices: null as Choices | null,
  projects: [] as ProjectSummary[],
  /** The open project, and the book open in it. */
  slug: null as string | null,
  book: null as string | null,
  tree: null as ProjectTree | null,
  doc: null as NoteDoc | null,
  /** Bumped whenever the editor should load `doc.body` afresh. */
  docVersion: 0,
  docWords: 0,
  saveState: "saved" as SaveState,
  view: "write" as View,
  focus: false,
  today: null as TodayInfo | null,
  /** Set while the new-project dialog is open: for a new project, or a new book in the open one. */
  newProject: null as { title: string; idea: string | null; mode: "project" | "book" } | null,
  toasts: [] as Toast[],
});

const SAVE_DELAY = 800;
const PROPERTY_DELAY = 500;
const COUNT_DELAY = 300;
const REFRESH_DELAY = 1500;
const SNAPSHOT_EVERY = 10 * 60 * 1000;

let body = "";
let bodyDirty = false;
let pendingProperties: Record<string, PropertyValue> = {};
let saveQueue: Promise<void> = Promise.resolve();
let saveTimer: ReturnType<typeof setTimeout> | undefined;
let countTimer: ReturnType<typeof setTimeout> | undefined;
let refreshTimer: ReturnType<typeof setTimeout> | undefined;

const hasPending = () => bodyDirty || Object.keys(pendingProperties).length > 0;

// ---- Starting up and choosing a folder ----

export async function init() {
  applyTheme();
  try {
    app.choices = await api.choices();
    app.vault = await api.vaultStatus();
    if (app.vault && !app.vault.needsUpgrade) await enterVault();
  } catch (error) {
    toastError(error);
  }
  app.ready = true;
  if (import.meta.env.DEV) void import("./dev").then((dev) => dev.run());

  setInterval(() => {
    if (app.vault && !app.vault.needsUpgrade) api.recordSnapshot().catch(() => {});
    void backUp(true);
  }, SNAPSHOT_EVERY);
  window.addEventListener("focus", () => void reloadIfChangedElsewhere());
  window.addEventListener("keydown", onKey);
  await getCurrentWindow().onCloseRequested(async (event) => {
    await save();
    if (app.saveState === "conflict" || app.saveState === "error") {
      event.preventDefault();
      toast("This note hasn't been saved yet. Resolve the problem above, then close again.", undefined, "error");
      return;
    }
    if (app.vault && !app.vault.needsUpgrade) await api.recordSnapshot().catch(() => {});
    await backUp(false);
  });
}

let backupError = "";

/**
 * Backs up the open bookshelf, if this computer does: with `daily`, only when there's no copy from
 * today yet. A failure is shown once, not every time it recurs.
 */
async function backUp(daily: boolean) {
  if (!app.vault || app.vault.needsUpgrade) return;
  try {
    await api.backUp(daily);
    backupError = "";
  } catch (error) {
    const message = `Couldn't back up this bookshelf: ${error}`;
    if (message !== backupError) toast(message, undefined, "error");
    backupError = message;
  }
}

async function enterVault() {
  await loadProjects();
  const last = remembered("project");
  const slug = app.projects.find((p) => p.slug === last)?.slug ?? app.projects[0]?.slug;
  if (slug) {
    await selectProject(slug);
  } else {
    app.slug = null;
    app.book = null;
    app.tree = null;
    clearDoc();
  }
  await refreshToday();
  void backUp(true);
}

export function setTheme(theme: Theme) {
  app.theme = theme;
  localStorage.setItem(THEME_KEY, theme);
  applyTheme();
}

function applyTheme() {
  if (app.theme === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = app.theme;
}

/** Saves the bookshelf's settings, then shows the new daily goal. */
export async function saveSettings(settings: { dailyGoal: number; author: string; spelling: string[] }) {
  try {
    app.vault = await api.saveSettings(settings);
    await refreshToday();
  } catch (error) {
    toastError(error);
  }
}

/** Opens a bookshelf the writer picks. A folder holding other things waits for them to agree first. */
export async function chooseVault() {
  const picked = await open({ directory: true, multiple: false, title: "Open a bookshelf" });
  if (typeof picked !== "string") return;
  try {
    const folder = await api.inspectFolder(picked);
    if (folder.state === "occupied") app.pendingFolder = folder;
    else await openFolder(picked);
  } catch (error) {
    toastError(error);
  }
}

/** Sets up and opens the folder the writer agreed to. */
export async function setUpPendingFolder() {
  const folder = app.pendingFolder;
  app.pendingFolder = null;
  if (folder) await openFolder(folder.path);
}

async function openFolder(path: string) {
  if (!(await settle())) return;
  try {
    app.vault = await api.openVault(path);
    app.view = "write";
    if (!app.vault.needsUpgrade) await enterVault();
  } catch (error) {
    toastError(error);
  }
}

/** Asks where a new bookshelf should go, starting from `current`. */
export async function chooseBookshelfPlace(current: string): Promise<string | null> {
  const picked = await open({
    directory: true,
    multiple: false,
    defaultPath: current || undefined,
    title: "Where should the bookshelf go?",
  });
  return typeof picked === "string" ? picked : null;
}

/**
 * Makes a new bookshelf, with the example world if asked, and opens it. Errors are left to the
 * caller, which shows them in its form.
 */
export async function startBookshelf(place: string, name: string, example: boolean) {
  if (!(await settle())) return;
  app.vault = await api.createBookshelf(place, name, example);
  app.starting = false;
  app.view = "write";
  await enterVault();
}

/** Upgrades a folder in story-time 0.1's layout, then opens it. */
export async function upgradeVault() {
  try {
    const backup = await api.upgradeVault();
    app.vault = await api.vaultStatus();
    await enterVault();
    toast(`Upgraded. The bookshelf as it was is kept in ${backup}.`);
  } catch (error) {
    toastError(error);
  }
}

// ---- Projects and notes ----

async function loadProjects() {
  app.projects = await api.listProjects();
}

/**
 * Opens a project, with `book` (else the book last open in it, else its first) open in it, and
 * `note` (else the note last open, else the book note) in the editor.
 */
export async function selectProject(slug: string, book: string | null = null, note: string | null = null) {
  if (!(await settle())) return;
  // The message's Undo is for the book open now.
  dismiss(moveToast);
  try {
    app.slug = slug;
    remember("project", slug);
    app.tree = await api.projectTree(slug, book ?? remembered(`book:${slug}`));
    app.book = app.tree.book?.summary.slug ?? null;
    if (app.book) remember(`book:${slug}`, app.book);
    const last = remembered(`note:${slug}`);
    const known = allNotes(app.tree).some((note) => note.path === last);
    const start = app.tree.book?.note.path ?? app.tree.projectNote.path;
    await openNote(note ?? (known && last && !book ? last : start), false);
  } catch (error) {
    toastError(error);
  }
}

export async function selectBook(book: string) {
  if (app.slug) await selectProject(app.slug, book);
}

export function allNotes(tree: ProjectTree): NoteRef[] {
  const book = tree.book;
  return [
    tree.projectNote,
    ...(book ? [book.note, ...(book.flowNote ? [book.flowNote] : []), ...bookScenes(tree)] : []),
    ...tree.characters,
    ...tree.locations,
    ...tree.world,
    ...(tree.timeline ? [tree.timeline] : []),
    ...tree.research,
    ...tree.scratchpad,
    ...tree.vaultResearch,
    ...tree.vaultScratchpad,
  ];
}

/** Every scene in the open book: those in its contents, in order, then the unplaced. */
export function bookScenes(tree: ProjectTree | null): SceneRef[] {
  const book = tree?.book;
  if (!book) return [];
  return [...book.parts.flatMap((part) => part.chapters.flatMap((chapter) => chapter.scenes)), ...book.unplaced];
}

/** A link to a note, written so that both Story Time and Obsidian follow it. */
export function linkFrom(from: string, to: string): Promise<string> {
  return api.linkFor(from, to);
}

/** Opens a note in the editor, saving the current one first. */
export async function openNote(path: string, show = true) {
  if (app.doc?.path !== path) {
    if (!(await settle())) return;
    try {
      setDoc(await api.readNote(path));
      if (app.slug) remember(`note:${app.slug}`, path);
    } catch (error) {
      toastError(error);
      return;
    }
  }
  if (show) app.view = "write";
}

function setDoc(doc: NoteDoc) {
  app.doc = doc;
  body = doc.body;
  bodyDirty = false;
  pendingProperties = {};
  app.docWords = doc.words;
  app.saveState = "saved";
  app.docVersion++;
}

function clearDoc() {
  app.doc = null;
  body = "";
  bodyDirty = false;
  pendingProperties = {};
  app.docWords = 0;
  app.saveState = "saved";
  app.docVersion++;
}

export function openNewProject(title = "", idea: string | null = null) {
  app.newProject = { title, idea, mode: "project" };
}

export function openNewBook() {
  if (app.slug) app.newProject = { title: "", idea: null, mode: "book" };
}

export async function createProject(args: {
  title: string;
  world: string | null;
  form: string;
  target: number | null;
  structure: string;
  fromIdea: string | null;
}) {
  if (!(await settle())) return;
  const created = await api.createProject(args);
  app.newProject = null;
  await loadProjects();
  await selectProject(created.project, created.book, created.scene);
  app.view = "write";
  toast(`Started ${created.title}.`);
}

/** Adds a book to the open project. Story Time adds it to the project note's list of books. */
export async function createBook(args: { title: string; form: string; target: number | null; structure: string }) {
  if (!app.slug || !(await settle())) return;
  const created = await api.createBook({ slug: app.slug, ...args });
  app.newProject = null;
  await loadProjects();
  await selectProject(created.project, created.book, created.scene);
  app.view = "write";
  toast(`Added ${created.title}.`);
}

export async function addNote(kind: BibleKind, name: string) {
  if (!app.slug) return;
  const slug = app.slug;
  await added(() => api.addNote(slug, kind, name));
}

/** Adds a scene to the open book. Story Time lists it in the book's contents unless it is unplaced. */
export async function addScene(name: string, place: ScenePlace) {
  if (!app.slug) return;
  const [slug, book] = [app.slug, app.book];
  await added(() => api.addScene(slug, book, name, place));
}

/** Adds an empty scrap to the open project's scratchpad, or the vault's, and opens it. */
export async function addScrap(inProject: boolean) {
  const slug = inProject ? app.slug : null;
  await added(() => api.addScrap(slug));
}

/** Adds a research note to the open project, or the vault. */
export async function addResearch(inProject: boolean, name: string) {
  const slug = inProject ? app.slug : null;
  await added(() => api.addResearch(slug, name));
}

async function added(add: () => Promise<{ path: string; name: string; sameName: string | null }>) {
  // Adding a scene rewrites the book note's contents, so any edits to it are saved first.
  if (!app.slug || !(await settle())) return;
  try {
    const note = await add();
    app.tree = await api.projectTree(app.slug, app.book);
    await openNote(note.path);
    if (note.sameName) {
      toast(`${note.sameName} has the same name, so links to "${note.name}" may go to either note.`);
    }
  } catch (error) {
    toastError(error);
  }
}

/**
 * Renames a scene, world-bible note or research note: its name, its file name and every link to it change together.
 * With no name, its file is renamed to match its name. Returns whether it was renamed.
 */
export async function renameNote(path: string, name: string | null): Promise<boolean> {
  if (!(await settle())) return false;
  try {
    const renamed = await api.renameNote(path, name);
    if (app.slug) app.tree = await api.projectTree(app.slug, app.book);
    const open = app.doc;
    if (open?.path === path) {
      setDoc(await api.readNote(renamed.path));
      if (app.slug) remember(`note:${app.slug}`, renamed.path);
    } else if (open && renamed.notes) {
      // The open note may have linked to the renamed one, and so have changed on disk.
      const fresh = await api.readNote(open.path);
      if (fresh.hash !== open.hash && app.doc?.path === open.path && !hasPending()) setDoc(fresh);
    }
    const file = renamed.path.split(/[\\/]/).pop();
    const done = name === null ? `Renamed the file to ${file}.` : `Renamed to ${renamed.name}.`;
    const links = renamed.links ? ` Updated ${plural(renamed.links, "link")} in ${plural(renamed.notes, "note")}.` : "";
    toast(done + links);
    if (renamed.sameName) {
      toast(`${renamed.sameName} has the same file name, so links to it may open either note in Obsidian.`);
    }
    return true;
  } catch (error) {
    toastError(error);
    return false;
  }
}

// ---- Moving scenes and chapters on the Board ----

/** The message about the latest move, which offers to undo it. */
let moveToast = 0;

/** Saves the open note, then reloads the open project's notes, so the Board shows the contents as they are now. */
export async function refreshTree() {
  await save();
  if (!app.slug) return;
  try {
    app.tree = await api.projectTree(app.slug, app.book);
  } catch (error) {
    toastError(error);
  }
}

/** Moves a scene in the open book's contents: within its chapter, into another, or in or out of the contents. */
export async function moveScene(scene: SceneRef, to: SceneSpot) {
  await moved(
    (slug, book, hash) => api.moveScene(slug, book, hash, scene.path, to),
    () => {
      const chapters = app.tree?.book?.parts.flatMap((part) => part.chapters) ?? [];
      const chapter = chapters.find((c) => c.scenes.some((s) => s.path === scene.path));
      return chapter ? `Moved ${scene.title} to ${chapter.heading}.` : `${scene.title} is unplaced now.`;
    },
  );
}

/** Moves a chapter of the open book, with its scenes. */
export async function moveChapter(chapter: ChapterRef, to: ChapterSpot) {
  await moved(
    (slug, book, hash) => api.moveChapter(slug, book, hash, chapter.index, to),
    () => `Moved ${chapter.heading}.`,
  );
}

/**
 * Runs a move, which rewrites the book note's contents, so edits are saved first. Story Time refuses the move if
 * the book note changed since the Board showed it (through a sync service, say), and the Board then catches up.
 */
async function moved(
  move: (slug: string, book: string | null, hash: string) => Promise<boolean>,
  describe: () => string,
) {
  const tree = app.tree;
  if (!app.slug || !tree?.book || !(await settle())) return;
  const [slug, book] = [app.slug, app.book];
  try {
    const done = await move(slug, book, tree.book.hash);
    await contentsChanged(tree.book.note.path);
    if (done) showMoveToast(describe(), true);
  } catch (error) {
    toastError(error);
    await refreshTree();
  }
}

/**
 * Undoes the open book's latest move on the Board, putting its contents back as they were before it. Each book's
 * moves are kept on this computer, in Story Time's data folder, so they can be undone after switching books or
 * reopening the app. Story Time refuses if the book note has changed since it last left it, and then forgets them.
 */
export async function undoMove() {
  const tree = app.tree;
  if (!app.slug || !tree?.book || !(await settle())) return;
  try {
    const what = await api.undoMove(app.slug, app.book);
    if (what === null) {
      showMoveToast("There's no move to undo in this book.", false);
      return;
    }
    await contentsChanged(tree.book.note.path);
    showMoveToast(`Put ${what} back.`, false);
  } catch (error) {
    dismiss(moveToast);
    toastError(error);
    await refreshTree();
  }
}

/** After the book note's contents were rewritten: shows them, and reloads the note if it is open. */
async function contentsChanged(bookNote: string) {
  if (!app.slug) return;
  app.tree = await api.projectTree(app.slug, app.book);
  if (app.doc?.path === bookNote && !hasPending()) setDoc(await api.readNote(bookNote));
}

/** One message about moves at a time: each replaces the last, so its Undo is always for the latest move. */
function showMoveToast(message: string, undoable: boolean) {
  dismiss(moveToast);
  moveToast = toast(message, undoable ? { label: "Undo", run: () => void undoMove() } : undefined);
}

// ---- Editing and saving ----

/** Called by the editor on every change. */
export function edited(text: string) {
  body = text;
  bodyDirty = true;
  if (app.saveState !== "conflict") app.saveState = "unsaved";
  scheduleSave(SAVE_DELAY);
  const path = app.doc?.path;
  clearTimeout(countTimer);
  countTimer = setTimeout(async () => {
    const words = await api.countWords(body).catch(() => app.docWords);
    if (app.doc?.path === path) app.docWords = words;
  }, COUNT_DELAY);
}

export function setProperty(key: string, value: PropertyValue) {
  if (!app.doc) return;
  app.doc.properties[key] = value;
  pendingProperties[key] = value;
  if (app.saveState !== "conflict") app.saveState = "unsaved";
  scheduleSave(PROPERTY_DELAY);
}

function scheduleSave(delay: number) {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => void save(), delay);
}

/** Saves pending changes now. Saves run one after another, never in parallel. */
export function save(force = false): Promise<void> {
  clearTimeout(saveTimer);
  saveQueue = saveQueue.then(() => saveOnce(force));
  return saveQueue;
}

async function saveOnce(force: boolean) {
  const doc = app.doc;
  if (!doc || !hasPending()) return;
  if (app.saveState === "conflict" && !force) return;
  const sentBody = bodyDirty ? body : undefined;
  const sentProperties = Object.keys(pendingProperties).length ? { ...pendingProperties } : undefined;
  bodyDirty = false;
  pendingProperties = {};
  app.saveState = "saving";
  const restore = () => {
    if (sentBody !== undefined) bodyDirty = true;
    if (sentProperties) pendingProperties = { ...sentProperties, ...pendingProperties };
  };
  try {
    const result = await api.saveNote({
      path: doc.path,
      body: sentBody,
      properties: sentProperties,
      baseHash: doc.hash,
      force,
    });
    if (result.conflict) {
      restore();
      app.saveState = "conflict";
      return;
    }
    doc.hash = result.hash;
    if (sentBody !== undefined) doc.body = sentBody;
    if (!bodyDirty) app.docWords = result.words;
    app.saveState = hasPending() ? "unsaved" : "saved";
    if (hasPending()) scheduleSave(SAVE_DELAY);
    scheduleRefresh();
  } catch (error) {
    restore();
    app.saveState = "error";
    toastError(error);
  }
}

/** Saves, and reports whether it is safe to move on (no conflict or failed save). */
async function settle(): Promise<boolean> {
  await save();
  if (app.saveState === "conflict") {
    toast("Resolve the conflict on this note first.", undefined, "error");
    return false;
  }
  if (app.saveState === "error") {
    toast("This note couldn't be saved. Try again with Ctrl+S.", undefined, "error");
    return false;
  }
  return true;
}

/** Resolves a conflict by writing this version over the one on disk. */
export async function keepMine() {
  await save(true);
}

/** Resolves a conflict by loading the version on disk, discarding unsaved changes here. */
export async function takeTheirs() {
  if (!app.doc) return;
  try {
    setDoc(await api.readNote(app.doc.path));
    scheduleRefresh();
  } catch (error) {
    toastError(error);
  }
}

/** When the window regains focus, pick up edits made in Obsidian or by a sync service. */
async function reloadIfChangedElsewhere() {
  const doc = app.doc;
  if (doc && !hasPending() && app.saveState === "saved") {
    try {
      const fresh = await api.readNote(doc.path);
      if (fresh.hash !== doc.hash && app.doc?.path === doc.path && !hasPending()) {
        setDoc(fresh);
        toast("Reloaded this note: it changed outside Story Time.");
      }
    } catch {
      // Moved or deleted elsewhere; leave what is on screen.
    }
  }
  scheduleRefresh();
}

function scheduleRefresh() {
  clearTimeout(refreshTimer);
  refreshTimer = setTimeout(async () => {
    try {
      if (app.slug) app.tree = await api.projectTree(app.slug, app.book);
      await loadProjects();
      await refreshToday();
    } catch {
      // A failed refresh only leaves counts slightly stale.
    }
  }, REFRESH_DELAY);
}

async function refreshToday() {
  app.today = await api.writingToday();
}

// ---- Export, focus, keys, messages ----

export async function compile(format: string) {
  if (!app.slug || !(await settle())) return;
  try {
    const result = await api.compileBook(app.slug, app.book, format);
    const skipped = result.skipped.length ? ` Left out ${plural(result.skipped.length, "empty scene")}.` : "";
    const size = `${plural(result.chapters, "chapter")}, ${plural(result.scenes, "scene")}, ${formatNumber(result.words)} words`;
    toast(`Exported ${size}.${skipped}`, {
      label: "Show file",
      run: () => void revealItemInDir(result.path),
    });
  } catch (error) {
    toastError(error);
  }
}

export async function toggleFocus(on = !app.focus) {
  app.focus = on;
  if (on) app.view = "write";
  try {
    await getCurrentWindow().setFullscreen(on);
  } catch {
    // Full screen is a nicety; focus mode works without it.
  }
}

function onKey(event: KeyboardEvent) {
  const mod = event.ctrlKey || event.metaKey;
  if (mod && event.key.toLowerCase() === "s") {
    event.preventDefault();
    void save();
  } else if (event.key === "F11" || (mod && event.shiftKey && event.key.toLowerCase() === "f")) {
    event.preventDefault();
    void toggleFocus();
  } else if (event.key === "Escape" && app.focus && !document.querySelector(".cm-panel")) {
    void toggleFocus(false);
  } else if (mod && !event.shiftKey && event.key.toLowerCase() === "z" && app.view === "board" && !isTextField(event.target)) {
    event.preventDefault();
    void undoMove();
  } else if (mod && event.key === ",") {
    event.preventDefault();
    app.view = "settings";
  } else if (mod && !event.shiftKey && /^[1-5]$/.test(event.key)) {
    const view = VIEWS[Number(event.key) - 1];
    if (!view.needsProject || app.slug) {
      event.preventDefault();
      app.view = view.key;
    }
  }
}

function isTextField(target: EventTarget | null) {
  return target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || (target instanceof HTMLElement && target.isContentEditable);
}

let nextToast = 1;

export function toast(message: string, action?: Toast["action"], tone: Toast["tone"] = "info"): number {
  const id = nextToast++;
  app.toasts.push({ id, message, tone, action });
  setTimeout(() => dismiss(id), action || tone === "error" ? 10000 : 5000);
  return id;
}

export function toastError(error: unknown) {
  toast(error instanceof Error ? error.message : String(error), undefined, "error");
}

export function dismiss(id: number) {
  app.toasts = app.toasts.filter((t) => t.id !== id);
}

// Small per-folder memory: the last project, and the last note in each project.
const storageKey = (key: string) => `story-time:${app.vault?.path}:${key}`;
const remembered = (key: string) => localStorage.getItem(storageKey(key));
const remember = (key: string, value: string) => localStorage.setItem(storageKey(key), value);
