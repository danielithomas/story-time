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
use story_time_core::contents::{Contents, Place};
use story_time_core::history::History;
use story_time_core::ideas;
use story_time_core::links::LinkIndex;
use story_time_core::note::{self, Note, PropertyValue, Saved};
use story_time_core::project::{
    BIBLE_DIR, BOOK_STATUSES, BOOKS_DIR, RESEARCH_DIR, SCENE_STATUSES, SCRATCHPAD_DIR,
};
use story_time_core::scaffold::{self, NewBook, NewProject, NewResearch};
use story_time_core::scratchpad::{self, Scratchpad};
use story_time_core::structures::{self, STRUCTURES};
use story_time_core::text::thousands;
use story_time_core::upgrade;
use story_time_core::vault::Measured;
use story_time_core::{Book, Form, NoteKind, Project, Scene, Vault, check, config, words};
use tauri::{AppHandle, Manager, State, WebviewWindow};

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

    /// The open vault, in whatever format it is in.
    fn any_vault(&self) -> CmdResult<Vault> {
        let root = self.root.lock().expect("state lock").clone();
        let root = root.ok_or_else(|| "No bookshelf is open.".to_string())?;
        Vault::open(root).or_message()
    }

    /// The open vault, refused while it needs upgrading, so nothing misreads or edits it.
    fn vault(&self) -> CmdResult<Vault> {
        let vault = self.any_vault()?;
        vault.config.ensure_current().or_message()?;
        Ok(vault)
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
    /// The vault is laid out as story-time 0.1 did it, and must be upgraded before it's opened.
    needs_upgrade: bool,
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
        needs_upgrade: vault.config.needs_upgrade(),
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
    /// Words in all the book's scenes, placed or not.
    words: u64,
    scenes: usize,
    chapters: usize,
    unplaced: usize,
}

fn book_summary(book: &Book, contents: &Contents) -> BookSummary {
    BookSummary {
        slug: book.slug.clone(),
        title: book.title(),
        status: book.status(),
        form: book.form().map(|form| form.key().to_string()),
        form_label: book.form().map(|form| form.label().to_string()),
        target: book.target(),
        deadline: book.deadline().map(|date| date.to_string()),
        words: contents.words(),
        scenes: contents.scenes().count(),
        chapters: contents.chapters().count(),
        unplaced: contents.unplaced.len(),
    }
}

fn project_summary(project: &Project) -> CmdResult<ProjectSummary> {
    let books = project
        .books()
        .or_message()?
        .iter()
        .map(|book| Ok(book_summary(book, &book.contents().or_message()?)))
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
pub struct SceneRef {
    path: String,
    name: String,
    title: String,
    status: Option<String>,
    pov: Option<String>,
    location: Option<String>,
    when: Option<String>,
    synopsis: Option<String>,
    words: usize,
}

fn scene_ref(scene: &Scene) -> SceneRef {
    SceneRef {
        path: scene.note.path.display().to_string(),
        name: scene.note.name.clone(),
        title: scene.title(),
        status: scene.status(),
        pov: scene.pov(),
        location: scene.location(),
        when: scene.when(),
        synopsis: scene.synopsis(),
        words: scene.words,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartRef {
    /// `None` for chapters before the first part, or in a book without parts.
    title: Option<String>,
    chapters: Vec<ChapterRef>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterRef {
    /// Its position in the book, counting from 0 across parts: where a new scene can be added.
    index: usize,
    /// What it is called on the page: its title, or "Chapter 3".
    heading: String,
    title: Option<String>,
    scenes: Vec<SceneRef>,
    words: u64,
}

/// A note in a scratchpad.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrapRef {
    path: String,
    name: String,
    /// Its name, or the first words of its text.
    title: String,
    created: Option<String>,
    words: usize,
}

fn scrap_ref(note: &Note) -> ScrapRef {
    ScrapRef {
        path: note.path.display().to_string(),
        name: note.name.clone(),
        title: scratchpad::label(note),
        created: note.frontmatter.str("created"),
        words: note.words(),
    }
}

/// A project's notes, with one of its books open, and the vault's scratchpad and research.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTree {
    project: ProjectSummary,
    project_note: NoteRef,
    book: Option<BookTree>,
    characters: Vec<NoteRef>,
    locations: Vec<NoteRef>,
    world: Vec<NoteRef>,
    timeline: Option<NoteRef>,
    research: Vec<NoteRef>,
    scratchpad: Vec<ScrapRef>,
    vault_research: Vec<NoteRef>,
    vault_scratchpad: Vec<ScrapRef>,
}

/// A book's notes, with its scenes as its contents list them.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookTree {
    summary: BookSummary,
    note: NoteRef,
    flow_note: Option<NoteRef>,
    has_parts: bool,
    parts: Vec<PartRef>,
    unplaced: Vec<SceneRef>,
}

fn book_tree(book: &Book) -> CmdResult<BookTree> {
    let contents = book.contents().or_message()?;
    let flow_path = book.flow_path();
    let flow_note = if flow_path.is_file() {
        Some(note_ref(&Note::load(&flow_path).or_message()?))
    } else {
        None
    };
    let mut index = 0;
    let parts = contents
        .parts
        .iter()
        .map(|part| PartRef {
            title: part.title.clone(),
            chapters: part
                .chapters
                .iter()
                .map(|chapter| {
                    index += 1;
                    ChapterRef {
                        index: index - 1,
                        heading: chapter.heading(),
                        title: chapter.title.clone(),
                        scenes: chapter.scenes.iter().map(scene_ref).collect(),
                        words: chapter.words(),
                    }
                })
                .collect(),
        })
        .collect();
    Ok(BookTree {
        summary: book_summary(book, &contents),
        note: note_ref(&book.note),
        flow_note,
        has_parts: contents.has_parts(),
        parts,
        unplaced: contents.unplaced.iter().map(scene_ref).collect(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteDoc {
    path: String,
    name: String,
    /// project, book, flow, scene, character, location, world, timeline, research, scrap or note.
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
    scene_statuses: Vec<String>,
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
        return Err(format!("{path} is not a note on this bookshelf"));
    }
    Ok(canonical)
}

/// What a note is, from where it sits in the vault, and the project and book it belongs to.
fn classify(vault: &Vault, path: &Path) -> (String, Option<String>, Option<String>) {
    if path == vault.ideas_path() {
        return ("note".to_string(), None, None);
    }
    if path.starts_with(vault.scratchpad_dir()) {
        return ("scrap".to_string(), None, None);
    }
    if path.starts_with(vault.research_dir()) {
        return ("research".to_string(), None, None);
    }
    let parts: Vec<String> = vault
        .relative(path)
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    let kind = match parts.as_slice() {
        [slug, file] if *file == format!("{slug}.md") => "project",
        [slug, bible, file] if bible == BIBLE_DIR && *file == format!("{slug}-timeline.md") => {
            "timeline"
        }
        [_, folder, _, ..] if folder == SCRATCHPAD_DIR => "scrap",
        [_, folder, _, ..] if folder == RESEARCH_DIR => "research",
        [_, books, book, file] if books == BOOKS_DIR && *file == format!("{book}.md") => "book",
        [_, books, book, file] if books == BOOKS_DIR && *file == format!("{book}-flow.md") => {
            "flow"
        }
        [_, books, _, folder, _, ..]
            if books == BOOKS_DIR && folder == NoteKind::Scene.folder() =>
        {
            "scene"
        }
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
    let vault = state.any_vault()?;
    apply_spelling(&window, spelling_languages(&vault));
    Ok(Some(vault_info(&vault)))
}

/// A folder the writer picked, before it opens.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderInfo {
    path: String,
    name: String,
    /// "bookshelf", "empty", or "occupied" for a folder holding other things.
    state: &'static str,
    /// What setting up an occupied folder adds to it.
    adds: Vec<String>,
}

/// Looks at a folder before it opens, so the writer can be asked before an occupied one is set up.
#[tauri::command]
pub async fn inspect_folder(path: String) -> CmdResult<FolderInfo> {
    let root = PathBuf::from(&path);
    let (state, adds) = match scaffold::inspect_folder(&root).or_message()? {
        scaffold::Folder::Bookshelf => ("bookshelf", Vec::new()),
        scaffold::Folder::Empty => ("empty", Vec::new()),
        scaffold::Folder::Occupied(adds) => ("occupied", adds),
    };
    Ok(FolderInfo {
        name: root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone()),
        path,
        state,
        adds,
    })
}

/// Where a new bookshelf goes unless the writer chooses somewhere else: their Documents folder.
#[tauri::command]
pub fn default_bookshelf_place(app: AppHandle) -> CmdResult<String> {
    let place = app.path().document_dir().or_else(|_| app.path().home_dir());
    Ok(place.map_err(|e| e.to_string())?.display().to_string())
}

/// Makes a new bookshelf called `name` in `place`, then opens it.
#[tauri::command]
pub async fn create_bookshelf(
    place: String,
    name: String,
    state: State<'_, AppState>,
    window: WebviewWindow,
) -> CmdResult<VaultInfo> {
    let root = scaffold::create_bookshelf(Path::new(&place), &name).or_message()?;
    enter(&root, &state, &window)
}

/// Opens a bookshelf, setting it up if it is new, and remembers it for next time. A folder in
/// story-time 0.1's layout is left as it is until the writer chooses to upgrade it.
#[tauri::command]
pub async fn open_vault(
    path: String,
    state: State<'_, AppState>,
    window: WebviewWindow,
) -> CmdResult<VaultInfo> {
    let root = PathBuf::from(&path);
    if !config::Config::load(&root).or_message()?.needs_upgrade() {
        scaffold::init_vault(&root, false).or_message()?;
    }
    enter(&root, &state, &window)
}

/// Makes `root` the open bookshelf, here and the next time the app starts.
fn enter(root: &Path, state: &AppState, window: &WebviewWindow) -> CmdResult<VaultInfo> {
    let vault = Vault::open(root).or_message()?;
    config::set_user_default_vault(&vault.root).or_message()?;
    state.remember(vault.root.clone());
    apply_spelling(window, spelling_languages(&vault));
    Ok(vault_info(&vault))
}

/// Upgrades the open vault to the current format, after backing it up. Returns where the backup is,
/// relative to the vault.
#[tauri::command]
pub async fn upgrade_vault(state: State<'_, AppState>) -> CmdResult<String> {
    let vault = state.any_vault()?;
    let plan = upgrade::plan(&vault).or_message()?;
    let backup = upgrade::apply(&vault, &plan, Local::now().naive_local()).or_message()?;
    Ok(vault.relative(&backup).display().to_string())
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
        scene_statuses: strings(&SCENE_STATUSES),
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
    let (vault, project) = state.project(&slug)?;
    let books = project.books().or_message()?;
    let open = match book {
        Some(book) => books.iter().find(|b| b.slug == book).or(books.first()),
        None => books.first(),
    };
    let book = open.map(book_tree).transpose()?;
    let refs = |kind| -> CmdResult<Vec<NoteRef>> {
        Ok(project
            .notes(kind)
            .or_message()?
            .iter()
            .map(note_ref)
            .collect())
    };
    let research = |dir: &Path| -> CmdResult<Vec<NoteRef>> {
        Ok(scratchpad::research(dir)
            .or_message()?
            .iter()
            .map(note_ref)
            .collect())
    };
    let scraps = |scratchpad: Scratchpad| scratchpad.notes.iter().map(scrap_ref).collect();
    let timeline = project.timeline_path();
    let timeline = if timeline.is_file() {
        Some(note_ref(&Note::load(&timeline).or_message()?))
    } else {
        None
    };
    Ok(ProjectTree {
        project: project_summary(&project)?,
        project_note: note_ref(&project.note),
        book,
        characters: refs(NoteKind::Character)?,
        locations: refs(NoteKind::Location)?,
        world: refs(NoteKind::World)?,
        timeline,
        research: research(&project.research_dir())?,
        scratchpad: scraps(Scratchpad::of_project(&project).or_message()?),
        vault_research: research(&vault.research_dir())?,
        vault_scratchpad: scraps(Scratchpad::of_vault(&vault).or_message()?),
    })
}

#[tauri::command]
pub async fn read_note(path: String, state: State<'_, AppState>) -> CmdResult<NoteDoc> {
    let vault = state.vault()?;
    let path = note_path(&vault, &path)?;
    let content = fs::read_to_string(&path).or_message()?;
    let note = Note::parse(&path, &content);
    let (kind, project, book) = classify(&vault, &path);
    let title = if kind == "scrap" {
        scratchpad::label(&note)
    } else {
        note.title()
    };
    Ok(NoteDoc {
        path: path.display().to_string(),
        name: note.name.clone(),
        kind,
        project,
        book,
        title,
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
    /// The book's first scene, empty and ready to write in.
    scene: String,
}

/// The name of the scene a book made in the app starts with. The CLI leaves new books empty,
/// since its users name their scenes as they add them.
const FIRST_SCENE: &str = "Scene 1";

fn created_book(vault: &Vault, project: &Project, book: &Book) -> CmdResult<CreatedBook> {
    let today = Local::now().date_naive();
    let scene =
        scaffold::add_scene(vault, project, book, FIRST_SCENE, Place::End, today).or_message()?;
    Ok(CreatedBook {
        project: project.slug.clone(),
        title: book.title(),
        book: book.slug.clone(),
        scene: scene.path.display().to_string(),
    })
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
    created_book(&vault, &project, &book)
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
    created_book(&vault, &project, &book)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddedNote {
    #[serde(flatten)]
    note: NoteRef,
    /// Another note with the same name elsewhere in the vault, which makes links ambiguous.
    same_name: Option<String>,
}

fn added_note(vault: &Vault, added: scaffold::NewNote) -> CmdResult<AddedNote> {
    Ok(AddedNote {
        note: note_ref(&Note::load(&added.path).or_message()?),
        same_name: added
            .same_name
            .map(|path| vault.relative(&path).display().to_string()),
    })
}

/// Adds a character, location or world note to the project's world bible.
#[tauri::command]
pub async fn add_note(
    slug: String,
    kind: String,
    name: String,
    state: State<'_, AppState>,
) -> CmdResult<AddedNote> {
    let kind = NoteKind::from_key(&kind).ok_or_else(|| format!("unknown note kind \"{kind}\""))?;
    let (vault, project) = state.project(&slug)?;
    let added = scaffold::add_note(&vault, &project, kind, &name, Local::now().date_naive())
        .or_message()?;
    added_note(&vault, added)
}

/// Adds a scene to a book: at the end of chapter `chapter` (counting from 0), else at the end of
/// the contents as a chapter of its own, or with `unplaced`, outside the contents.
#[tauri::command]
pub async fn add_scene(
    slug: String,
    book: Option<String>,
    name: String,
    chapter: Option<usize>,
    unplaced: bool,
    state: State<'_, AppState>,
) -> CmdResult<AddedNote> {
    let (vault, project, book) = state.book(&slug, book.as_deref())?;
    let place = match (unplaced, chapter) {
        (true, _) => Place::Unplaced,
        (false, Some(n)) => Place::Chapter(n),
        (false, None) => Place::End,
    };
    let added = scaffold::add_scene(
        &vault,
        &project,
        &book,
        &name,
        place,
        Local::now().date_naive(),
    )
    .or_message()?;
    added_note(&vault, added)
}

/// The project with this slug, or `None` for the vault itself.
fn owner(state: &AppState, slug: Option<&str>) -> CmdResult<(Vault, Option<Project>)> {
    match slug {
        Some(slug) => {
            let (vault, project) = state.project(slug)?;
            Ok((vault, Some(project)))
        }
        None => Ok((state.vault()?, None)),
    }
}

/// Adds an empty scrap to a project's scratchpad, or with no project, the vault's.
#[tauri::command]
pub async fn add_scrap(slug: Option<String>, state: State<'_, AppState>) -> CmdResult<AddedNote> {
    let (vault, project) = owner(&state, slug.as_deref())?;
    let added = scaffold::add_scrap(
        &vault,
        project.as_ref(),
        "",
        None,
        Local::now().naive_local(),
    )
    .or_message()?;
    added_note(&vault, added)
}

/// Adds a research note to a project, or with no project, to the vault.
#[tauri::command]
pub async fn add_research(
    slug: Option<String>,
    name: String,
    state: State<'_, AppState>,
) -> CmdResult<AddedNote> {
    let (vault, project) = owner(&state, slug.as_deref())?;
    let research = NewResearch {
        name: &name,
        source: None,
        text: None,
        by_agent: false,
    };
    let added = scaffold::add_research(
        &vault,
        project.as_ref(),
        &research,
        Local::now().date_naive(),
    )
    .or_message()?;
    added_note(&vault, added)
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
        .preview(now, &vault.counts(&measured).or_message()?);
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
    scenes: usize,
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
    /// Words in the placed scenes: the manuscript.
    placed_words: u64,
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
        .preview(now, &vault.counts(&measured).or_message()?);
    let Measured {
        project,
        book,
        contents,
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
    let placed = contents.placed_words();
    let length = (placed > 0).then(|| Form::for_length(placed));

    let mut statuses: Vec<StatusCount> = Vec::new();
    for scene in contents.scenes() {
        let status = scene.status().unwrap_or_else(|| "no status".to_string());
        match statuses.iter_mut().find(|count| count.status == status) {
            Some(count) => {
                count.scenes += 1;
                count.words += scene.words as u64;
            }
            None => statuses.push(StatusCount {
                status,
                scenes: 1,
                words: scene.words as u64,
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
        book: book_summary(&book, &contents),
        placed_words: placed,
        world_title: project.title(),
        genre: project.genre(),
        length_label: length.map(|form| form.label().to_string()),
        length_range: length.map(|form| form.range().to_string()),
        pages: placed.div_ceil(vault.config.words_per_page.max(1)),
        reading_minutes: placed.div_ceil(vault.config.reading_wpm.max(1)),
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
    broken_entries: Vec<Mention>,
    repeated: Vec<String>,
    not_on_page: Vec<BibleNote>,
    misnamed: Vec<Rename>,
    drifted: Vec<Rename>,
    without_status: Vec<String>,
    without_synopsis: Vec<String>,
    empty_scenes: Vec<String>,
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
        broken_entries: mentions(report.broken_entries),
        repeated: report.repeated,
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
        empty_scenes: report.empty_scenes,
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
    scenes: usize,
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
    if manuscript.scenes == 0 {
        return Err(format!(
            "{} has no scenes with words in them in its contents yet",
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
        scenes: manuscript.scenes,
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
