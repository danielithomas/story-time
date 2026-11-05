//! `story-time`: organise, measure and compile the stories in a Markdown (Obsidian) vault.

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
use story_time_core::scaffold::NewBook;
use story_time_core::structures::STRUCTURES;
use story_time_core::{Form, NoteKind, Vault};

use crate::output::thousands;

#[derive(Parser)]
#[command(
    name = "story-time",
    version,
    about = "Organise, measure and compile the stories in a Markdown (Obsidian) vault."
)]
struct Cli {
    /// The vault folder. Defaults to the nearest folder holding .story-time or .obsidian, then
    /// to `vault` in ~/.config/story-time/config.toml.
    #[arg(long, global = true, env = "STORY_TIME_VAULT")]
    vault: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Set up a vault: config, note templates, and the folder for Obsidian's Templates plugin.
    Init {
        /// Replace existing templates with the built-in versions.
        #[arg(long)]
        force: bool,
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
    /// Find world-bible gaps: links to notes that don't exist yet, notes that never appear on the
    /// page, contents that don't match the scenes, and scenes missing a status or synopsis.
    Check { project: Option<String> },
    /// Stitch the scenes a book's contents list into one manuscript.
    Compile {
        project: Option<String>,
        /// The book (default: the book you are in, or the project's only one).
        #[arg(short, long)]
        book: Option<String>,
        #[arg(long, default_value = "md", value_parser = format_values())]
        format: String,
        /// Where to write it (default: .story-time/build/<project>/<book>.<format> in the vault).
        #[arg(short, long)]
        output: Option<PathBuf>,
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
    if let Command::Init { force } = cli.command {
        return commands::init(&cli.vault.unwrap_or(cwd), force);
    }
    let vault = Vault::discover(cli.vault.as_deref(), &cwd)?;
    let now = Local::now().naive_local();
    // Values below were validated by clap's possible-values parsers.
    match cli.command {
        Command::Init { .. } => unreachable!("handled above"),
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
        Command::Idea { text, non_fiction } => commands::idea(&vault, &text.join(" "), non_fiction),
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
