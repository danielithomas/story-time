//! One function per subcommand: call into the core, then print for a person to read.

use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::{Days, NaiveDateTime};
use story_time_core::backup;
use story_time_core::check::{self, BIBLE};
use story_time_core::compile::{self, Format};
use story_time_core::config::{FORMAT, UserConfig};
use story_time_core::contents::Place;
use story_time_core::example;
use story_time_core::history::History;
use story_time_core::ideas;
use story_time_core::rename;
use story_time_core::scaffold::{self, NewBook, NewProject, NewResearch, Step};
use story_time_core::scratchpad::{self, Scratchpad};
use story_time_core::upgrade;
use story_time_core::vault::Measured;
use story_time_core::{Book, Form, NoteKind, Scene, Vault};

use crate::json;
use crate::output::{Align, Table, bar, percent, plural, signed, thousands, wrap};

pub fn init(root: &Path, force: bool, example: bool, now: NaiveDateTime) -> Result<()> {
    let root = root
        .canonicalize()
        .with_context(|| format!("{} not found", root.display()))?;
    let steps = scaffold::init_vault(&root, force)?;
    println!("Bookshelf: {}", root.display());
    for step in &steps {
        let (verb, path) = match step {
            Step::Created(path) => ("created", path),
            Step::Updated(path) => ("updated", path),
            Step::Kept(path) => ("kept   ", path),
        };
        println!(
            "  {verb}  {}",
            path.strip_prefix(&root).unwrap_or(path).display()
        );
    }
    if steps
        .iter()
        .any(|step| matches!(step, Step::Created(path) if path.ends_with("templates.json")))
    {
        println!(
            "\nObsidian's Templates plugin now reads from the templates folder (restart Obsidian if it is open)."
        );
    }
    if example {
        let dir = example::add_example(&Vault::open(&root)?, now)?;
        println!(
            "\nAdded {}/, an example world with a short book whose scenes explain how story-time \
             works. Delete the folder when you're done with it.",
            dir.strip_prefix(&root).unwrap_or(&dir).display()
        );
    }
    Ok(())
}

pub fn backup(vault: &Vault, now: NaiveDateTime) -> Result<()> {
    let folder = backup::folder(vault, UserConfig::load()?.backup_folder().as_deref())?;
    match backup::back_up(vault, &folder, now)? {
        Some(copy) => println!("Backed up to {}", copy.display()),
        None => println!(
            "Nothing has changed since the last backup, in {}",
            folder.display()
        ),
    }
    Ok(())
}

pub fn upgrade(vault: &Vault, dry_run: bool, now: NaiveDateTime) -> Result<()> {
    let plan = upgrade::plan(vault)?;
    let shown = |path: &Path| vault.relative(path).display().to_string();
    println!("Upgrading {} to format {FORMAT}", vault.root.display());
    for world in &plan.worlds {
        println!(
            "  {}/ becomes a world with one book, {}/",
            shown(&world.from),
            shown(world.book.parent().unwrap_or(&world.book))
        );
        println!("    create  {}", shown(&world.note));
        println!("    create  {}", shown(&world.timeline));
    }
    for (from, to) in &plan.moves {
        println!("    move    {} → {}", shown(from), shown(to));
    }
    for path in &plan.replaced {
        println!(
            "    replace {} (unchanged since story-time 0.1)",
            shown(path)
        );
    }
    println!("    update  the config, Obsidian's templates folder, and links to notes that moved");
    if dry_run {
        println!("\nNothing was changed. Run `story-time upgrade` to make these changes.");
        return Ok(());
    }
    let backup = upgrade::apply(vault, &plan, now)?;
    println!(
        "\nUpgraded. The bookshelf as it was is in {}.",
        shown(&backup)
    );
    if plan.moves.iter().any(|(_, to)| {
        to.file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("old-"))
    }) {
        println!(
            "Templates you had changed are kept as templates/old-*.md: copy what you want into the new ones."
        );
    }
    Ok(())
}

pub fn new(
    vault: &Vault,
    world: Option<&str>,
    book: &NewBook,
    from_idea: Option<Option<String>>,
    now: NaiveDateTime,
) -> Result<()> {
    let idea = from_idea.map(|name| name.unwrap_or_else(|| book.title.to_string()));
    let spec = NewProject { world, book: *book };
    let (project, created) = scaffold::start_project(vault, &spec, idea.as_deref(), now)?;
    let slug = &project.slug;
    println!("Created {slug}/");
    println!("  {slug}.md  the project note: the world and its books, in order");
    println!("  bible/  characters/, locations/, world/ and the timeline, shared by every book");
    print_book(&created, book);
    if idea.is_some() {
        println!(
            "\nCopied the premise from {} and linked the idea to the project.",
            vault.relative(&vault.ideas_path()).display()
        );
    }
    println!("\nNext: story-time add scene \"Scene title\" -p {slug}");
    Ok(())
}

pub fn new_book(
    vault: &Vault,
    cwd: &Path,
    project: &str,
    book: &NewBook,
    now: NaiveDateTime,
) -> Result<()> {
    let project = vault.find_project(Some(project), cwd)?;
    let created = scaffold::start_book(vault, &project, book, now)?;
    println!("Added {} to {}/", created.title(), project.slug);
    print_book(&created, book);
    println!(
        "\nNext: story-time add scene \"Scene title\" -p {} -b {}",
        project.slug, created.slug
    );
    Ok(())
}

fn print_book(book: &Book, spec: &NewBook) {
    let slug = &book.slug;
    let target = book.target().unwrap_or(spec.form.default_target());
    println!("  books/{slug}/");
    println!("    {slug}.md       book note: logline, premise, contents, status and target");
    println!(
        "    {slug}-flow.md  story flow: {} for about {} words",
        spec.structure.name,
        thousands(target as i64)
    );
    println!("    scenes/");
}

#[allow(clippy::too_many_arguments)]
pub fn add(
    vault: &Vault,
    cwd: &Path,
    kind: NoteKind,
    name: &str,
    project: Option<&str>,
    book: Option<&str>,
    place: Place,
    now: NaiveDateTime,
) -> Result<()> {
    let project = vault.find_project(project, cwd)?;
    let note = if kind == NoteKind::Scene {
        let book = project.find_book(book, cwd)?;
        let note = scaffold::add_scene(vault, &project, &book, name, place, now.date())?;
        println!("Created {}", vault.relative(&note.path).display());
        let title = book.title();
        match place {
            Place::Unplaced => println!("It is unplaced: {title}'s contents don't list it yet."),
            Place::End => {
                println!("Listed at the end of {title}'s contents, as a chapter of its own.")
            }
            Place::Chapter(n) => println!(
                "Listed at the end of chapter {} in {title}'s contents.",
                n + 1
            ),
        }
        note
    } else {
        if place != Place::End {
            bail!("--chapter and --unplaced are for scenes");
        }
        let note = scaffold::add_note(vault, &project, kind, name, now.date())?;
        println!("Created {}", vault.relative(&note.path).display());
        note
    };
    if let Some(other) = note.same_name {
        eprintln!(
            "Note: {} has the same name, so Obsidian links to that name may go to either note.",
            vault.relative(&other).display()
        );
    }
    Ok(())
}

pub fn stats_all(vault: &Vault, now: NaiveDateTime, json: bool) -> Result<()> {
    let (measured, history) = vault.snapshot(now)?;
    if json {
        return json::print(&json::stats(vault, &measured, &history, now.date()));
    }
    if measured.is_empty() {
        println!("No books yet. Start one with: story-time new \"Title\"");
        return Ok(());
    }
    let mut table = Table::new(&[
        ("Book", Align::Left),
        ("World", Align::Left),
        ("Status", Align::Left),
        ("Form", Align::Left),
        ("Scenes", Align::Right),
        ("Words", Align::Right),
        ("Target", Align::Right),
        ("Progress", Align::Left),
    ]);
    for Measured {
        project,
        book,
        contents,
        words,
    } in &measured
    {
        let target = book.target();
        let (title, world) = (book.title(), project.title());
        table.row(vec![
            title.clone(),
            if world == title { String::new() } else { world },
            book.status().unwrap_or_default(),
            book.form()
                .map(|form| form.label().to_string())
                .unwrap_or_default(),
            contents.scenes().count().to_string(),
            thousands(*words as i64),
            target.map(|t| thousands(t as i64)).unwrap_or_default(),
            target
                .map(|t| {
                    format!(
                        "{} {}",
                        bar(*words as f64 / t as f64, 20),
                        percent(*words, t)
                    )
                })
                .unwrap_or_default(),
        ]);
    }
    println!();
    table.print(2);
    let total = measured.iter().map(|m| m.words).sum();
    println!();
    println!(
        "  {} in all · {} today of a {}-word goal · {}",
        plural(total, "word", "words"),
        signed(history.written_on(now.date(), None)),
        thousands(vault.config.daily_goal as i64),
        streak(history.streak(now.date())),
    );
    Ok(())
}

pub fn stats_project(
    vault: &Vault,
    cwd: &Path,
    query: &str,
    now: NaiveDateTime,
    json: bool,
) -> Result<()> {
    let project = vault.find_project(Some(query), cwd)?;
    let (measured, history) = vault.snapshot(now)?;
    let books: Vec<Measured> = measured
        .into_iter()
        .filter(|m| m.project.slug == project.slug)
        .collect();
    if json {
        let sizes = json::ProjectSizes {
            characters: project.notes(NoteKind::Character)?.len(),
            locations: project.notes(NoteKind::Location)?.len(),
            world: project.notes(NoteKind::World)?.len(),
            research: scratchpad::research(&project.research_dir())?.len(),
        };
        let scratchpad = Scratchpad::of_project(&project)?;
        let report = json::project_stats(
            vault,
            &project,
            &books,
            &history,
            now.date(),
            sizes,
            &scratchpad,
        );
        return json::print(&report);
    }
    if books.is_empty() {
        println!(
            "\n  {} has no books yet. Add one with: story-time new \"Title\" --in {}",
            project.title(),
            project.slug
        );
    }
    for measured in &books {
        book_stats(vault, measured, &history, now);
    }
    let bible = BIBLE
        .iter()
        .map(|kind| {
            let count = project.notes(*kind)?.len() as u64;
            Ok(match kind {
                NoteKind::Character => plural(count, "character", "characters"),
                NoteKind::Location => plural(count, "location", "locations"),
                _ => plural(count, "world note", "world notes"),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    println!(
        "\n  World bible of {}: {}",
        project.title(),
        bible.join(" · ")
    );
    let scratchpad = Scratchpad::of_project(&project)?;
    let research = scratchpad::research(&project.research_dir())?;
    println!(
        "  Scratchpad: {}, {} · Research: {}",
        plural(scratchpad.notes.len() as u64, "note", "notes"),
        plural(scratchpad.words, "word", "words"),
        plural(research.len() as u64, "note", "notes"),
    );
    Ok(())
}

fn book_stats(vault: &Vault, measured: &Measured, history: &History, now: NaiveDateTime) {
    let Measured {
        project,
        book,
        contents,
        words,
    } = measured;
    let words = *words;
    println!("\n  {}", book.title());
    let details: Vec<String> = [
        book.status(),
        book.form().map(|form| form.label().to_string()),
        Some(project.genre().join(", ")).filter(|genre| !genre.is_empty()),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !details.is_empty() {
        println!("  {}", details.join(" · "));
    }
    println!();
    if contents.scenes().next().is_none() && contents.chapters().next().is_none() {
        println!(
            "  No scenes yet. Add one with: story-time add scene \"Title\" -p {} -b {}",
            project.slug, book.slug
        );
    } else {
        let mut table = Table::new(&[
            ("Chapter", Align::Left),
            ("Scene", Align::Left),
            ("Status", Align::Left),
            ("POV", Align::Left),
            ("Words", Align::Right),
        ]);
        let row = |first: String, scene: &Scene| {
            vec![
                first,
                scene.title(),
                scene.status().unwrap_or_default(),
                scene.pov().unwrap_or_default(),
                thousands(scene.words as i64),
            ]
        };
        let blank = || vec![String::new(); 5];
        for part in &contents.parts {
            if let Some(title) = &part.title {
                let mut heading = blank();
                heading[0] = title.to_uppercase();
                table.row(heading);
            }
            for chapter in &part.chapters {
                if chapter.scenes.is_empty() {
                    let mut empty = blank();
                    empty[0] = chapter.heading();
                    empty[1] = "(no scenes yet)".to_string();
                    table.row(empty);
                }
                for (i, scene) in chapter.scenes.iter().enumerate() {
                    let first = if i == 0 {
                        chapter.heading()
                    } else {
                        String::new()
                    };
                    table.row(row(first, scene));
                }
            }
        }
        for (i, scene) in contents.unplaced.iter().enumerate() {
            let first = if i == 0 {
                "Unplaced".to_string()
            } else {
                String::new()
            };
            table.row(row(first, scene));
        }
        let mut total = blank();
        total[1] = "Total".to_string();
        total[4] = thousands(words as i64);
        table.row(total);
        table.print(2);
    }
    println!();

    if let Some(target) = book.target() {
        let left = target.saturating_sub(words);
        let remaining = if left == 0 {
            "target reached".to_string()
        } else {
            format!("{} to go", thousands(left as i64))
        };
        println!(
            "  Progress     {} {} of {} ({remaining})",
            bar(words as f64 / target as f64, 20),
            percent(words, target),
            thousands(target as i64)
        );
        if let Some(deadline) = book.deadline() {
            let days_left = (deadline - now.date()).num_days();
            if days_left > 0 && left > 0 {
                let per_day = left.div_ceil(days_left as u64);
                println!(
                    "  Deadline     {deadline} · {} left · {} a day",
                    plural(days_left as u64, "day", "days"),
                    plural(per_day, "word", "words")
                );
            } else if left > 0 {
                println!("  Deadline     {deadline} has passed");
            }
        }
    }
    let placed = contents.placed_words();
    if placed > 0 {
        let form = Form::for_length(placed);
        println!(
            "  Length       {} territory ({})",
            form.label(),
            form.range()
        );
        let pages = placed.div_ceil(vault.config.words_per_page.max(1));
        let minutes = placed.div_ceil(vault.config.reading_wpm.max(1));
        println!(
            "  Manuscript   about {} · {} to read",
            plural(pages, "page", "pages"),
            duration(minutes)
        );
    }
    if !contents.unplaced.is_empty() {
        println!(
            "  Unplaced     {} · {}, not in the manuscript",
            plural(contents.unplaced.len() as u64, "scene", "scenes"),
            plural(words - placed, "word", "words")
        );
    }
    println!(
        "  Today        {} words",
        signed(history.written_on(now.date(), Some(&book.log_key())))
    );
}

pub fn outline(
    vault: &Vault,
    cwd: &Path,
    project: Option<&str>,
    book: Option<&str>,
    json: bool,
) -> Result<()> {
    let project = vault.find_project(project, cwd)?;
    let book = project.find_book(book, cwd)?;
    let contents = book.contents()?;
    if json {
        return json::print(&json::outline(vault, &project, &book, &contents));
    }
    println!("\n  {}: outline\n", book.title());
    if contents.scenes().next().is_none() && contents.chapters().next().is_none() {
        println!("  No scenes yet.");
        return Ok(());
    }
    for part in &contents.parts {
        if let Some(title) = &part.title {
            println!("  {}\n", title.to_uppercase());
        }
        for chapter in &part.chapters {
            println!("  {}", chapter.heading());
            if chapter.scenes.is_empty() {
                println!("    (no scenes yet)");
            }
            for scene in &chapter.scenes {
                print_scene(scene);
            }
            println!();
        }
    }
    if !contents.unplaced.is_empty() {
        println!("  Unplaced (not in the contents)");
        for scene in &contents.unplaced {
            print_scene(scene);
        }
        println!();
    }
    Ok(())
}

fn print_scene(scene: &Scene) {
    let facts: Vec<String> = [
        scene.status(),
        scene.pov(),
        scene.location(),
        scene.when(),
        Some(plural(scene.words as u64, "word", "words")),
    ]
    .into_iter()
    .flatten()
    .collect();
    println!("    {}  [{}]", scene.title(), facts.join(" · "));
    let synopsis = scene
        .synopsis()
        .unwrap_or_else(|| "(no synopsis yet)".to_string());
    for line in wrap(&synopsis, 70) {
        println!("      {line}");
    }
}

pub fn today(vault: &Vault, now: NaiveDateTime, json: bool) -> Result<()> {
    let (measured, history) = vault.snapshot(now)?;
    let day = now.date();
    if json {
        let scratchpads = vault.scratchpads()?;
        return json::print(&json::today(vault, &measured, &scratchpads, &history, day));
    }
    let written = history.written_on(day, None);
    let goal = vault.config.daily_goal;
    let progress = written.max(0) as u64;
    println!("\n  {}", day.format("%A %-d %B %Y"));
    println!(
        "  {} words  {}  {} of your {}-word goal{}",
        signed(written),
        bar(
            if goal > 0 {
                progress as f64 / goal as f64
            } else {
                0.0
            },
            20
        ),
        percent(progress, goal),
        thousands(goal as i64),
        if goal > 0 && progress >= goal {
            " ✓"
        } else {
            ""
        },
    );
    let mut table = Table::new(&[("", Align::Left), ("", Align::Right)]);
    let books = measured.iter().map(|m| (m.book.title(), m.book.log_key()));
    let scratchpads = vault
        .scratchpads()?
        .into_iter()
        .map(|scratchpad| (scratchpad.title(), scratchpad.log_key()));
    for (title, key) in books.chain(scratchpads) {
        let words = history.written_on(day, Some(&key));
        if words != 0 {
            table.row(vec![title, signed(words)]);
        }
    }
    table.print(4);
    println!("  {}", streak(history.streak(day)));
    if history.daily(None).keys().all(|logged| *logged >= day) {
        println!(
            "\n  The log starts today. Words written from now on are counted each time you run"
        );
        println!(
            "  stats, today or history, so run one after each session, or schedule `story-time snapshot`."
        );
    }
    Ok(())
}

pub fn history(vault: &Vault, days: u32, now: NaiveDateTime, json: bool) -> Result<()> {
    let (_, history) = vault.snapshot(now)?;
    let daily = history.daily(None);
    let goal = vault.config.daily_goal as i64;
    let today = now.date();
    let range: Vec<_> = (0..u64::from(days))
        .rev()
        .filter_map(|back| today.checked_sub_days(Days::new(back)))
        .collect();
    let written: Vec<i64> = range
        .iter()
        .map(|day| daily.get(day).copied().unwrap_or(0))
        .collect();
    if json {
        return json::print(&json::history(vault.config.daily_goal, &range, &written));
    }
    let scale = written.iter().copied().max().unwrap_or(0).max(goal).max(1) as f64;
    println!();
    for (day, words) in range.iter().zip(&written) {
        let shown = if *words == 0 {
            "·".to_string()
        } else {
            signed(*words)
        };
        let mark = if goal > 0 && *words >= goal {
            " ✓"
        } else {
            ""
        };
        println!(
            "  {}  {shown:>7}  {}{mark}",
            day.format("%a %e %b"),
            bar(*words as f64 / scale, 30)
        );
    }
    let total: i64 = written.iter().sum();
    let active = written.iter().filter(|words| **words > 0).count() as u64;
    let best = written.iter().copied().max().unwrap_or(0).max(0);
    println!();
    println!(
        "  {} over {} · wrote on {} of them · best {} · goal {}",
        plural(total.max(0) as u64, "word", "words"),
        plural(u64::from(days), "day", "days"),
        active,
        thousands(best),
        thousands(goal),
    );
    Ok(())
}

/// `-` means the text comes from standard input.
fn text_or_stdin(text: &str) -> Result<String> {
    if text != "-" {
        return Ok(text.to_string());
    }
    io::read_to_string(io::stdin()).context("reading standard input")
}

pub fn scrap_add(
    vault: &Vault,
    cwd: &Path,
    text: &str,
    project: Option<&str>,
    source: Option<&str>,
    now: NaiveDateTime,
) -> Result<()> {
    let text = text_or_stdin(text)?;
    if text.trim().is_empty() {
        bail!("there is no text to keep");
    }
    let project = project
        .map(|query| vault.find_project(Some(query), cwd))
        .transpose()?;
    let added = scaffold::add_scrap(vault, project.as_ref(), &text, source, now)?;
    println!("Created {}", vault.relative(&added.path).display());
    Ok(())
}

pub fn research_add(
    vault: &Vault,
    cwd: &Path,
    project: Option<&str>,
    research: &NewResearch,
    text: Option<&str>,
    now: NaiveDateTime,
) -> Result<()> {
    let text = text.map(text_or_stdin).transpose()?;
    let project = project
        .map(|query| vault.find_project(Some(query), cwd))
        .transpose()?;
    let research = NewResearch {
        text: text.as_deref(),
        ..*research
    };
    let added = scaffold::add_research(vault, project.as_ref(), &research, now.date())?;
    println!("Created {}", vault.relative(&added.path).display());
    if let Some(other) = added.same_name {
        eprintln!(
            "Note: {} has the same name, so Obsidian links to that name may go to either note.",
            vault.relative(&other).display()
        );
    }
    Ok(())
}

pub fn ideas(vault: &Vault, json: bool) -> Result<()> {
    let entries = ideas::list_ideas(&ideas::read(vault)?);
    if json {
        return json::print(&json::ideas(entries));
    }
    if entries.is_empty() {
        println!("No ideas yet. Capture one with: story-time idea \"Name: description\"");
        return Ok(());
    }
    let projects = vault.projects()?;
    let mut section = None;
    for idea in &entries {
        if section != Some(&idea.section) {
            println!("\n  {}", idea.section);
            section = Some(&idea.section);
        }
        let started = idea
            .project
            .as_ref()
            .and_then(|slug| projects.iter().find(|project| project.slug == *slug))
            .map(|project| format!("  → {}", project.title()))
            .unwrap_or_default();
        println!("    {}{started}", idea.text);
    }
    println!();
    Ok(())
}

pub fn idea(vault: &Vault, text: &str, non_fiction: bool) -> Result<()> {
    let section = if non_fiction {
        "Non-Fiction"
    } else {
        "Fiction"
    };
    ideas::capture(vault, section, text)?;
    println!(
        "Added to {} under {section}.",
        vault.relative(&vault.ideas_path()).display()
    );
    Ok(())
}

pub fn check(vault: &Vault, cwd: &Path, project: Option<&str>, json: bool) -> Result<()> {
    let project = vault.find_project(project, cwd)?;
    let report = check::check(vault, &project)?;
    if json {
        return json::print(&json::check(vault, &project, report));
    }
    println!("\n  World bible check: {}\n", project.title());
    if report.is_clean() {
        println!("  Nothing to report.");
        return Ok(());
    }
    for (heading, links) in [
        (
            "Linked, but no note by that name yet:",
            &report.undocumented,
        ),
        (
            "Links that could mean more than one note (Obsidian may open the wrong one):",
            &report.ambiguous_links,
        ),
        (
            "Links by a note's name rather than its file name (Obsidian can't follow them):",
            &report.alias_links,
        ),
    ] {
        if links.is_empty() {
            continue;
        }
        println!("  {heading}");
        let mut table = Table::new(&[("Link", Align::Left), ("Linked from", Align::Left)]);
        for (target, sources) in links {
            table.row(vec![
                target.clone(),
                sources.iter().cloned().collect::<Vec<_>>().join(", "),
            ]);
        }
        table.print(4);
        println!();
    }
    if !report.broken_entries.is_empty() {
        println!("  In a list of books or a book's contents, but not one of its books or scenes:");
        let mut table = Table::new(&[("Entry", Align::Left), ("Listed in", Align::Left)]);
        for (target, sources) in &report.broken_entries {
            table.row(vec![
                target.clone(),
                sources.iter().cloned().collect::<Vec<_>>().join(", "),
            ]);
        }
        table.print(4);
        println!();
    }
    if !report.not_on_page.is_empty() {
        println!(
            "  Documented, but not on the page yet (no scene mentions the name, title or an alias):"
        );
        let mut table = Table::new(&[("Kind", Align::Left), ("Note", Align::Left)]);
        for (kind, name) in &report.not_on_page {
            table.row(vec![kind.key().to_string(), name.clone()]);
        }
        table.print(4);
        println!();
    }
    if !report.near_misses.is_empty() {
        println!("  Spellings close to a world-bible name (typos?):");
        let mut table = Table::new(&[
            ("Word", Align::Left),
            ("Close to", Align::Left),
            ("In", Align::Left),
        ]);
        for miss in &report.near_misses {
            let close = if miss.case_only {
                format!("{} (capitals differ)", miss.term)
            } else if miss.term == miss.name {
                miss.term.clone()
            } else {
                format!("{} ({})", miss.term, miss.name)
            };
            let scenes = miss.scenes.iter().cloned().collect::<Vec<_>>().join(", ");
            table.row(vec![miss.word.clone(), close, scenes]);
        }
        table.print(4);
        println!(
            "    A spelling that's meant stops being flagged once it's one of the note's aliases.\n"
        );
    }
    for (heading, renames) in [
        ("Names that aren't kebab-case:", &report.misnamed),
        (
            "File names that no longer match the note's name:",
            &report.drifted,
        ),
    ] {
        if renames.is_empty() {
            continue;
        }
        println!("  {heading}");
        let mut table = Table::new(&[("File", Align::Left), ("Rename to", Align::Left)]);
        for (path, name) in renames {
            table.row(vec![
                vault.relative(path).display().to_string(),
                name.clone(),
            ]);
        }
        table.print(4);
        println!("    A note can be renamed to match with `story-time rename <file>`.\n");
    }
    for (label, scenes) in [
        (
            "Listed more than once in the contents (only the first counts)",
            &report.repeated,
        ),
        ("Scenes without a status", &report.without_status),
        ("Scenes without a synopsis", &report.without_synopsis),
        ("Empty scenes", &report.empty_scenes),
    ] {
        if !scenes.is_empty() {
            println!("  {label}: {}", scenes.join(", "));
        }
    }
    if !report.unreadable_frontmatter.is_empty() {
        println!("\n  Properties (frontmatter) that are not valid YAML:");
        for (path, error) in &report.unreadable_frontmatter {
            println!("    {}: {error}", vault.relative(path).display());
        }
    }
    Ok(())
}

pub fn rename(vault: &Vault, cwd: &Path, note: &str, name: Option<&str>) -> Result<()> {
    let path = rename::find(vault, cwd, note)?;
    let old_name = story_time_core::note::Note::load(&path)?.title();
    let renamed = rename::rename(vault, &path, name)?;
    let file = vault.relative(&renamed.path).display();
    if renamed.path == path && renamed.name == old_name {
        println!("Nothing to rename: {file} already matches its name, {old_name}.");
        return Ok(());
    }
    if renamed.name == old_name {
        println!("Renamed {} to {file}.", vault.relative(&path).display());
    } else {
        println!("Renamed {old_name} to {}: {file}.", renamed.name);
    }
    if renamed.links > 0 {
        println!(
            "Updated {} in {}.",
            plural(renamed.links as u64, "link", "links"),
            plural(renamed.notes as u64, "note", "notes")
        );
    }
    if let Some(other) = renamed.same_name {
        println!(
            "{} has the same file name, so bare links to it may open either note in Obsidian.",
            vault.relative(&other).display()
        );
    }
    Ok(())
}

pub fn compile(
    vault: &Vault,
    cwd: &Path,
    project: Option<&str>,
    book: Option<&str>,
    format: Format,
    output: Option<PathBuf>,
) -> Result<()> {
    let project = vault.find_project(project, cwd)?;
    let book = project.find_book(book, cwd)?;
    let manuscript = compile::manuscript(&book, vault.config.author.as_deref())?;
    if manuscript.scenes == 0 {
        bail!(
            "{} has no scenes with words in them in its contents yet",
            book.title()
        );
    }
    let out = output.unwrap_or_else(|| {
        vault
            .build_dir()
            .join(&project.slug)
            .join(format!("{}.{}", book.slug, format.key()))
    });
    compile::export(&manuscript.markdown, &out, format)?;
    println!(
        "Compiled {} ({}, {}) into {}",
        plural(manuscript.chapters as u64, "chapter", "chapters"),
        plural(manuscript.scenes as u64, "scene", "scenes"),
        plural(manuscript.words as u64, "word", "words"),
        out.display()
    );
    if !manuscript.skipped.is_empty() {
        println!(
            "Left out, as they have no words yet: {}",
            manuscript.skipped.join(", ")
        );
    }
    Ok(())
}

fn streak(days: u32) -> String {
    match days {
        0 => "no current streak".to_string(),
        1 => "1-day streak".to_string(),
        n => format!("{n}-day streak"),
    }
}

fn duration(minutes: u64) -> String {
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}
