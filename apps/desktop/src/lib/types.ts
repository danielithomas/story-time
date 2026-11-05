// Shapes returned by the Rust commands in src-tauri/src/commands.rs.

export interface VaultInfo {
  path: string;
  name: string;
  dailyGoal: number;
}

export interface Choice {
  key: string;
  label: string;
  detail: string;
}

export interface Choices {
  forms: Choice[];
  structures: Choice[];
  formats: Choice[];
  chapterStatuses: string[];
  bookStatuses: string[];
}

/** A project: a world and the books set in it. */
export interface ProjectSummary {
  slug: string;
  title: string;
  genre: string[];
  books: BookSummary[];
}

export interface BookSummary {
  slug: string;
  title: string;
  status: string | null;
  form: string | null;
  formLabel: string | null;
  target: number | null;
  deadline: string | null;
  words: number;
  chapters: number;
}

export interface CreatedBook {
  project: string;
  book: string;
  title: string;
}

export interface NoteRef {
  path: string;
  name: string;
  title: string;
}

export interface AddedNote extends NoteRef {
  sameName: string | null;
}

export interface ChapterRef extends NoteRef {
  number: number | null;
  status: string | null;
  pov: string | null;
  synopsis: string | null;
  words: number;
}

/** A project's notes, with one of its books open. */
export interface ProjectTree {
  project: ProjectSummary;
  projectNote: NoteRef;
  book: BookTree | null;
  characters: NoteRef[];
  locations: NoteRef[];
  world: NoteRef[];
}

export interface BookTree {
  summary: BookSummary;
  note: NoteRef;
  flowNote: NoteRef | null;
  chapters: ChapterRef[];
}

export type PropertyValue = string | string[];

export type NoteKind = "project" | "book" | "flow" | "chapter" | "character" | "location" | "world" | "note";

export interface NoteDoc {
  path: string;
  name: string;
  kind: NoteKind;
  project: string | null;
  book: string | null;
  title: string;
  body: string;
  properties: Record<string, PropertyValue>;
  hash: string;
  words: number;
}

export interface SaveResult {
  conflict: boolean;
  hash: string;
  words: number;
}

export interface TodayInfo {
  words: number;
  goal: number;
  streak: number;
}

export interface BookProgress {
  book: BookSummary;
  worldTitle: string;
  genre: string[];
  lengthLabel: string | null;
  lengthRange: string | null;
  pages: number;
  readingMinutes: number;
  daysLeft: number | null;
  wordsPerDay: number | null;
  today: number;
  goal: number;
  statuses: { status: string; chapters: number; words: number }[];
  characters: number;
  locations: number;
  world: number;
  history: { date: string; words: number }[];
}

export interface CheckReport {
  undocumented: { name: string; sources: string[] }[];
  ambiguousLinks: { name: string; sources: string[] }[];
  aliasLinks: { name: string; sources: string[] }[];
  notOnPage: { kind: string; name: string }[];
  misnamed: { path: string; to: string }[];
  drifted: { path: string; to: string }[];
  withoutStatus: string[];
  withoutSynopsis: string[];
  emptyChapters: string[];
  unreadable: { path: string; error: string }[];
}

export interface CompileResult {
  path: string;
  chapters: number;
  words: number;
  skipped: string[];
}

export interface Idea {
  section: string;
  name: string;
  text: string;
  project: string | null;
}

/** Note kinds that can be added to a project. */
export type AddableKind = "chapter" | "character" | "location" | "world";
