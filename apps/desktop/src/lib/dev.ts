// Development-only helpers for checking screens without clicking through them. Loaded by
// init() only when running `npm run tauri dev`; release builds never include this file.
//
//   VITE_START_VIEW=board npm run tauri dev         open straight into a view (or focus, new-project, new-book)
//   VITE_SNAPSHOT_DIR=/tmp/shots npm run tauri dev  save a PNG of every screen, typing a test
//                                                   sentence into the open note along the way
//   VITE_THEME=light npm run tauri dev              use the light palette whatever the system says

import { invoke } from "@tauri-apps/api/core";
import { EditorView } from "@codemirror/view";
import { app, openNewBook, openNewProject, toggleFocus, VIEWS, type View } from "./store.svelte";

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export async function run() {
  const env = import.meta.env;
  if (env.VITE_THEME) document.documentElement.dataset.theme = env.VITE_THEME;
  if (env.VITE_SNAPSHOT_DIR && !app.vault) {
    await wait(1500);
    await invoke("dev_snapshot", { path: `${env.VITE_SNAPSHOT_DIR}/welcome.png` });
    return;
  }
  if (!app.vault) return;
  if (env.VITE_SNAPSHOT_DIR) await snapshotTour(env.VITE_SNAPSHOT_DIR);
  else if (env.VITE_START_VIEW) await startView(env.VITE_START_VIEW);
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
  const shots: [string, () => unknown][] = [
    ["write", () => (app.view = "write")],
    ["typed", () => type("\n\nSnapshot tour sentence with eight words in it.\n")],
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
