// Development-only helpers for checking screens without clicking through them. Loaded by
// init() only when running `npm run tauri dev`; release builds never include this file.
//
//   VITE_START_VIEW=board npm run tauri dev         open straight into a view (or focus, new-project, new-book)
//   VITE_SNAPSHOT_DIR=/tmp/shots npm run tauri dev  save a PNG of every screen, typing a test
//                                                   sentence into the open note along the way
//   VITE_THEME=light npm run tauri dev              use the light palette whatever the system says

import { invoke } from "@tauri-apps/api/core";
import { EditorView } from "@codemirror/view";
import { app, bookScenes, openNewBook, openNewProject, openNote, toggleFocus, VIEWS, type View } from "./store.svelte";

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export async function run() {
  const env = import.meta.env;
  if (env.VITE_THEME) document.documentElement.dataset.theme = env.VITE_THEME;
  if (env.VITE_SNAPSHOT_DIR && (!app.vault || app.vault.needsUpgrade)) {
    await welcomeTour(env.VITE_SNAPSHOT_DIR);
    return;
  }
  if (!app.vault || app.vault.needsUpgrade) return;
  if (env.VITE_SNAPSHOT_DIR) await snapshotTour(env.VITE_SNAPSHOT_DIR);
  else if (env.VITE_START_VIEW) await startView(env.VITE_START_VIEW);
}

/** The screens before a bookshelf opens: welcome, starting one, and asking before setting one up. */
async function welcomeTour(dir: string) {
  const shot = async (name: string) => {
    await wait(800);
    await invoke("dev_snapshot", { path: `${dir}/${name}.png` });
  };
  await wait(700);
  if (app.vault) return shot("upgrade");
  await shot("welcome");
  app.starting = true;
  await shot("start");
  app.starting = false;
  app.pendingFolder = { path: "/home/writer/Documents", name: "Documents", state: "occupied", adds: [".story-time/", "templates/"] };
  await shot("set-up");
  app.pendingFolder = null;
}

async function startView(start: string) {
  if (start === "focus") await toggleFocus(true);
  else if (start === "new-project") openNewProject();
  else if (start === "new-book") openNewBook();
  else if (VIEWS.some((view) => view.key === start)) app.view = start as View;
}

/** Types at the end of the open note through CodeMirror, exercising the same path as a keyboard. */
function type(text: string) {
  const dom = document.querySelector<HTMLElement>(".cm-editor");
  const view = dom && EditorView.findFromDOM(dom);
  view?.dispatch({ changes: { from: view.state.doc.length, insert: text } });
}

async function snapshotTour(dir: string) {
  const scenes = bookScenes(app.tree);
  const unplaced = app.tree?.book?.unplaced[0];
  const scrap = app.tree?.scratchpad[0] ?? app.tree?.vaultScratchpad[0];
  const research = app.tree?.research[0] ?? app.tree?.vaultResearch[0];
  const shots: [string, () => unknown][] = [
    ["write", () => (app.view = "write")],
    ["scene", () => scenes[0] && openNote(scenes[0].path)],
    ["typed", () => type("\n\nSnapshot tour sentence with eight words in it.\n")],
    ["unplaced", () => unplaced && openNote(unplaced.path)],
    [
      "scrap",
      async () => {
        if (scrap) await openNote(scrap.path);
        document.querySelector(".binder")?.scrollTo(0, 1e6);
      },
    ],
    ["research", () => research && openNote(research.path)],
    ["conflict", () => (app.saveState = "conflict")],
    [
      "board",
      () => {
        app.saveState = "saved";
        app.view = "board";
      },
    ],
    ["progress", () => (app.view = "progress")],
    ["check", () => (app.view = "check")],
    ["ideas", () => (app.view = "ideas")],
    ["new-project", () => openNewProject()],
    [
      "new-book",
      () => {
        app.newProject = null;
        openNewBook();
      },
    ],
    [
      "focus",
      () => {
        app.newProject = null;
        return toggleFocus(true);
      },
    ],
  ];
  await wait(1500);
  for (const [name, show] of shots) {
    await show();
    await wait(name === "typed" ? 3500 : 1500);
    await invoke("dev_snapshot", { path: `${dir}/${name}.png` });
    await wait(700);
  }
  await toggleFocus(false);
  await wait(2500);
  await invoke("dev_snapshot", { path: `${dir}/done.png` });
}
