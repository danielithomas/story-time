// Shapes returned by the Rust commands in src-tauri/src/commands.rs.

export interface VaultInfo {
  path: string;
  name: string;
  dailyGoal: number;
  /** Laid out as story-time 0.1 did it: it must be upgraded before it opens. */
  needsUpgrade: boolean;
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
  sceneStatuses: string[];
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
  /** Words in all the book's scenes, placed or not. */
  words: number;
  scenes: number;
  chapters: number;
  unplaced: number;
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

export interface SceneRef extends NoteRef {
  status: string | null;
  pov: string | null;
  location: string | null;
  when: string | null;
  synopsis: string | null;
  words: number;
}

export interface ChapterRef {
  /** Its position in the book, counting from 0 across parts. */
  index: number;
  /** Its title, or "Chapter 3". */
  heading: string;
  title: string | null;
  scenes: SceneRef[];
  words: number;
}

export interface PartRef {
  title: string | null;
  chapters: ChapterRef[];
}

/** A note in a scratchpad. Its title is its name, or the first words of its text. */
export interface ScrapRef extends NoteRef {
  created: string | null;
  words: number;
}

/** A project's notes, with one of its books open, and the vault's scratchpad and research. */
export interface ProjectTree {
  project: ProjectSummary;
  projectNote: NoteRef;
  book: BookTree | null;
  characters: NoteRef[];
  locations: NoteRef[];
  world: NoteRef[];
  timeline: NoteRef | null;
  research: NoteRef[];
  scratchpad: ScrapRef[];
  vaultResearch: NoteRef[];
  vaultScratchpad: ScrapRef[];
}

/** A book's notes, with its scenes as its contents list them. */
export interface BookTree {
  summary: BookSummary;
  note: NoteRef;
  flowNote: NoteRef | null;
  hasParts: boolean;
  parts: PartRef[];
  unplaced: SceneRef[];
}

export type PropertyValue = string | string[];

export type NoteKind =
  | "project"
  | "book"
  | "flow"
  | "scene"
  | "character"
  | "location"
  | "world"
  | "timeline"
  | "research"
  | "scrap"
  | "note";

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
  /** Words in the placed scenes: the manuscript. */
  placedWords: number;
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
  statuses: { status: string; scenes: number; words: number }[];
  characters: number;
  locations: number;
  world: number;
  history: { date: string; words: number }[];
}

export interface CheckReport {
  undocumented: { name: string; sources: string[] }[];
  ambiguousLinks: { name: string; sources: string[] }[];
  aliasLinks: { name: string; sources: string[] }[];
  brokenEntries: { name: string; sources: string[] }[];
  repeated: string[];
  notOnPage: { kind: string; name: string }[];
  misnamed: { path: string; to: string }[];
  drifted: { path: string; to: string }[];
  withoutStatus: string[];
  withoutSynopsis: string[];
  emptyScenes: string[];
  unreadable: { path: string; error: string }[];
}

export interface CompileResult {
  path: string;
  chapters: number;
  scenes: number;
  words: number;
  skipped: string[];
}

export interface Idea {
  section: string;
  name: string;
  text: string;
  project: string | null;
}

/** Kinds of world-bible note that can be added to a project. */
export type BibleKind = "character" | "location" | "world";

/** Where a new scene goes: at the end of a chapter (by index), at the end of the contents as a chapter of its own, or unplaced. */
export type ScenePlace = { chapter: number } | "end" | "unplaced";
