//! One function per subcommand: call into the core, then print for a person to read.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::{Days, NaiveDateTime};
use story_time_core::check::{self, BIBLE};
use story_time_core::compile::{self, Format};
use story_time_core::ideas;
use story_time_core::scaffold::{self, NewProject, Step};
use story_time_core::structures::Structure;
use story_time_core::vault::Measured;
use story_time_core::{Form, NoteKind, Vault};

use crate::output::{Align, Table, bar, percent, plural, signed, thousands, wrap};

pub fn init(root: &Path, force: bool) -> Result<()> {
    let root = root
        .canonicalize()
        .with_context(|| format!("{} not found", root.display()))?;
    let steps = scaffold::init_vault(&root, force)?;
    println!("Vault: {}", root.display());
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
    Ok(())
}

pub fn new(
    vault: &Vault,
    title: &str,
    form: Form,
    target: Option<u64>,
    structure: &'static Structure,
    from_idea: Option<Option<String>>,
    now: NaiveDateTime,
) -> Result<()> {
    let idea = from_idea.map(|name| name.unwrap_or_else(|| title.to_string()));
    let spec = NewProject {
        title,
        form,
        target,
        structure,
        premise: None,
    };
    let project = scaffold::start_project(vault, &spec, idea.as_deref(), now)?;

    let slug = &project.slug;
    let target = project.target().unwrap_or(form.default_target());
    println!("Created {slug}/");
    println!("  {slug}.md       project note: logline, premise, themes");
    println!(
        "  {slug}-flow.md  story flow: {} for about {} words",
        structure.name,
        thousands(target as i64)
    );
    println!("  chapters/  characters/  locations/  world/");
    if idea.is_some() {
        println!(
            "\nCopied the premise from {} and linked the idea to [[{slug}]].",
            vault.relative(&vault.ideas_path()).display()
        );
    }
    println!("\nNext: story-time add chapter \"Chapter title\" -p {slug}");
    Ok(())
}

pub fn add(
    vault: &Vault,
    cwd: &Path,
    kind: NoteKind,
    name: &str,
    project: Option<&str>,
    now: NaiveDateTime,
) -> Result<()> {
    let project = vault.find_project(project, cwd)?;
    let note = scaffold::add_note(vault, &project, kind, name, now.date())?;
    println!("Created {}", vault.relative(&note.path).display());
    if let Some(other) = note.same_name {
        eprintln!(
            "Note: {} has the same name, so Obsidian links to that name may go to either note.",
            vault.relative(&other).display()
        );
    }
    Ok(())
}

pub fn stats_all(vault: &Vault, now: NaiveDateTime) -> Result<()> {
    let (measured, history) = vault.snapshot(now)?;
    if measured.is_empty() {
        println!("No projects yet. Start one with: story-time new \"Title\"");
        return Ok(());
    }
    let mut table = Table::new(&[
        ("Project", Align::Left),
        ("Status", Align::Left),
        ("Form", Align::Left),
        ("Chapters", Align::Right),
        ("Words", Align::Right),
        ("Target", Align::Right),
        ("Progress", Align::Left),
    ]);
    for Measured {
        project,
        chapters,
        words,
    } in &measured
    {
        let target = project.target();
        table.row(vec![
            project.title(),
            project.status().unwrap_or_default(),
            project
                .form()
                .map(|form| form.label().to_string())
                .unwrap_or_default(),
            chapters.len().to_string(),
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

pub fn stats_project(vault: &Vault, cwd: &Path, query: &str, now: NaiveDateTime) -> Result<()> {
    let project = vault.find_project(Some(query), cwd)?;
    let (measured, history) = vault.snapshot(now)?;
    let Measured {
        project,
        chapters,
        words,
    } = measured
        .into_iter()
        .find(|m| m.project.slug == project.slug)
        .context("the project disappeared while being measured")?;

    println!("\n  {}", project.title());
    let details: Vec<String> = [
        project.status(),
        project.form().map(|form| form.label().to_string()),
        project.genre(),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !details.is_empty() {
        println!("  {}", details.join(" · "));
    }
    println!();
    if chapters.is_empty() {
        println!(
            "  No chapters yet. Add one with: story-time add chapter \"Title\" -p {}",
            project.slug
        );
    } else {
        let mut table = Table::new(&[
            ("#", Align::Right),
            ("Chapter", Align::Left),
            ("Status", Align::Left),
            ("POV", Align::Left),
            ("Words", Align::Right),
        ]);
        for chapter in &chapters {
            table.row(vec![
                chapter
                    .number()
                    .map(|n| format!("{n:02}"))
                    .unwrap_or_default(),
                chapter.title(),
                chapter.status().unwrap_or_default(),
                chapter.pov().unwrap_or_default(),
                thousands(chapter.words as i64),
            ]);
        }
        table.row(vec![
            String::new(),
            "Total".to_string(),
            String::new(),
            String::new(),
            thousands(words as i64),
        ]);
        table.print(2);
    }
    println!();

    if let Some(target) = project.target() {
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
        if let Some(deadline) = project.deadline() {
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
    if words > 0 {
        let form = Form::for_length(words);
        println!(
            "  Length       {} territory ({})",
            form.label(),
            form.range()
        );
        let pages = words.div_ceil(vault.config.words_per_page.max(1));
        let minutes = words.div_ceil(vault.config.reading_wpm.max(1));
        println!(
            "  Manuscript   about {} · {} to read",
            plural(pages, "page", "pages"),
            duration(minutes)
        );
    }
    println!(
        "  Today        {} words",
        signed(history.written_on(now.date(), Some(&project.log_key())))
    );
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
    println!("  Story bible  {}", bible.join(" · "));
    Ok(())
}

pub fn outline(vault: &Vault, cwd: &Path, project: Option<&str>) -> Result<()> {
    let project = vault.find_project(project, cwd)?;
    let chapters = project.chapters()?;
    println!("\n  {}: outline\n", project.title());
    if chapters.is_empty() {
        println!("  No chapters yet.");
        return Ok(());
    }
    for chapter in &chapters {
        let number = chapter
            .number()
            .map(|n| format!("{n:02}  "))
            .unwrap_or_default();
        let facts: Vec<String> = [
            chapter.status(),
            chapter.pov(),
            Some(plural(chapter.words as u64, "word", "words")),
        ]
        .into_iter()
        .flatten()
        .collect();
        println!("  {number}{}  [{}]", chapter.title(), facts.join(" · "));
        let synopsis = chapter
            .synopsis()
            .unwrap_or_else(|| "(no synopsis yet)".to_string());
        for line in wrap(&synopsis, 72) {
            println!("  {}{line}", " ".repeat(number.len()));
        }
        println!();
    }
    Ok(())
}

pub fn today(vault: &Vault, now: NaiveDateTime) -> Result<()> {
    let (measured, history) = vault.snapshot(now)?;
    let day = now.date();
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
    for Measured { project, .. } in &measured {
        let words = history.written_on(day, Some(&project.log_key()));
        if words != 0 {
            table.row(vec![project.title(), signed(words)]);
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

pub fn history(vault: &Vault, days: u32, now: NaiveDateTime) -> Result<()> {
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

pub fn check(vault: &Vault, cwd: &Path, project: Option<&str>) -> Result<()> {
    let project = vault.find_project(project, cwd)?;
    let report = check::check(vault, &project)?;
    println!("\n  Story bible check: {}\n", project.title());
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
    if !report.not_on_page.is_empty() {
        println!(
            "  Documented, but not on the page yet (no chapter mentions the name, title or an alias):"
        );
        let mut table = Table::new(&[("Kind", Align::Left), ("Note", Align::Left)]);
        for (kind, name) in &report.not_on_page {
            table.row(vec![kind.key().to_string(), name.clone()]);
        }
        table.print(4);
        println!();
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
        println!();
    }
    for (label, chapters) in [
        ("Chapters without a status", &report.without_status),
        ("Chapters without a synopsis", &report.without_synopsis),
        ("Empty chapters", &report.empty_chapters),
    ] {
        if !chapters.is_empty() {
            println!("  {label}: {}", chapters.join(", "));
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

pub fn compile(
    vault: &Vault,
    cwd: &Path,
    project: Option<&str>,
    format: Format,
    output: Option<PathBuf>,
) -> Result<()> {
    let project = vault.find_project(project, cwd)?;
    let manuscript = compile::manuscript(&project, vault.config.author.as_deref())?;
    if manuscript.chapters == 0 {
        bail!("{} has no chapters with words in them yet", project.title());
    }
    let out = output.unwrap_or_else(|| {
        vault
            .build_dir()
            .join(format!("{}.{}", project.slug, format.key()))
    });
    compile::export(&manuscript.markdown, &out, format)?;
    println!(
        "Compiled {} ({}) into {}",
        plural(manuscript.chapters as u64, "chapter", "chapters"),
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
