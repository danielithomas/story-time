//! What the commands that only look at the bookshelf report, as JSON: printed by `--json`, and
//! returned by the MCP server's tools, so the two always agree. Counting words records the
//! counts in the writing log first, as every command that counts does.

use std::path::Path;

use anyhow::Result;
use chrono::NaiveDateTime;
use serde::Serialize;
use serde_json::Value;
use story_time_core::check;
use story_time_core::ideas;
use story_time_core::scratchpad::{self, Scratchpad};
use story_time_core::search;
use story_time_core::vault::Measured;
use story_time_core::{NoteKind, Vault};

use crate::json;

fn value(report: impl Serialize) -> Result<Value> {
    Ok(serde_json::to_value(report)?)
}

/// `stats`: every book, or with a project, its books in detail.
pub fn stats(
    vault: &Vault,
    cwd: &Path,
    project: Option<&str>,
    now: NaiveDateTime,
) -> Result<Value> {
    let Some(query) = project else {
        let (measured, history) = vault.snapshot(now)?;
        return value(json::stats(vault, &measured, &history, now.date()));
    };
    let project = vault.find_project(Some(query), cwd)?;
    let (measured, history) = vault.snapshot(now)?;
    let books: Vec<Measured> = measured
        .into_iter()
        .filter(|m| m.project.slug == project.slug)
        .collect();
    let sizes = json::ProjectSizes {
        characters: project.notes(NoteKind::Character)?.len(),
        locations: project.notes(NoteKind::Location)?.len(),
        world: project.notes(NoteKind::World)?.len(),
        research: scratchpad::research(&project.research_dir())?.len(),
    };
    let scratchpad = Scratchpad::of_project(&project)?;
    value(json::project_stats(
        vault,
        &project,
        &books,
        &history,
        now.date(),
        sizes,
        &scratchpad,
    ))
}

/// `outline`: a book's scenes, chapter by chapter.
pub fn outline(
    vault: &Vault,
    cwd: &Path,
    project: Option<&str>,
    book: Option<&str>,
) -> Result<Value> {
    let project = vault.find_project(project, cwd)?;
    let book = project.find_book(book, cwd)?;
    let contents = book.contents()?;
    value(json::outline(vault, &project, &book, &contents))
}

/// `today`: words written today against the daily goal, and the streak.
pub fn today(vault: &Vault, now: NaiveDateTime) -> Result<Value> {
    let (measured, history) = vault.snapshot(now)?;
    let scratchpads = vault.scratchpads()?;
    value(json::today(
        vault,
        &measured,
        &scratchpads,
        &history,
        now.date(),
    ))
}

/// `check`: a project's world-bible gaps and loose ends.
pub fn check(vault: &Vault, cwd: &Path, project: Option<&str>) -> Result<Value> {
    let project = vault.find_project(project, cwd)?;
    let report = check::check(vault, &project)?;
    value(json::check(vault, &project, report))
}

/// `ideas`: the ideas file's ideas.
pub fn ideas(vault: &Vault) -> Result<Value> {
    value(json::ideas(ideas::list_ideas(&ideas::read(vault)?)))
}

/// `notes`: every note on the bookshelf, or in one project.
pub fn notes(vault: &Vault, cwd: &Path, project: Option<&str>) -> Result<Value> {
    let project = project
        .map(|query| vault.find_project(Some(query), cwd))
        .transpose()?;
    value(json::notes(vault, &search::notes(vault, project.as_ref())?))
}

/// `search`: the notes with every word of `query`.
pub fn search(vault: &Vault, cwd: &Path, query: &str, project: Option<&str>) -> Result<Value> {
    let project = project
        .map(|query| vault.find_project(Some(query), cwd))
        .transpose()?;
    let found = search::search(vault, query, project.as_ref())?;
    value(json::search(vault, query, &found))
}

/// `show`: one note in full.
pub fn show(vault: &Vault, cwd: &Path, note: &str) -> Result<Value> {
    let path = search::find(vault, cwd, note)?;
    let (listed, content) = search::read(vault, &path)?;
    value(json::shown(vault, &listed, &content))
}
