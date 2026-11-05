//! The engine behind `story-time`.
//!
//! A vault is a plain folder of Markdown notes; an Obsidian vault works as-is. Each story project
//! is a folder at the vault root holding a note of the same name, a `<name>-flow.md` story flow,
//! and `chapters/`, `characters/`, `locations/` and `world/` folders. Everything this crate knows
//! comes from those files and their YAML frontmatter, and everything it writes is ordinary
//! Markdown, so the vault stays readable and editable without it. Front ends (the CLI today, a
//! desktop app or web server later) sit on top of this crate.

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

pub use project::{Chapter, Form, NoteKind, Project};
pub use vault::Vault;
