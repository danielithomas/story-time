//! The commands the interface calls. Each one opens the vault afresh, so edits to its config
//! take effect at once, and reports failures as readable messages.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{Days, Local};
use serde::Serialize;
use serde_json::{Map, Value};
use story_time_core::compile::{self, Format};
use story_time_core::history::History;
use story_time_core::ideas;
use story_time_core::links::LinkIndex;
use story_time_core::note::{self, Note, PropertyValue, Saved};
use story_time_core::project::{BIBLE_DIR, BOOK_STATUSES, BOOKS_DIR, CHAPTER_STATUSES};
use story_time_core::scaffold::{self, NewBook, NewProject};
use story_time_core::structures::{self, STRUCTURES};
use story_time_core::text::thousands;
use story_time_core::vault::{self as vaults, Measured};
use story_time_core::{Book, Chapter, Form, NoteKind, Project, Vault, check, config, words};
use tauri::{State, WebviewWindow};

type CmdResult<T> = Result<T, String>;

/// Turns errors into the messages the interface shows.
trait OrMessage<T> {
    fn or_message(self) -> CmdResult<T>;
}

impl<T> OrMessage<T> for anyhow::Result<T> {
    fn or_message(self) -> CmdResult<T> {
        self.map_err(|e| format!("{e:#}"))
    }
}

impl<T> OrMessage<T> for io::Result<T> {
    fn or_message(self) -> CmdResult<T> {
        self.map_err(|e| e.to_string())
    }
}

/// The stories folder in use. Only its path is kept; the vault is reopened per command.
#[derive(Default)]
pub struct AppState {
    root: Mutex<Option<PathBuf>>,
}

impl AppState {
    pub fn remember(&self, root: PathBuf) {
        *self.root.lock().expect("state lock") = Some(root);
    }

    fn is_open(&self) -> bool {
        self.root.lock().expect("state lock").is_some()
    }

    fn vault(&self) -> CmdResult<Vault> {
        let root = self.root.lock().expect("state lock").clone();
        let root = root.ok_or_else(|| "No stories folder is open.".to_string())?;
        Vault::open(root).or_message()
    }

    fn project(&self, slug: &str) -> CmdResult<(Vault, Project)> {
        let vault = self.vault()?;
        let project = vault.find_project(Some(slug), &vault.root).or_message()?;
        Ok((vault, project))
    }

    /// A project and one of its books: the one named, else the first.
    fn book(&self, slug: &str, book: Option<&str>) -> CmdResult<(Vault, Project, Book)> {
        let (vault, project) = self.project(slug)?;
        let books = project.books().or_message()?;
        let found = match book {
            Some(book) => books.into_iter().find(|b| b.slug == book),
            None => books.into_iter().next(),
        };
        let book = found.ok_or_else(|| {
            format!(
                "{} has no book \"{}\"",
                project.title(),
                book.unwrap_or_default()
            )
        })?;
        Ok((vault, project, book))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultInfo {
    path: String,
    name: String,
    daily_goal: u64,
}

fn vault_info(vault: &Vault) -> VaultInfo {
    VaultInfo {
        path: vault.root.display().to_string(),
        name: vault
            .root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        daily_goal: vault.config.daily_goal,
    }
}

/// A project: a world and its books.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    slug: String,
    title: String,
    genre: Vec<String>,
    books: Vec<BookSummary>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookSummary {
    slug: String,
    title: String,
    status: Option<String>,
    form: Option<String>,
    form_label: Option<String>,
    target: Option<u64>,
    deadline: Option<String>,
    words: u64,
    chapters: usize,
}

fn book_summary(book: &Book, chapters: &[Chapter]) -> BookSummary {
    BookSummary {
        slug: book.slug.clone(),
        title: book.title(),
        status: book.status(),
        form: book.form().map(|form| form.key().to_string()),
        form_label: book.form().map(|form| form.label().to_string()),
        target: book.target(),
        deadline: book.deadline().map(|date| date.to_string()),
        words: chapters.iter().map(|chapter| chapter.words as u64).sum(),
        chapters: chapters.len(),
    }
}

fn project_summary(project: &Project) -> CmdResult<ProjectSummary> {
    let books = project
        .books()
        .or_message()?
        .iter()
        .map(|book| Ok(book_summary(book, &book.chapters().or_message()?)))
        .collect::<CmdResult<Vec<_>>>()?;
    Ok(ProjectSummary {
        slug: project.slug.clone(),
        title: project.title(),
        genre: project.genre(),
        books,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteRef {
    path: String,
    name: String,
    title: String,
}

fn note_ref(note: &Note) -> NoteRef {
    NoteRef {
        path: note.path.display().to_string(),
        name: note.name.clone(),
        title: note.title(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterRef {
    path: String,
    name: String,
    title: String,
    number: Option<u32>,
    status: Option<String>,
    pov: Option<String>,
    synopsis: Option<String>,
    words: usize,
}

/// A project's notes, with one of its books open.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTree {
    project: ProjectSummary,
    project_note: NoteRef,
    book: Option<BookTree>,
    characters: Vec<NoteRef>,
    locations: Vec<NoteRef>,
    world: Vec<NoteRef>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookTree {
    summary: BookSummary,
    note: NoteRef,
    flow_note: Option<NoteRef>,
    chapters: Vec<ChapterRef>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteDoc {
    path: String,
    name: String,
    /// project, book, flow, chapter, character, location, world or note.
    kind: String,
    project: Option<String>,
    book: Option<String>,
    title: String,
    body: String,
    properties: Map<String, Value>,
    /// Identifies the file's content when it was read, to detect changes made elsewhere.
    hash: String,
    words: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveResult {
    /// The file changed on disk since it was read, so nothing was written.
    conflict: bool,
    hash: String,
    words: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    key: String,
    label: String,
    detail: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Choices {
    forms: Vec<Choice>,
    structures: Vec<Choice>,
    formats: Vec<Choice>,
    chapter_statuses: Vec<String>,
    book_statuses: Vec<String>,
}

/// A note path from the interface, accepted only if it is a Markdown file inside the vault.
fn note_path(vault: &Vault, path: &str) -> CmdResult<PathBuf> {
    let canonical = Path::new(path)
        .canonicalize()
        .map_err(|e| format!("{path}: {e}"))?;
    let is_markdown = canonical
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
    if !canonical.starts_with(&vault.root) || !is_markdown {
        return Err(format!("{path} is not a note in the stories folder"));
    }
    Ok(canonical)
}

/// What a note is, from where it sits in the vault, and the project and book it belongs to.
fn classify(vault: &Vault, path: &Path) -> (String, Option<String>, Option<String>) {
    let parts: Vec<String> = vault
        .relative(path)
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    let kind = match parts.as_slice() {
        [slug, file] if *file == format!("{slug}.md") => "project",
        [_, books, book, file] if books == BOOKS_DIR && *file == format!("{book}.md") => "book",
        [_, books, book, file] if books == BOOKS_DIR && *file == format!("{book}-flow.md") => {
            "flow"
        }
        [_, books, _, folder, _, ..] if books == BOOKS_DIR && folder == "chapters" => "chapter",
        [_, bible, folder, _, ..] if bible == BIBLE_DIR => NoteKind::ALL
            .into_iter()
            .find(|kind| kind.is_bible() && kind.folder() == folder.as_str())
            .map_or("note", NoteKind::key),
        _ => "note",
    };
    let project = (parts.len() > 1).then(|| parts[0].clone());
    let book = match parts.as_slice() {
        [_, books, book, _, ..] if books == BOOKS_DIR => Some(book.clone()),
        _ => None,
    };
    (kind.to_string(), project, book)
}

fn properties_json(note: &Note) -> Map<String, Value> {
    note.frontmatter
        .entries()
        .into_iter()
        .map(|(key, value)| {
            let value = match value {
                PropertyValue::Text(text) => Value::String(text),
                PropertyValue::List(items) => {
                    Value::Array(items.into_iter().map(Value::String).collect())
                }
            };
            (key, value)
        })
        .collect()
}

fn property_value(value: &Value) -> PropertyValue {
    let text = |value: &Value| match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    };
    match value {
        Value::Array(items) => PropertyValue::List(items.iter().map(text).collect()),
        other => PropertyValue::Text(text(other)),
    }
}

/// The vault's `spelling` setting, else the system language.
fn spelling_languages(vault: &Vault) -> Vec<String> {
    if let Some(languages) = vault.config.spelling.clone().filter(|l| !l.is_empty()) {
        return languages;
    }
    let lang = std::env::var("LANG").unwrap_or_default();
    let code = lang.split(['.', '@']).next().unwrap_or_default();
    match code {
        "" | "C" | "POSIX" => vec!["en_US".to_string()],
        code => vec![code.to_string()],
    }
}

/// WebKitGTK ships with spellchecking off; WebView2 and WKWebView use the system spellchecker.
fn apply_spelling(window: &WebviewWindow, languages: Vec<String>) {
    #[cfg(target_os = "linux")]
    {
        let _ = window.with_webview(move |webview| {
            use webkit2gtk::{WebContextExt, WebViewExt};
            if let Some(context) = webview.inner().context() {
                context.set_spell_checking_enabled(true);
                let languages: Vec<&str> = languages.iter().map(String::as_str).collect();
                context.set_spell_checking_languages(&languages);
            }
        });
    }
    #[cfg(not(target_os = "linux"))]
    let _ = (window, languages);
}

#[tauri::command]
pub async fn vault_status(
    state: State<'_, AppState>,
    window: WebviewWindow,
) -> CmdResult<Option<VaultInfo>> {
    if !state.is_open() {
        return Ok(None);
    }
    let vault = state.vault()?;
    apply_spelling(&window, spelling_languages(&vault));
    Ok(Some(vault_info(&vault)))
}

/// Opens a stories folder, setting it up if it is new, and remembers it for next time.
#[tauri::command]
pub async fn open_vault(
    path: String,
    state: State<'_, AppState>,
    window: WebviewWindow,
) -> CmdResult<VaultInfo> {
    let root = PathBuf::from(&path);
    scaffold::init_vault(&root, false).or_message()?;
    let vault = Vault::open(&root).or_message()?;
    config::set_user_default_vault(&vault.root).or_message()?;
    state.remember(vault.root.clone());
    apply_spelling(&window, spelling_languages(&vault));
    Ok(vault_info(&vault))
}

#[tauri::command]
pub fn choices() -> Choices {
    let strings = |items: &[&str]| items.iter().map(|item| item.to_string()).collect();
    Choices {
        forms: Form::ALL
            .map(|form| Choice {
                key: form.key().into(),
                label: form.label().into(),
                detail: format!(
                    "{} · default target {}",
                    form.range(),
                    thousands(form.default_target() as i64)
                ),
            })
            .into(),
        structures: STRUCTURES
            .map(|structure| Choice {
                key: structure.key.into(),
                label: structure.name.into(),
                detail: structure.summary.into(),
            })
            .into(),
        formats: Format::ALL
            .map(|format| Choice {
                key: format.key().into(),
                label: format.key().to_uppercase(),
                detail: format.describe().into(),
            })
            .into(),
        chapter_statuses: strings(&CHAPTER_STATUSES),
        book_statuses: strings(&BOOK_STATUSES),
    }
}

#[tauri::command]
pub async fn list_projects(state: State<'_, AppState>) -> CmdResult<Vec<ProjectSummary>> {
    state
        .vault()?
        .projects()
        .or_message()?
        .iter()
        .map(project_summary)
        .collect()
}

#[tauri::command]
pub async fn project_tree(
    slug: String,
    book: Option<String>,
    state: State<'_, AppState>,
) -> CmdResult<ProjectTree> {
    let (_, project) = state.project(&slug)?;
    let books = project.books().or_message()?;
    let open = match book {
        Some(book) => books.iter().find(|b| b.slug == book).or(books.first()),
        None => books.first(),
    };
    let book = match open {
        Some(book) => {
            let chapters = book.chapters().or_message()?;
            let flow_path = book.flow_path();
            let flow_note = if flow_path.is_file() {
                Some(note_ref(&Note::load(&flow_path).or_message()?))
            } else {
                None
            };
            Some(BookTree {
                summary: book_summary(book, &chapters),
                note: note_ref(&book.note),
                flow_note,
                chapters: chapters
                    .iter()
                    .map(|chapter| ChapterRef {
                        path: chapter.note.path.display().to_string(),
                        name: chapter.note.name.clone(),
                        title: chapter.title(),
                        number: chapter.number(),
                        status: chapter.status(),
                        pov: chapter.pov(),
                        synopsis: chapter.synopsis(),
                        words: chapter.words,
                    })
                    .collect(),
            })
        }
        None => None,
    };
    let refs = |kind| -> CmdResult<Vec<NoteRef>> {
        Ok(project
            .notes(kind)
            .or_message()?
            .iter()
            .map(note_ref)
            .collect())
    };
    Ok(ProjectTree {
        project: project_summary(&project)?,
        project_note: note_ref(&project.note),
        book,
        characters: refs(NoteKind::Character)?,
        locations: refs(NoteKind::Location)?,
        world: refs(NoteKind::World)?,
    })
}

#[tauri::command]
pub async fn read_note(path: String, state: State<'_, AppState>) -> CmdResult<NoteDoc> {
    let vault = state.vault()?;
    let path = note_path(&vault, &path)?;
    let content = fs::read_to_string(&path).or_message()?;
    let note = Note::parse(&path, &content);
    let (kind, project, book) = classify(&vault, &path);
    Ok(NoteDoc {
        path: path.display().to_string(),
        name: note.name.clone(),
        kind,
        project,
        book,
        title: note.title(),
        properties: properties_json(&note),
        hash: note::content_hash(&content),
        words: note.words(),
        body: note.body,
    })
}

/// Saves a new body and/or property values. Unless `force` is set, nothing is written if the
/// file changed on disk since `base_hash` (in Obsidian, or through a sync service).
#[tauri::command]
pub async fn save_note(
    path: String,
    body: Option<String>,
    properties: Option<Map<String, Value>>,
    base_hash: String,
    force: bool,
    state: State<'_, AppState>,
) -> CmdResult<SaveResult> {
    let vault = state.vault()?;
    let path = note_path(&vault, &path)?;
    let updates: Vec<(&str, PropertyValue)> = properties
        .iter()
        .flatten()
        .map(|(key, value)| (key.as_str(), property_value(value)))
        .collect();
    let saved = note::save(&path, body.as_deref(), &updates, &base_hash, force).or_message()?;
    Ok(match saved {
        Saved::Written { hash, words } => SaveResult {
            conflict: false,
            hash,
            words,
        },
        Saved::Conflict { hash } => SaveResult {
            conflict: true,
            hash,
            words: 0,
        },
    })
}

/// Counts words in note text by the same rules as everywhere else.
#[tauri::command]
pub fn count_words(text: String) -> usize {
    words::count_words(&text)
}

/// Where a new book went: its project and its own slug.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedBook {
    project: String,
    book: String,
    title: String,
}

fn new_book<'a>(
    title: &'a str,
    form: &str,
    target: Option<u64>,
    structure: &str,
) -> CmdResult<NewBook<'a>> {
    Ok(NewBook {
        title,
        form: Form::from_key(form).ok_or_else(|| format!("unknown form \"{form}\""))?,
        target,
        structure: structures::find(structure)
            .ok_or_else(|| format!("unknown structure \"{structure}\""))?,
        premise: None,
    })
}

/// Starts a project: a world, and its first book.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn create_project(
    title: String,
    world: Option<String>,
    form: String,
    target: Option<u64>,
    structure: String,
    from_idea: Option<String>,
    state: State<'_, AppState>,
) -> CmdResult<CreatedBook> {
    let vault = state.vault()?;
    let spec = NewProject {
        world: world.as_deref().filter(|world| !world.trim().is_empty()),
        book: new_book(&title, &form, target, &structure)?,
    };
    let idea = from_idea.as_deref().filter(|idea| !idea.trim().is_empty());
    let (project, book) =
        scaffold::start_project(&vault, &spec, idea, Local::now().naive_local()).or_message()?;
    Ok(CreatedBook {
        project: project.slug,
        title: book.title(),
        book: book.slug,
    })
}

/// Adds another book to a project.
#[tauri::command]
pub async fn create_book(
    slug: String,
    title: String,
    form: String,
    target: Option<u64>,
    structure: String,
    state: State<'_, AppState>,
) -> CmdResult<CreatedBook> {
    let (vault, project) = state.project(&slug)?;
    let spec = new_book(&title, &form, target, &structure)?;
    let book =
        scaffold::start_book(&vault, &project, &spec, Local::now().naive_local()).or_message()?;
    Ok(CreatedBook {
        project: project.slug,
        title: book.title(),
        book: book.slug,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddedNote {
    #[serde(flatten)]
    note: NoteRef,
    /// Another note with the same name elsewhere in the vault, which makes links ambiguous.
    same_name: Option<String>,
}

/// Adds a chapter to a book, or a world-bible note to the project.
#[tauri::command]
pub async fn add_note(
    slug: String,
    book: Option<String>,
    kind: String,
    name: String,
    state: State<'_, AppState>,
) -> CmdResult<AddedNote> {
    let kind = NoteKind::from_key(&kind).ok_or_else(|| format!("unknown note kind \"{kind}\""))?;
    let (vault, project, book) = if kind.is_bible() {
        let (vault, project) = state.project(&slug)?;
        (vault, project, None)
    } else {
        let (vault, project, book) = state.book(&slug, book.as_deref())?;
        (vault, project, Some(book))
    };
    let added = scaffold::add_note(
        &vault,
        &project,
        book.as_ref(),
        kind,
        &name,
        Local::now().date_naive(),
    )
    .or_message()?;
    Ok(AddedNote {
        note: note_ref(&Note::load(&added.path).or_message()?),
        same_name: added
            .same_name
            .map(|path| vault.relative(&path).display().to_string()),
    })
}

/// A link from one note to another that both Story Time and Obsidian follow.
#[tauri::command]
pub async fn link_for(from: String, to: String, state: State<'_, AppState>) -> CmdResult<String> {
    let vault = state.vault()?;
    let (from, to) = (note_path(&vault, &from)?, note_path(&vault, &to)?);
    let note = Note::load(&to).or_message()?;
    Ok(LinkIndex::build(&vault)
        .or_message()?
        .link(&from, &to, &note.title()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TodayInfo {
    words: i64,
    goal: u64,
    streak: u32,
}

/// Words written today, counted live from the files without adding to the writing log.
#[tauri::command]
pub async fn writing_today(state: State<'_, AppState>) -> CmdResult<TodayInfo> {
    let vault = state.vault()?;
    let now = Local::now().naive_local();
    let measured = vault.measure().or_message()?;
    let history = History::load(&vault.history_path())
        .or_message()?
        .preview(now, &vaults::counts(&measured));
    Ok(TodayInfo {
        words: history.written_on(now.date(), None),
        goal: vault.config.daily_goal,
        streak: history.streak(now.date()),
    })
}

/// Adds current word counts to the writing log.
#[tauri::command]
pub async fn record_snapshot(state: State<'_, AppState>) -> CmdResult<()> {
    state
        .vault()?
        .snapshot(Local::now().naive_local())
        .or_message()
        .map(drop)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusCount {
    status: String,
    chapters: usize,
    words: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayWords {
    date: String,
    words: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookProgress {
    book: BookSummary,
    /// The project's title: the world the book is set in.
    world_title: String,
    genre: Vec<String>,
    length_label: Option<String>,
    length_range: Option<String>,
    pages: u64,
    reading_minutes: u64,
    days_left: Option<i64>,
    words_per_day: Option<u64>,
    today: i64,
    goal: u64,
    statuses: Vec<StatusCount>,
    characters: usize,
    locations: usize,
    world: usize,
    history: Vec<DayWords>,
}

#[tauri::command]
pub async fn book_progress(
    slug: String,
    book: Option<String>,
    days: u32,
    state: State<'_, AppState>,
) -> CmdResult<BookProgress> {
    let (vault, _, wanted) = state.book(&slug, book.as_deref())?;
    let now = Local::now().naive_local();
    let today = now.date();
    let measured = vault.measure().or_message()?;
    let history = History::load(&vault.history_path())
        .or_message()?
        .preview(now, &vaults::counts(&measured));
    let Measured {
        project,
        book,
        chapters,
        words,
    } = measured
        .into_iter()
        .find(|m| m.book.dir == wanted.dir)
        .ok_or_else(|| format!("no book \"{}\"", wanted.slug))?;

    let remaining = book.target().map(|target| target.saturating_sub(words));
    let days_left = book
        .deadline()
        .map(|deadline| (deadline - today).num_days());
    let words_per_day = match (remaining, days_left) {
        (Some(remaining), Some(days)) if remaining > 0 && days > 0 => {
            Some(remaining.div_ceil(days as u64))
        }
        _ => None,
    };
    let length = (words > 0).then(|| Form::for_length(words));

    let mut statuses: Vec<StatusCount> = Vec::new();
    for chapter in &chapters {
        let status = chapter.status().unwrap_or_else(|| "no status".to_string());
        match statuses.iter_mut().find(|count| count.status == status) {
            Some(count) => {
                count.chapters += 1;
                count.words += chapter.words as u64;
            }
            None => statuses.push(StatusCount {
                status,
                chapters: 1,
                words: chapter.words as u64,
            }),
        }
    }

    let daily = history.daily(Some(&book.log_key()));
    let history_days = (0..u64::from(days))
        .rev()
        .filter_map(|back| today.checked_sub_days(Days::new(back)))
        .map(|day| DayWords {
            date: day.to_string(),
            words: daily.get(&day).copied().unwrap_or(0),
        })
        .collect();
    let count = |kind| project.notes(kind).map(|notes| notes.len()).or_message();

    Ok(BookProgress {
        book: book_summary(&book, &chapters),
        world_title: project.title(),
        genre: project.genre(),
        length_label: length.map(|form| form.label().to_string()),
        length_range: length.map(|form| form.range().to_string()),
        pages: words.div_ceil(vault.config.words_per_page.max(1)),
        reading_minutes: words.div_ceil(vault.config.reading_wpm.max(1)),
        days_left,
        words_per_day,
        today: history.written_on(today, Some(&book.log_key())),
        goal: vault.config.daily_goal,
        statuses,
        characters: count(NoteKind::Character)?,
        locations: count(NoteKind::Location)?,
        world: count(NoteKind::World)?,
        history: history_days,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mention {
    name: String,
    sources: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BibleNote {
    kind: String,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Unreadable {
    path: String,
    error: String,
}

/// A file to rename: its path within the vault, and the name it should have.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rename {
    path: String,
    to: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckReport {
    undocumented: Vec<Mention>,
    ambiguous_links: Vec<Mention>,
    alias_links: Vec<Mention>,
    not_on_page: Vec<BibleNote>,
    misnamed: Vec<Rename>,
    drifted: Vec<Rename>,
    without_status: Vec<String>,
    without_synopsis: Vec<String>,
    empty_chapters: Vec<String>,
    unreadable: Vec<Unreadable>,
}

#[tauri::command]
pub async fn check_project(slug: String, state: State<'_, AppState>) -> CmdResult<CheckReport> {
    let (vault, project) = state.project(&slug)?;
    let report = check::check(&vault, &project).or_message()?;
    let mentions = |links: check::LinkSources| {
        links
            .into_iter()
            .map(|(name, sources)| Mention {
                name,
                sources: sources.into_iter().collect(),
            })
            .collect()
    };
    let renames = |renames: Vec<(PathBuf, String)>| {
        renames
            .into_iter()
            .map(|(path, to)| Rename {
                path: vault.relative(&path).display().to_string(),
                to,
            })
            .collect()
    };
    Ok(CheckReport {
        undocumented: mentions(report.undocumented),
        ambiguous_links: mentions(report.ambiguous_links),
        alias_links: mentions(report.alias_links),
        misnamed: renames(report.misnamed),
        drifted: renames(report.drifted),
        not_on_page: report
            .not_on_page
            .into_iter()
            .map(|(kind, name)| BibleNote {
                kind: kind.key().to_string(),
                name,
            })
            .collect(),
        without_status: report.without_status,
        without_synopsis: report.without_synopsis,
        empty_chapters: report.empty_chapters,
        unreadable: report
            .unreadable_frontmatter
            .into_iter()
            .map(|(path, error)| Unreadable {
                path: vault.relative(&path).display().to_string(),
                error,
            })
            .collect(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileResult {
    path: String,
    chapters: usize,
    words: usize,
    skipped: Vec<String>,
}

#[tauri::command]
pub async fn compile_book(
    slug: String,
    book: Option<String>,
    format: String,
    state: State<'_, AppState>,
) -> CmdResult<CompileResult> {
    let (vault, project, book) = state.book(&slug, book.as_deref())?;
    let format = Format::from_key(&format).ok_or_else(|| format!("unknown format \"{format}\""))?;
    let manuscript = compile::manuscript(&book, vault.config.author.as_deref()).or_message()?;
    if manuscript.chapters == 0 {
        return Err(format!(
            "{} has no chapters with words in them yet",
            book.title()
        ));
    }
    let out = vault
        .build_dir()
        .join(&project.slug)
        .join(format!("{}.{}", book.slug, format.key()));
    compile::export(&manuscript.markdown, &out, format).or_message()?;
    Ok(CompileResult {
        path: out.display().to_string(),
        chapters: manuscript.chapters,
        words: manuscript.words,
        skipped: manuscript.skipped,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Idea {
    section: String,
    name: String,
    text: String,
    project: Option<String>,
}

#[tauri::command]
pub async fn list_ideas(state: State<'_, AppState>) -> CmdResult<Vec<Idea>> {
    let content = ideas::read(&state.vault()?).or_message()?;
    Ok(ideas::list_ideas(&content)
        .into_iter()
        .map(|idea| Idea {
            section: idea.section,
            name: idea.name,
            text: idea.text,
            project: idea.project,
        })
        .collect())
}

#[tauri::command]
pub async fn capture_idea(
    text: String,
    non_fiction: bool,
    state: State<'_, AppState>,
) -> CmdResult<()> {
    let section = if non_fiction {
        "Non-Fiction"
    } else {
        "Fiction"
    };
    ideas::capture(&state.vault()?, section, &text).or_message()
}

/// Development builds on Linux only: saves a PNG of the window's own contents, so screens can be
/// checked without capturing anything else on the desktop. The file appears shortly after this
/// returns.
#[tauri::command]
pub fn dev_snapshot(path: String, window: WebviewWindow) -> CmdResult<()> {
    #[cfg(all(debug_assertions, target_os = "linux"))]
    {
        window
            .with_webview(move |webview| {
                use webkit2gtk::{SnapshotOptions, SnapshotRegion, WebViewExt};
                let cancellable: Option<&webkit2gtk::gio::Cancellable> = None;
                webview.inner().snapshot(
                    SnapshotRegion::Visible,
                    SnapshotOptions::NONE,
                    cancellable,
                    move |result| {
                        let saved = result.map_err(|e| e.to_string()).and_then(|surface| {
                            let mut file = fs::File::create(&path).map_err(|e| e.to_string())?;
                            surface.write_to_png(&mut file).map_err(|e| e.to_string())
                        });
                        if let Err(error) = saved {
                            eprintln!("snapshot to {path} failed: {error}");
                        }
                    },
                );
            })
            .map_err(|e| e.to_string())
    }
    #[cfg(not(all(debug_assertions, target_os = "linux")))]
    {
        let _ = (path, window);
        Err("Snapshots are only available in Linux development builds.".to_string())
    }
}
