// Typed wrappers around the Rust commands. Argument names are camelCase here and snake_case in Rust;
// Tauri converts between them.

import { invoke } from "@tauri-apps/api/core";
import type {
  AddableKind,
  AddedNote,
  CheckReport,
  Choices,
  CompileResult,
  Idea,
  NoteDoc,
  ProjectProgress,
  ProjectSummary,
  ProjectTree,
  PropertyValue,
  SaveResult,
  TodayInfo,
  VaultInfo,
} from "./types";

export const api = {
  vaultStatus: () => invoke<VaultInfo | null>("vault_status"),
  openVault: (path: string) => invoke<VaultInfo>("open_vault", { path }),
  choices: () => invoke<Choices>("choices"),
  listProjects: () => invoke<ProjectSummary[]>("list_projects"),
  projectTree: (slug: string) => invoke<ProjectTree>("project_tree", { slug }),
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
    form: string;
    target: number | null;
    structure: string;
    fromIdea: string | null;
  }) => invoke<ProjectSummary>("create_project", args),
  addNote: (slug: string, kind: AddableKind, name: string) =>
    invoke<AddedNote>("add_note", { slug, kind, name }),
  writingToday: () => invoke<TodayInfo>("writing_today"),
  recordSnapshot: () => invoke<void>("record_snapshot"),
  projectProgress: (slug: string, days: number) =>
    invoke<ProjectProgress>("project_progress", { slug, days }),
  checkProject: (slug: string) => invoke<CheckReport>("check_project", { slug }),
  compileProject: (slug: string, format: string) =>
    invoke<CompileResult>("compile_project", { slug, format }),
  listIdeas: () => invoke<Idea[]>("list_ideas"),
  captureIdea: (text: string, nonFiction: boolean) =>
    invoke<void>("capture_idea", { text, nonFiction }),
};
