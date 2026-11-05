//! The engine behind `story-time`.
//!
//! A vault is a plain folder of Markdown notes; an Obsidian vault works as-is. Each project is a
//! world: a folder at the vault root holding a note of the same name, its world bible in `bible/`
//! (characters, locations and world notes), and its books in `books/<book>/`, each with a book
//! note, a story flow and `chapters/`. Everything this crate knows comes from those files and
//! their YAML frontmatter, and everything it writes is ordinary Markdown, so the vault stays
//! readable and editable without it. `docs/vault-format.md` specifies the layout. Front ends (the
//! CLI and the desktop app) sit on top of this crate.

pub mod check;
pub mod compile;
pub mod config;
pub mod history;
pub mod ideas;
pub mod ids;
pub mod links;
pub mod note;
pub mod project;
pub mod scaffold;
pub mod structures;
pub mod templates;
pub mod text;
pub mod vault;
pub mod words;

pub use project::{Book, Chapter, Form, NoteKind, Project};
pub use vault::Vault;
