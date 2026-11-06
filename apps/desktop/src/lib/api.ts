// Typed wrappers around the Rust commands. Argument names are camelCase here and snake_case in Rust;
// Tauri converts between them.

import { invoke } from "@tauri-apps/api/core";
import type {
  AddedNote,
  BibleKind,
  BookProgress,
  CheckReport,
  Choices,
  CompileResult,
  CreatedBook,
  Idea,
  NoteDoc,
  ProjectSummary,
  ProjectTree,
  PropertyValue,
  SaveResult,
  ScenePlace,
  TodayInfo,
  VaultInfo,
} from "./types";

export const api = {
  vaultStatus: () => invoke<VaultInfo | null>("vault_status"),
  openVault: (path: string) => invoke<VaultInfo>("open_vault", { path }),
  /** Returns where the backup of the folder as it was is kept. */
  upgradeVault: () => invoke<string>("upgrade_vault"),
  choices: () => invoke<Choices>("choices"),
  listProjects: () => invoke<ProjectSummary[]>("list_projects"),
  projectTree: (slug: string, book: string | null) => invoke<ProjectTree>("project_tree", { slug, book }),
  readNote: (path: string) => invoke<NoteDoc>("read_note", { path }),
  saveNote: (args: {
    path: string;
    body?: string;
    properties?: Record<string, PropertyValue>;
    baseHash: string;
    force?: boolean;
  }) => invoke<SaveResult>("save_note", { ...args, force: args.force ?? false }),
  countWords: (text: string) => invoke<number>("count_words", { text }),
  createProject: (args: {
    title: string;
    world: string | null;
    form: string;
    target: number | null;
    structure: string;
    fromIdea: string | null;
  }) => invoke<CreatedBook>("create_project", args),
  createBook: (args: { slug: string; title: string; form: string; target: number | null; structure: string }) =>
    invoke<CreatedBook>("create_book", args),
  addNote: (slug: string, kind: BibleKind, name: string) => invoke<AddedNote>("add_note", { slug, kind, name }),
  addScene: (slug: string, book: string | null, name: string, place: ScenePlace) =>
    invoke<AddedNote>("add_scene", {
      slug,
      book,
      name,
      chapter: typeof place === "object" ? place.chapter : null,
      unplaced: place === "unplaced",
    }),
  addScrap: (slug: string | null) => invoke<AddedNote>("add_scrap", { slug }),
  addResearch: (slug: string | null, name: string) => invoke<AddedNote>("add_research", { slug, name }),
  linkFor: (from: string, to: string) => invoke<string>("link_for", { from, to }),
  writingToday: () => invoke<TodayInfo>("writing_today"),
  recordSnapshot: () => invoke<void>("record_snapshot"),
  bookProgress: (slug: string, book: string | null, days: number) =>
    invoke<BookProgress>("book_progress", { slug, book, days }),
  checkProject: (slug: string) => invoke<CheckReport>("check_project", { slug }),
  compileBook: (slug: string, book: string | null, format: string) =>
    invoke<CompileResult>("compile_book", { slug, book, format }),
  listIdeas: () => invoke<Idea[]>("list_ideas"),
  captureIdea: (text: string, nonFiction: boolean) =>
    invoke<void>("capture_idea", { text, nonFiction }),
};
