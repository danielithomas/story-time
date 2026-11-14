// Typed wrappers around the Rust commands. Argument names are camelCase here and snake_case in Rust;
// Tauri converts between them.

import { invoke } from "@tauri-apps/api/core";
import type {
  AddedNote,
  BibleKind,
  BookProgress,
  ChapterSpot,
  CheckReport,
  DeletedBook,
  DeletedChapter,
  Choices,
  CompileResult,
  CreatedBook,
  FolderInfo,
  Idea,
  NoteDoc,
  NoteLinks,
  RelocatedNote,
  RenamedNote,
  ProjectSummary,
  ProjectTree,
  PropertyValue,
  SaveResult,
  ScenePlace,
  SceneSpot,
  SettingsInfo,
  TodayInfo,
  VaultInfo,
} from "./types";

export const api = {
  vaultStatus: () => invoke<VaultInfo | null>("vault_status"),
  inspectFolder: (path: string) => invoke<FolderInfo>("inspect_folder", { path }),
  defaultBookshelfPlace: () => invoke<string>("default_bookshelf_place"),
  createBookshelf: (place: string, name: string, example: boolean) =>
    invoke<VaultInfo>("create_bookshelf", { place, name, example }),
  openVault: (path: string) => invoke<VaultInfo>("open_vault", { path }),
  /** Returns where the backup of the folder as it was is kept. */
  upgradeVault: () => invoke<string>("upgrade_vault"),
  /** Returns the copy made, if one was. */
  backUp: (daily: boolean) => invoke<string | null>("back_up", { daily }),
  settings: () => invoke<SettingsInfo>("settings"),
  saveSettings: (args: { dailyGoal: number; author: string; contact: string[]; paper: string; spelling: string[] }) =>
    invoke<VaultInfo>("save_settings", args),
  /** `folder` null means Story Time's data folder. */
  setBackup: (on: boolean, folder: string | null) => invoke<SettingsInfo>("set_backup", { on, folder }),
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
  /** Moves a scene, and returns whether anything moved. */
  moveScene: (slug: string, book: string | null, baseHash: string, scene: string, to: SceneSpot) =>
    invoke<boolean>("move_scene", { slug, book, baseHash, scene, to }),
  /** Moves a chapter, and returns whether anything moved. */
  moveChapter: (slug: string, book: string | null, baseHash: string, chapter: number, to: ChapterSpot) =>
    invoke<boolean>("move_chapter", { slug, book, baseHash, chapter, to }),
  /** Deletes a chapter: its scenes stay in the book, unplaced, or with `withScenes` go to the system trash too. */
  deleteChapter: (slug: string, book: string | null, baseHash: string, chapter: number, withScenes: boolean) =>
    invoke<DeletedChapter>("delete_chapter", { slug, book, baseHash, chapter, withScenes }),
  /** Undoes the book's latest move, and returns what moved back, or null if there was none to undo. */
  undoMove: (slug: string, book: string | null) => invoke<string | null>("undo_move", { slug, book }),
  renameNote: (path: string, name: string | null, open: string | null) =>
    invoke<RenamedNote>("rename_note", { path, name, open }),
  linksToNote: (path: string) => invoke<NoteLinks>("links_to_note", { path }),
  /** Deletes a book: its scenes move to its world's scratchpad unless `withScenes`, and the rest goes to the trash. */
  deleteBook: (slug: string, book: string, withScenes: boolean) =>
    invoke<DeletedBook>("delete_book", { slug, book, withScenes }),
  /** Sends a world's whole folder to the system trash, and returns its title. */
  deleteProject: (slug: string) => invoke<string>("delete_project", { slug }),
  /** Moves a note to the system trash, and returns its name. */
  deleteNote: (path: string) => invoke<string>("delete_note", { path }),
  cutScene: (path: string) => invoke<RelocatedNote>("cut_scene", { path }),
  /** Moves a scrap into a book as an unplaced scene: a cut scene as it was, any other under `name` or its own. */
  useScrap: (path: string, slug: string, book: string | null, name: string | null) =>
    invoke<RelocatedNote>("use_scrap", { path, slug, book, name }),
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
