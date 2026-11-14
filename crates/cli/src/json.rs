//! `--json`: what `stats`, `outline`, `today`, `history`, `check` and `ideas` report, as JSON for
//! scripts and other programs (Obsidian's Shell Commands plugin, n8n, a dashboard). Keys are
//! snake_case and stay stable; a value that isn't set is `null`. Paths are relative to the
//! bookshelf, and links in properties (a scene's point of view, say) are given as the names they
//! show.

use anyhow::Result;
use chrono::NaiveDate;
use serde::Serialize;
use story_time_core::check::Report;
use story_time_core::contents::Contents;
use story_time_core::history::History;
use story_time_core::ideas::IdeaEntry;
use story_time_core::scratchpad::Scratchpad;
use story_time_core::vault::Measured;
use story_time_core::words::plain_text;
use story_time_core::{Book, Form, Project, Scene, Vault};

/// Prints `value` as indented JSON.
pub fn print(value: &impl Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

#[derive(Serialize)]
pub struct ProjectRef {
    slug: String,
    title: String,
    id: Option<String>,
}

pub fn project_ref(project: &Project) -> ProjectRef {
    ProjectRef {
        slug: project.slug.clone(),
        title: project.title(),
        id: project.id(),
    }
}

#[derive(Serialize)]
pub struct BookSummary {
    project: ProjectRef,
    slug: String,
    title: String,
    id: Option<String>,
    path: String,
    status: Option<String>,
    form: Option<&'static str>,
    target: Option<u64>,
    deadline: Option<NaiveDate>,
    /// Every scene in the book, placed or not, and their words.
    scenes: usize,
    words: u64,
    /// The scenes the contents list, and their words: the manuscript.
    placed_scenes: usize,
    placed_words: u64,
    /// Words against the target, from 0 (1 or more once it's reached).
    progress: Option<f64>,
    written_today: i64,
}

fn book_summary(
    vault: &Vault,
    measured: &Measured,
    history: &History,
    day: NaiveDate,
) -> BookSummary {
    let Measured {
        project,
        book,
        contents,
        words,
    } = measured;
    BookSummary {
        project: project_ref(project),
        slug: book.slug.clone(),
        title: book.title(),
        id: book.id(),
        path: relative(vault, &book.note.path),
        status: book.status(),
        form: book.form().map(Form::key),
        target: book.target(),
        deadline: book.deadline(),
        scenes: contents.scenes().count(),
        words: *words,
        placed_scenes: contents.placed().count(),
        placed_words: contents.placed_words(),
        progress: book.target().map(|target| *words as f64 / target as f64),
        written_today: history.written_on(day, Some(&book.log_key())),
    }
}

/// `stats`: every book on the bookshelf.
#[derive(Serialize)]
pub struct Stats {
    date: NaiveDate,
    daily_goal: u64,
    written_today: i64,
    streak: u32,
    words: u64,
    books: Vec<BookSummary>,
}

pub fn stats(vault: &Vault, measured: &[Measured], history: &History, day: NaiveDate) -> Stats {
    Stats {
        date: day,
        daily_goal: vault.config.daily_goal,
        written_today: history.written_on(day, None),
        streak: history.streak(day),
        words: measured.iter().map(|m| m.words).sum(),
        books: measured
            .iter()
            .map(|m| book_summary(vault, m, history, day))
            .collect(),
    }
}

#[derive(Serialize)]
pub struct BookDetail {
    #[serde(flatten)]
    summary: BookSummary,
    /// The form the placed words currently fall in, by length.
    length: Option<&'static str>,
    manuscript_pages: u64,
    reading_minutes: u64,
    /// With a target and a deadline still to come, the words a day it takes to meet it.
    words_a_day: Option<u64>,
    days_left: Option<i64>,
    parts: Vec<PartOutline>,
    unplaced: Vec<SceneOutline>,
}

#[derive(Serialize)]
pub struct WorldBible {
    characters: usize,
    locations: usize,
    world: usize,
}

#[derive(Serialize)]
pub struct ScratchpadSize {
    notes: usize,
    words: u64,
}

/// `stats <project>`: one world's books scene by scene, and the size of its world bible,
/// scratchpad and research.
#[derive(Serialize)]
pub struct ProjectStats {
    date: NaiveDate,
    project: ProjectRef,
    genre: Vec<String>,
    books: Vec<BookDetail>,
    world_bible: WorldBible,
    scratchpad: ScratchpadSize,
    research: usize,
}

pub struct ProjectSizes {
    pub characters: usize,
    pub locations: usize,
    pub world: usize,
    pub research: usize,
}

pub fn project_stats(
    vault: &Vault,
    project: &Project,
    books: &[Measured],
    history: &History,
    day: NaiveDate,
    sizes: ProjectSizes,
    scratchpad: &Scratchpad,
) -> ProjectStats {
    let books = books
        .iter()
        .map(|measured| {
            let placed = measured.contents.placed_words();
            let book = &measured.book;
            let left = book
                .target()
                .map(|target| target.saturating_sub(measured.words));
            let days_left = book.deadline().map(|deadline| (deadline - day).num_days());
            let words_a_day = match (left, days_left) {
                (Some(left), Some(days)) if left > 0 && days > 0 => {
                    Some(left.div_ceil(days as u64))
                }
                _ => None,
            };
            let (parts, unplaced) = outline_of(vault, &measured.contents);
            BookDetail {
                summary: book_summary(vault, measured, history, day),
                length: (placed > 0).then(|| Form::for_length(placed).key()),
                manuscript_pages: placed.div_ceil(vault.config.words_per_page.max(1)),
                reading_minutes: placed.div_ceil(vault.config.reading_wpm.max(1)),
                words_a_day,
                days_left,
                parts,
                unplaced,
            }
        })
        .collect();
    ProjectStats {
        date: day,
        project: project_ref(project),
        genre: project.genre(),
        books,
        world_bible: WorldBible {
            characters: sizes.characters,
            locations: sizes.locations,
            world: sizes.world,
        },
        scratchpad: ScratchpadSize {
            notes: scratchpad.notes.len(),
            words: scratchpad.words,
        },
        research: sizes.research,
    }
}

#[derive(Serialize)]
pub struct SceneOutline {
    /// Its file name, which links use.
    name: String,
    title: String,
    path: String,
    status: Option<String>,
    pov: Option<String>,
    location: Option<String>,
    when: Option<String>,
    synopsis: Option<String>,
    words: usize,
}

fn scene_outline(vault: &Vault, scene: &Scene) -> SceneOutline {
    SceneOutline {
        name: scene.note.name.clone(),
        title: scene.title(),
        path: relative(vault, &scene.note.path),
        status: scene.status(),
        pov: scene.pov().map(|pov| plain_text(&pov)),
        location: scene.location().map(|location| plain_text(&location)),
        when: scene.when(),
        synopsis: scene.synopsis(),
        words: scene.words,
    }
}

#[derive(Serialize)]
pub struct ChapterOutline {
    /// What it's called on the page: its title, or "Chapter 3".
    heading: String,
    title: Option<String>,
    words: u64,
    scenes: Vec<SceneOutline>,
}

#[derive(Serialize)]
pub struct PartOutline {
    /// `null` for chapters before the first part, or in a book without parts.
    title: Option<String>,
    chapters: Vec<ChapterOutline>,
}

fn outline_of(vault: &Vault, contents: &Contents) -> (Vec<PartOutline>, Vec<SceneOutline>) {
    let parts = contents
        .parts
        .iter()
        .map(|part| PartOutline {
            title: part.title.clone(),
            chapters: part
                .chapters
                .iter()
                .map(|chapter| ChapterOutline {
                    heading: chapter.heading(),
                    title: chapter.title.clone(),
                    words: chapter.words(),
                    scenes: chapter
                        .scenes
                        .iter()
                        .map(|scene| scene_outline(vault, scene))
                        .collect(),
                })
                .collect(),
        })
        .collect();
    let unplaced = contents
        .unplaced
        .iter()
        .map(|scene| scene_outline(vault, scene))
        .collect();
    (parts, unplaced)
}

/// `outline`: a book's scenes, part by part and chapter by chapter, then those not placed.
#[derive(Serialize)]
pub struct Outline {
    project: ProjectRef,
    book: String,
    title: String,
    parts: Vec<PartOutline>,
    unplaced: Vec<SceneOutline>,
}

pub fn outline(vault: &Vault, project: &Project, book: &Book, contents: &Contents) -> Outline {
    let (parts, unplaced) = outline_of(vault, contents);
    Outline {
        project: project_ref(project),
        book: book.slug.clone(),
        title: book.title(),
        parts,
        unplaced,
    }
}

#[derive(Serialize)]
pub struct Written {
    /// A book's or scratchpad's name, and the key the writing log records it under.
    title: String,
    key: String,
    words: i64,
}

/// `today`: words written today, book by book and scratchpad by scratchpad.
#[derive(Serialize)]
pub struct Today {
    date: NaiveDate,
    daily_goal: u64,
    written: i64,
    goal_met: bool,
    streak: u32,
    by: Vec<Written>,
}

pub fn today(
    vault: &Vault,
    measured: &[Measured],
    scratchpads: &[Scratchpad],
    history: &History,
    day: NaiveDate,
) -> Today {
    let written = history.written_on(day, None);
    let goal = vault.config.daily_goal;
    let books = measured.iter().map(|m| (m.book.title(), m.book.log_key()));
    let pads = scratchpads.iter().map(|pad| (pad.title(), pad.log_key()));
    Today {
        date: day,
        daily_goal: goal,
        written,
        goal_met: goal > 0 && written >= goal as i64,
        streak: history.streak(day),
        by: books
            .chain(pads)
            .map(|(title, key)| Written {
                words: history.written_on(day, Some(&key)),
                title,
                key,
            })
            .filter(|written| written.words != 0)
            .collect(),
    }
}

#[derive(Serialize)]
pub struct Day {
    date: NaiveDate,
    words: i64,
}

/// `history`: words written each day, oldest first.
#[derive(Serialize)]
pub struct HistoryReport {
    daily_goal: u64,
    days: Vec<Day>,
    total: i64,
    days_written: usize,
    best: i64,
}

pub fn history(goal: u64, days: &[NaiveDate], written: &[i64]) -> HistoryReport {
    HistoryReport {
        daily_goal: goal,
        days: days
            .iter()
            .zip(written)
            .map(|(date, words)| Day {
                date: *date,
                words: *words,
            })
            .collect(),
        total: written.iter().sum(),
        days_written: written.iter().filter(|words| **words > 0).count(),
        best: written.iter().copied().max().unwrap_or(0).max(0),
    }
}

#[derive(Serialize)]
pub struct Linked {
    /// A link as written, and the notes it's in, by file name.
    link: String,
    from: Vec<String>,
}

#[derive(Serialize)]
pub struct NotOnPage {
    kind: &'static str,
    name: String,
}

#[derive(Serialize)]
pub struct NearMiss {
    word: String,
    /// The word of the name it's close to, and that note's name and path.
    term: String,
    name: String,
    note: String,
    case_only: bool,
    scenes: Vec<String>,
}

#[derive(Serialize)]
pub struct Rename {
    path: String,
    rename_to: String,
}

#[derive(Serialize)]
pub struct Unreadable {
    path: String,
    error: String,
}

/// `check`: everything it reports, and whether there was nothing to report.
#[derive(Serialize)]
pub struct Check {
    project: ProjectRef,
    clean: bool,
    undocumented: Vec<Linked>,
    ambiguous_links: Vec<Linked>,
    alias_links: Vec<Linked>,
    broken_entries: Vec<Linked>,
    repeated: Vec<String>,
    not_on_page: Vec<NotOnPage>,
    near_misses: Vec<NearMiss>,
    misnamed: Vec<Rename>,
    drifted: Vec<Rename>,
    without_status: Vec<String>,
    without_synopsis: Vec<String>,
    empty_scenes: Vec<String>,
    unreadable_frontmatter: Vec<Unreadable>,
}

pub fn check(vault: &Vault, project: &Project, report: Report) -> Check {
    let clean = report.is_clean();
    let linked = |links: story_time_core::check::LinkSources| {
        links
            .into_iter()
            .map(|(link, from)| Linked {
                link,
                from: from.into_iter().collect(),
            })
            .collect()
    };
    let renames = |renames: Vec<(std::path::PathBuf, String)>| {
        renames
            .into_iter()
            .map(|(path, rename_to)| Rename {
                path: relative(vault, &path),
                rename_to,
            })
            .collect()
    };
    Check {
        project: project_ref(project),
        clean,
        undocumented: linked(report.undocumented),
        ambiguous_links: linked(report.ambiguous_links),
        alias_links: linked(report.alias_links),
        broken_entries: linked(report.broken_entries),
        repeated: report.repeated,
        not_on_page: report
            .not_on_page
            .into_iter()
            .map(|(kind, name)| NotOnPage {
                kind: kind.key(),
                name,
            })
            .collect(),
        near_misses: report
            .near_misses
            .into_iter()
            .map(|miss| NearMiss {
                note: relative(vault, &miss.note),
                word: miss.word,
                term: miss.term,
                name: miss.name,
                case_only: miss.case_only,
                scenes: miss.scenes.into_iter().collect(),
            })
            .collect(),
        misnamed: renames(report.misnamed),
        drifted: renames(report.drifted),
        without_status: report.without_status,
        without_synopsis: report.without_synopsis,
        empty_scenes: report.empty_scenes,
        unreadable_frontmatter: report
            .unreadable_frontmatter
            .into_iter()
            .map(|(path, error)| Unreadable {
                path: relative(vault, &path),
                error,
            })
            .collect(),
    }
}

#[derive(Serialize)]
pub struct Idea {
    section: String,
    name: String,
    text: String,
    /// The project it became, by file name, if it has been started.
    project: Option<String>,
}

/// `ideas`: every idea in the ideas file, with the heading it's under.
#[derive(Serialize)]
pub struct Ideas {
    ideas: Vec<Idea>,
}

pub fn ideas(entries: Vec<IdeaEntry>) -> Ideas {
    Ideas {
        ideas: entries
            .into_iter()
            .map(|entry| Idea {
                section: entry.section,
                name: entry.name,
                text: entry.text,
                project: entry.project,
            })
            .collect(),
    }
}

fn relative(vault: &Vault, path: &std::path::Path) -> String {
    vault.relative(path).to_string_lossy().replace('\\', "/")
}
