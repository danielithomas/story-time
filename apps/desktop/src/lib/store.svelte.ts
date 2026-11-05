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
  AddableKind,
  Choices,
  NoteDoc,
  NoteRef,
  ProjectSummary,
  ProjectTree,
  PropertyValue,
  TodayInfo,
  VaultInfo,
} from "./types";

export type View = "write" | "board" | "progress" | "check" | "ideas";

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
  vault: null as VaultInfo | null,
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
  try {
    app.choices = await api.choices();
    app.vault = await api.vaultStatus();
    if (app.vault) await enterVault();
  } catch (error) {
    toastError(error);
  }
  app.ready = true;
  if (import.meta.env.DEV) void import("./dev").then((dev) => dev.run());

  setInterval(() => {
    if (app.vault) api.recordSnapshot().catch(() => {});
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
    if (app.vault) await api.recordSnapshot().catch(() => {});
  });
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
}

export async function chooseVault() {
  const picked = await open({ directory: true, multiple: false, title: "Choose your stories folder" });
  if (typeof picked !== "string") return;
  if (!(await settle())) return;
  try {
    app.vault = await api.openVault(picked);
    app.view = "write";
    await enterVault();
  } catch (error) {
    toastError(error);
  }
}

// ---- Projects and notes ----

async function loadProjects() {
  app.projects = await api.listProjects();
}

/** Opens a project, with `book` (else the book last open in it, else its first) open in it. */
export async function selectProject(slug: string, book: string | null = null) {
  if (!(await settle())) return;
  try {
    app.slug = slug;
    remember("project", slug);
    app.tree = await api.projectTree(slug, book ?? remembered(`book:${slug}`));
    app.book = app.tree.book?.summary.slug ?? null;
    if (app.book) remember(`book:${slug}`, app.book);
    const last = remembered(`note:${slug}`);
    const known = allNotes(app.tree).some((note) => note.path === last);
    const start = app.tree.book?.note.path ?? app.tree.projectNote.path;
    await openNote(known && last && !book ? last : start, false);
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
    ...(book ? [book.note, ...(book.flowNote ? [book.flowNote] : []), ...book.chapters] : []),
    ...tree.characters,
    ...tree.locations,
    ...tree.world,
  ];
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
  await selectProject(created.project, created.book);
  app.view = "write";
  toast(`Started ${created.title}.`);
}

/** Adds a book to the open project. Story Time adds it to the project note's list of books. */
export async function createBook(args: { title: string; form: string; target: number | null; structure: string }) {
  if (!app.slug || !(await settle())) return;
  const created = await api.createBook({ slug: app.slug, ...args });
  app.newProject = null;
  await loadProjects();
  await selectProject(created.project, created.book);
  app.view = "write";
  toast(`Added ${created.title}.`);
}

export async function addNote(kind: AddableKind, name: string) {
  if (!app.slug) return;
  try {
    const added = await api.addNote(app.slug, app.book, kind, name);
    app.tree = await api.projectTree(app.slug, app.book);
    await openNote(added.path);
    if (added.sameName) {
      toast(`${added.sameName} has the same name, so links to "${added.name}" may go to either note.`);
    }
  } catch (error) {
    toastError(error);
  }
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
    const skipped = result.skipped.length ? ` Left out ${plural(result.skipped.length, "empty chapter")}.` : "";
    toast(`Exported ${plural(result.chapters, "chapter")}, ${formatNumber(result.words)} words.${skipped}`, {
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
  } else if (mod && !event.shiftKey && /^[1-5]$/.test(event.key)) {
    const view = VIEWS[Number(event.key) - 1];
    if (!view.needsProject || app.slug) {
      event.preventDefault();
      app.view = view.key;
    }
  }
}

let nextToast = 1;

export function toast(message: string, action?: Toast["action"], tone: Toast["tone"] = "info") {
  const id = nextToast++;
  app.toasts.push({ id, message, tone, action });
  setTimeout(() => dismiss(id), action || tone === "error" ? 10000 : 5000);
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
