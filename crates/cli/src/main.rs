//! `story-time`: organise, measure and compile the stories on a bookshelf of Markdown files.

mod commands;
mod output;

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use chrono::Local;
use clap::builder::{PossibleValue, PossibleValuesParser};
use clap::{Parser, Subcommand};
use story_time_core::compile::Format;
use story_time_core::contents::Place;
use story_time_core::scaffold::{NewBook, NewResearch};
use story_time_core::structures::STRUCTURES;
use story_time_core::{Form, NoteKind, Vault};

use crate::output::thousands;

#[derive(Parser)]
#[command(
    name = "story-time",
    version,
    about = "Organise, measure and compile the stories on a bookshelf: a folder of Markdown files, which Obsidian can open too."
)]
struct Cli {
    /// The bookshelf folder. Defaults to the nearest folder holding .story-time or .obsidian,
    /// then to `bookshelf` in ~/.config/story-time/config.toml.
    #[arg(
        long,
        alias = "vault",
        value_name = "PATH",
        global = true,
        env = "STORY_TIME_BOOKSHELF"
    )]
    bookshelf: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Set up a bookshelf: config, note templates, and the folder for Obsidian's Templates plugin.
    Init {
        /// Replace existing templates with the built-in versions.
        #[arg(long)]
        force: bool,
        /// Add an example world with one short book whose scenes explain how story-time works.
        #[arg(long)]
        example: bool,
    },
    /// Start a book: in a new project (a world, with its world bible), or with --in, in an
    /// existing one.
    New {
        /// The book's title; its folder is named after it ("Sample Story" → sample-story).
        title: String,
        /// Name the new project's world, if it isn't the book's title.
        #[arg(long, conflicts_with = "in_project")]
        world: Option<String>,
        /// Add the book to this existing project (slug or title) instead of starting a new one.
        #[arg(long = "in", value_name = "PROJECT")]
        in_project: Option<String>,
        #[arg(long, default_value = "novel", value_parser = form_values())]
        form: String,
        /// Target length in words. Defaults to a typical length for the form.
        #[arg(long)]
        target: Option<u64>,
        /// How to lay out the story flow note.
        #[arg(long, default_value = "three-act", value_parser = structure_values())]
        structure: String,
        /// Copy the premise from the idea with this name in the ideas file (default: the title),
        /// and link the idea to the new project. Put this after the title.
        #[arg(long, value_name = "IDEA", conflicts_with = "in_project")]
        from_idea: Option<Option<String>>,
    },
    /// Add a scene to a book, or a character, location or world note to a project's world bible.
    Add {
        #[arg(value_parser = kind_values())]
        kind: String,
        /// The note's name.
        name: String,
        /// Project slug or title (default: the project you are in, or the only one).
        #[arg(short, long)]
        project: Option<String>,
        /// For a scene, the book (default: the book you are in, or the project's only one).
        #[arg(short, long)]
        book: Option<String>,
        /// Add the scene to the end of this chapter in the book's contents, counting from 1.
        /// Without it, the scene goes at the end of the contents as a chapter of its own.
        #[arg(long, value_name = "N", value_parser = clap::value_parser!(u32).range(1..))]
        chapter: Option<u32>,
        /// Leave the scene out of the book's contents, to place it later.
        #[arg(long, conflicts_with = "chapter")]
        unplaced: bool,
    },
    /// Word counts and progress for every book, or scene by scene for one project's books.
    Stats { project: Option<String> },
    /// Each scene's status, point of view and synopsis, chapter by chapter: a corkboard for the
    /// terminal.
    Outline {
        project: Option<String>,
        /// The book (default: the book you are in, or the project's only one).
        #[arg(short, long)]
        book: Option<String>,
    },
    /// Words written today against your daily goal, and your writing streak.
    Today,
    /// Words written per day.
    History {
        #[arg(long, default_value_t = 14)]
        days: u32,
    },
    /// Record word counts without printing anything (for cron jobs or git hooks).
    Snapshot,
    /// Capture an idea in the ideas file.
    Idea {
        /// The idea, written as "Name: description". Quotes are optional.
        #[arg(required = true, num_args = 1..)]
        text: Vec<String>,
        /// File it under Non-Fiction instead of Fiction.
        #[arg(long)]
        non_fiction: bool,
    },
    /// Keep a fragment in a scratchpad: the bookshelf's, or with --project, a world's. Scratchpad
    /// words count towards the daily goal but towards no book.
    Scrap {
        #[command(subcommand)]
        action: ScrapAction,
    },
    /// Keep research: what is true in the real world, with its source. Research isn't counted.
    Research {
        #[command(subcommand)]
        action: ResearchAction,
    },
    /// Find world-bible gaps: links to notes that don't exist yet, notes that never appear on the
    /// page, contents that don't match the scenes, and scenes missing a status or synopsis.
    Check { project: Option<String> },
    /// Convert a bookshelf from story-time 0.1's layout to the current one, after copying it into
    /// .story-time/backups/.
    Upgrade {
        /// List the changes without making them.
        #[arg(long)]
        dry_run: bool,
    },
    /// Back up the bookshelf now, as a zip with the copies the desktop app makes each day: in
    /// Story Time's data folder, or `backup_folder` in ~/.config/story-time/config.toml.
    Backup,
    /// Stitch the scenes a book's contents list into one manuscript.
    Compile {
        project: Option<String>,
        /// The book (default: the book you are in, or the project's only one).
        #[arg(short, long)]
        book: Option<String>,
        #[arg(long, default_value = "md", value_parser = format_values())]
        format: String,
        /// Where to write it (default: .story-time/build/<project>/<book>.<format> on the bookshelf).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum ScrapAction {
    /// Capture a note, named by the time of capture. Its text is kept exactly as given.
    Add {
        /// The text, or - to read it from standard input.
        #[arg(required = true, num_args = 1..)]
        text: Vec<String>,
        /// Put it in this project's scratchpad (slug or title) instead of the bookshelf's.
        #[arg(short, long)]
        project: Option<String>,
        /// Where it came from, such as `dictation`.
        #[arg(long)]
        source: Option<String>,
    },
}

#[derive(Subcommand)]
enum ResearchAction {
    /// Add a research note named after its subject.
    Add {
        /// What it is about: the note's name.
        name: String,
        /// Put it in this project's research (slug or title) instead of the bookshelf's.
        #[arg(short, long)]
        project: Option<String>,
        /// Where it came from: a URL or citation.
        #[arg(long)]
        source: Option<String>,
        /// What it says, or - to read it from standard input. Without it, the note starts from
        /// the template.
        #[arg(long)]
        text: Option<String>,
        /// An agent wrote it: the note opens with a callout asking the writer to check it against
        /// its source.
        #[arg(long)]
        agent: bool,
    },
}

fn form_values() -> PossibleValuesParser {
    PossibleValuesParser::new(Form::ALL.map(|form| {
        PossibleValue::new(form.key()).help(format!(
            "{}, {} (default target {})",
            form.label(),
            form.range(),
            thousands(form.default_target() as i64)
        ))
    }))
}

fn structure_values() -> PossibleValuesParser {
    PossibleValuesParser::new(
        STRUCTURES.map(|s| PossibleValue::new(s.key).help(format!("{}: {}", s.name, s.summary))),
    )
}

fn kind_values() -> PossibleValuesParser {
    PossibleValuesParser::new(
        NoteKind::ALL.map(|kind| PossibleValue::new(kind.key()).help(kind.describe())),
    )
}

fn format_values() -> PossibleValuesParser {
    PossibleValuesParser::new(
        Format::ALL.map(|format| PossibleValue::new(format.key()).help(format.describe())),
    )
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("story-time: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let cwd = env::current_dir()?;
    // Before 0.3 the bookshelf was called a vault, and the old variable still works.
    let bookshelf = cli
        .bookshelf
        .or_else(|| env::var_os("STORY_TIME_VAULT").map(PathBuf::from));
    let now = Local::now().naive_local();
    if let Command::Init { force, example } = cli.command {
        return commands::init(&bookshelf.unwrap_or(cwd), force, example, now);
    }
    let vault = Vault::discover(bookshelf.as_deref(), &cwd)?;
    if let Command::Upgrade { dry_run } = cli.command {
        return commands::upgrade(&vault, dry_run, now);
    }
    vault.config.ensure_current()?;
    // Values below were validated by clap's possible-values parsers.
    match cli.command {
        Command::Init { .. } | Command::Upgrade { .. } => unreachable!("handled above"),
        Command::New {
            title,
            world,
            in_project,
            form,
            target,
            structure,
            from_idea,
        } => {
            let form = Form::from_key(&form).expect("validated form");
            let structure =
                story_time_core::structures::find(&structure).expect("validated structure");
            let book = NewBook {
                title: &title,
                form,
                target,
                structure,
                premise: None,
            };
            match in_project {
                Some(project) => commands::new_book(&vault, &cwd, &project, &book, now),
                None => commands::new(&vault, world.as_deref(), &book, from_idea, now),
            }
        }
        Command::Add {
            kind,
            name,
            project,
            book,
            chapter,
            unplaced,
        } => {
            let kind = NoteKind::from_key(&kind).expect("validated kind");
            let place = match (chapter, unplaced) {
                (_, true) => Place::Unplaced,
                (Some(n), _) => Place::Chapter(n as usize - 1),
                (None, false) => Place::End,
            };
            commands::add(
                &vault,
                &cwd,
                kind,
                &name,
                project.as_deref(),
                book.as_deref(),
                place,
                now,
            )
        }
        Command::Stats { project: None } => commands::stats_all(&vault, now),
        Command::Stats {
            project: Some(project),
        } => commands::stats_project(&vault, &cwd, &project, now),
        Command::Outline { project, book } => {
            commands::outline(&vault, &cwd, project.as_deref(), book.as_deref())
        }
        Command::Today => commands::today(&vault, now),
        Command::History { days } => commands::history(&vault, days, now),
        Command::Snapshot => vault.snapshot(now).map(drop),
        Command::Backup => commands::backup(&vault, now),
        Command::Idea { text, non_fiction } => commands::idea(&vault, &text.join(" "), non_fiction),
        Command::Scrap {
            action:
                ScrapAction::Add {
                    text,
                    project,
                    source,
                },
        } => commands::scrap_add(
            &vault,
            &cwd,
            &text.join(" "),
            project.as_deref(),
            source.as_deref(),
            now,
        ),
        Command::Research {
            action:
                ResearchAction::Add {
                    name,
                    project,
                    source,
                    text,
                    agent,
                },
        } => commands::research_add(
            &vault,
            &cwd,
            project.as_deref(),
            &NewResearch {
                name: &name,
                source: source.as_deref(),
                text: None,
                by_agent: agent,
            },
            text.as_deref(),
            now,
        ),
        Command::Check { project } => commands::check(&vault, &cwd, project.as_deref()),
        Command::Compile {
            project,
            book,
            format,
            output,
        } => {
            let format = Format::from_key(&format).expect("validated format");
            commands::compile(
                &vault,
                &cwd,
                project.as_deref(),
                book.as_deref(),
                format,
                output,
            )
        }
    }
}
