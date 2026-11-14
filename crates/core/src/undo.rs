//! Undo history for moves in a book's contents. The moves were made on one computer, so the
//! history belongs to it rather than to the bookshelf: it is kept in Story Time's data folder,
//! where a synced folder doesn't carry it to other computers and git doesn't see it, and it lasts
//! after the app closes. Each book has a small TOML file, named by the book's ID, holding the
//! contents as they were before each move, newest last.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::contents::{self, Moved};
use crate::note::content_hash;
use crate::project::Book;
use crate::vault::{Vault, data_dir};

/// How many moves each book keeps to undo.
pub const LIMIT: usize = 100;

#[derive(Debug, Default, Serialize, Deserialize)]
struct History {
    /// The book note's hash as Story Time last left it, after the latest move or undo. An undo
    /// goes ahead only if the note still has it, so it never overwrites a change made since.
    written: String,
    moves: Vec<Step>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Step {
    /// What moved: a scene's name or a chapter's heading.
    what: String,
    /// The `## Contents` section's lines before the move.
    previous: Vec<String>,
}

/// The undo history this computer keeps for one bookshelf's books.
pub struct Undo {
    dir: PathBuf,
}

impl Undo {
    /// The history for `vault`, in its folder in `undo/` in Story Time's data folder.
    pub fn for_vault(vault: &Vault) -> Result<Undo> {
        Ok(Undo::in_dir(vault.folder_in(&data_dir()?.join("undo"))))
    }

    /// The history kept in `dir`.
    pub fn in_dir(dir: PathBuf) -> Undo {
        Undo { dir }
    }

    fn path(&self, book: &Book) -> PathBuf {
        let name = book
            .id()
            .unwrap_or_else(|| format!("{}--{}", book.project, book.slug));
        self.dir.join(format!("{name}.toml"))
    }

    /// The book's history, or an empty one if it has none or its file can't be read.
    fn load(&self, book: &Book) -> History {
        fs::read_to_string(self.path(book))
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn save(&self, book: &Book, history: &History) -> Result<()> {
        if history.moves.is_empty() {
            return self.forget(book);
        }
        let path = self.path(book);
        fs::create_dir_all(&self.dir)
            .with_context(|| format!("creating {}", self.dir.display()))?;
        let text = format!(
            "# Moves on Story Time's Board that can still be undone in \"{}\", newest last, kept on\n\
             # this computer only. Each holds the book's contents as they were before the move. An\n\
             # undo happens only if the book note is still as Story Time left it, with the hash in\n\
             # `written`. Deleting this file forgets them.\n\n{}",
            book.title(),
            toml::to_string_pretty(history).context("writing the undo history")?
        );
        fs::write(&path, text).with_context(|| format!("writing {}", path.display()))
    }

    /// Forgets the book's history, as when it's deleted.
    pub fn forget(&self, book: &Book) -> Result<()> {
        match fs::remove_file(self.path(book)) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                Err(error).with_context(|| format!("removing {}", self.path(book).display()))
            }
            _ => Ok(()),
        }
    }

    /// Records a move made in `book` from the content `base_hash` was taken from. The moves
    /// before it can still be undone only if nothing else changed the note between them and it.
    pub fn record(&self, book: &Book, base_hash: &str, moved: &Moved) -> Result<()> {
        let mut history = self.load(book);
        if history.written != base_hash {
            history.moves.clear();
        }
        history.moves.push(Step {
            what: moved.what.clone(),
            previous: moved.previous.clone(),
        });
        if history.moves.len() > LIMIT {
            history.moves.remove(0);
        }
        history.written = moved.hash.clone();
        self.save(book, &history)
    }

    /// Undoes `book`'s latest move, putting its contents back as they were, and returns what
    /// moved back, or `None` if there's no move to undo. If the book note has changed since
    /// Story Time last left it, nothing is written and the whole history is forgotten, since none
    /// of it can be undone safely any more.
    pub fn undo(&self, book: &Book) -> Result<Option<String>> {
        let mut history = self.load(book);
        let Some(step) = history.moves.pop() else {
            return Ok(None);
        };
        let path = &book.note.path;
        let content =
            fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        if content_hash(&content) != history.written {
            self.forget(book)?;
            bail!(
                "{} has changed since that move, so it can't be undone",
                book.title()
            );
        }
        history.written = contents::restore(book, &history.written, &step.previous)?;
        self.save(book, &history)?;
        Ok(Some(step.what))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contents::{ChapterSpot, Spot, move_chapter, move_scene};
    use crate::project::Form;
    use crate::scaffold::{NewBook, NewProject, add_scene, create_project};
    use crate::structures::THREE_ACT;
    use chrono::NaiveDate;
    use tempfile::TempDir;

    struct Shelf {
        _dir: TempDir,
        vault: Vault,
        book: Book,
        undo: Undo,
        scenes: Vec<PathBuf>,
    }

    impl Shelf {
        fn new() -> Shelf {
            let dir = TempDir::new().unwrap();
            let root = dir.path().join("shelf");
            fs::create_dir_all(root.join(".obsidian")).unwrap();
            let vault = Vault::open(&root).unwrap();
            let today = NaiveDate::from_ymd_opt(2026, 11, 14).unwrap();
            let new = NewProject {
                world: None,
                book: NewBook {
                    title: "Sample Story",
                    form: Form::Novel,
                    target: None,
                    structure: &THREE_ACT,
                    premise: None,
                },
            };
            let (project, book) = create_project(&vault, &new, today).unwrap();
            let scenes = ["Arrival", "First Light", "Leaving Home"]
                .iter()
                .map(|name| {
                    let book = Book::load(&book.dir, &project.slug).unwrap().unwrap();
                    let place = contents::Place::End;
                    add_scene(&vault, &project, &book, name, place, today)
                        .unwrap()
                        .path
                })
                .collect();
            let undo = Undo::in_dir(dir.path().join("data"));
            Shelf {
                _dir: dir,
                vault,
                book,
                undo,
                scenes,
            }
        }

        fn book(&self) -> Book {
            Book::load(&self.book.dir, &self.book.project)
                .unwrap()
                .unwrap()
        }

        fn order(&self) -> Vec<String> {
            let contents = self.book().contents().unwrap();
            contents.placed().map(|scene| scene.title()).collect()
        }

        /// Moves a scene as the Board does, and records it.
        fn move_scene(&self, scene: usize, to: Spot) {
            let book = self.book();
            let moved = move_scene(&self.vault, &book, &book.hash, &self.scenes[scene], &to)
                .unwrap()
                .unwrap();
            self.undo.record(&book, &book.hash, &moved).unwrap();
        }
    }

    #[test]
    fn undoes_moves_newest_first_even_after_the_app_closes() {
        let shelf = Shelf::new();
        let original = fs::read_to_string(&shelf.book.note.path).unwrap();
        shelf.move_scene(2, Spot::Before(shelf.scenes[0].clone()));
        shelf.move_scene(0, Spot::Unplaced);
        let book = shelf.book();
        let moved = move_chapter(&book, &book.hash, 1, ChapterSpot::Before(0))
            .unwrap()
            .unwrap();
        shelf.undo.record(&book, &book.hash, &moved).unwrap();
        assert_eq!(shelf.order(), ["First Light", "Leaving Home"]);

        let file = fs::read_to_string(shelf.undo.path(&shelf.book)).unwrap();
        assert!(file.starts_with("# Moves on Story Time's Board"), "{file}");
        assert!(file.contains("what = \"Chapter 2\""), "{file}");

        // A new Undo stands for the app opened again: the history is on disk.
        let reopened = Undo::in_dir(shelf.undo.dir.clone());
        let undone: Vec<String> = (0..3)
            .map(|_| reopened.undo(&shelf.book()).unwrap().unwrap())
            .collect();
        assert_eq!(undone, ["Chapter 2", "Arrival", "Leaving Home"]);
        assert_eq!(reopened.undo(&shelf.book()).unwrap(), None);
        assert!(
            !reopened.path(&shelf.book).exists(),
            "an empty history leaves no file"
        );
        let restored = fs::read_to_string(&shelf.book.note.path).unwrap();
        let contents = |text: &str| {
            crate::note::section(text, contents::HEADING)
                .unwrap()
                .join("\n")
        };
        assert_eq!(contents(&restored), contents(&original));
    }

    #[test]
    fn never_undoes_over_a_change_made_since() {
        let shelf = Shelf::new();
        shelf.move_scene(2, Spot::Before(shelf.scenes[0].clone()));
        shelf.move_scene(0, Spot::End);
        let note = &shelf.book.note.path;
        let edited = fs::read_to_string(note).unwrap() + "\nAn edit made since.\n";
        fs::write(note, &edited).unwrap();
        assert!(shelf.undo.undo(&shelf.book()).is_err());
        assert_eq!(fs::read_to_string(note).unwrap(), edited);
        assert_eq!(
            shelf.undo.undo(&shelf.book()).unwrap(),
            None,
            "the history is forgotten"
        );

        // A move after a change elsewhere starts the history afresh.
        shelf.move_scene(1, Spot::Unplaced);
        let book = shelf.book();
        fs::write(note, fs::read_to_string(note).unwrap() + "More.\n").unwrap();
        let book_now = shelf.book();
        let moved = move_scene(
            &shelf.vault,
            &book_now,
            &book_now.hash,
            &shelf.scenes[1],
            &Spot::End,
        )
        .unwrap()
        .unwrap();
        shelf
            .undo
            .record(&book_now, &book_now.hash, &moved)
            .unwrap();
        assert_ne!(book.hash, book_now.hash);
        assert_eq!(
            shelf.undo.undo(&shelf.book()).unwrap().as_deref(),
            Some("First Light")
        );
        assert_eq!(shelf.undo.undo(&shelf.book()).unwrap(), None);
    }

    #[test]
    fn keeps_only_the_latest_moves() {
        let shelf = Shelf::new();
        for i in 0..LIMIT + 5 {
            let to = if i % 2 == 0 {
                Spot::Unplaced
            } else {
                Spot::End
            };
            shelf.move_scene(0, to);
        }
        assert_eq!(shelf.undo.load(&shelf.book).moves.len(), LIMIT);
    }
}
