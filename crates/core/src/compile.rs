//! Stitching a book's scenes into one manuscript, and exporting it with pandoc.

use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::LazyLock;

use anyhow::{Context, Result, bail};
use regex::Regex;

use crate::project::{Book, Scene};
use crate::text::yaml_escape;
use crate::words::prose;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Markdown,
    Docx,
    Epub,
    Html,
    Pdf,
}

impl Format {
    pub const ALL: [Format; 5] = [
        Format::Markdown,
        Format::Docx,
        Format::Epub,
        Format::Html,
        Format::Pdf,
    ];

    /// Also the file extension.
    pub fn key(self) -> &'static str {
        match self {
            Format::Markdown => "md",
            Format::Docx => "docx",
            Format::Epub => "epub",
            Format::Html => "html",
            Format::Pdf => "pdf",
        }
    }

    pub fn from_key(key: &str) -> Option<Format> {
        Format::ALL
            .into_iter()
            .find(|format| format.key().eq_ignore_ascii_case(key))
    }

    pub fn describe(self) -> &'static str {
        match self {
            Format::Markdown => "one Markdown file (no extra software needed)",
            Format::Docx => "Word document, via pandoc",
            Format::Epub => "e-book, via pandoc",
            Format::Html => "web page, via pandoc",
            Format::Pdf => "PDF, via pandoc and a LaTeX engine",
        }
    }
}

pub struct Manuscript {
    pub markdown: String,
    pub chapters: usize,
    pub scenes: usize,
    pub words: usize,
    /// Placed scenes left out because they have no words yet.
    pub skipped: Vec<String>,
}

/// The scene break between the scenes of a chapter.
const SCENE_BREAK: &str = "***";

/// The placed scenes in the order the contents list them, with notes-to-self removed: a heading
/// for each part and chapter, and a scene break between the scenes of a chapter. Unplaced scenes,
/// and scenes with no words yet, are left out.
pub fn manuscript(book: &Book, author: Option<&str>) -> Result<Manuscript> {
    let contents = book.contents()?;
    let mut markdown = format!("---\ntitle: \"{}\"\n", yaml_escape(&book.title()));
    if let Some(author) = author {
        markdown.push_str(&format!("author: \"{}\"\n", yaml_escape(author)));
    }
    markdown.push_str("---\n\n");
    let chapter_heading = if contents.has_parts() { "##" } else { "#" };
    let (mut chapters, mut scenes, mut words, mut skipped) = (0, 0, 0, Vec::new());
    for part in &contents.parts {
        let mut text = String::new();
        for chapter in &part.chapters {
            let mut included = Vec::new();
            for scene in &chapter.scenes {
                if scene.words == 0 {
                    skipped.push(scene.note.name.clone());
                } else {
                    included.push(scene_text(scene));
                    words += scene.words;
                }
            }
            if included.is_empty() {
                continue;
            }
            text.push_str(&format!("{chapter_heading} {}\n\n", chapter.heading()));
            text.push_str(&included.join(&format!("\n{SCENE_BREAK}\n\n")));
            text.push('\n');
            chapters += 1;
            scenes += included.len();
        }
        if text.is_empty() {
            continue;
        }
        if let Some(title) = &part.title {
            markdown.push_str(&format!("# {title}\n\n"));
        }
        markdown.push_str(&text);
    }
    Ok(Manuscript {
        markdown: format!("{}\n", markdown.trim_end()),
        chapters,
        scenes,
        words,
        skipped,
    })
}

/// A scene's prose, without the title heading its note opens with: the contents give the
/// manuscript its headings.
fn scene_text(scene: &Scene) -> String {
    let text = prose(&scene.note.body);
    let mut lines = text.trim().lines().peekable();
    if lines.peek().is_some_and(|line| line.starts_with("# ")) {
        lines.next();
    }
    paragraphs(lines.collect::<Vec<_>>().join("\n").trim())
}

static STRUCTURAL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:#{1,6}\s|>|\||(?:[-*+]|\d+[.)])\s)").unwrap());

/// Obsidian shows every line break, but pandoc joins adjacent lines into one paragraph. In
/// fiction a line break almost always means a new paragraph, so consecutive prose lines are
/// separated with a blank line; lists, quotes, tables and headings are left alone. Runs of blank
/// lines (often left where comments were removed) collapse to one.
fn paragraphs(text: &str) -> String {
    let is_prose = |line: &str| {
        !line.trim().is_empty() && !line.starts_with([' ', '\t']) && !STRUCTURAL.is_match(line)
    };
    let lines: Vec<&str> = text.lines().collect();
    let mut out = String::with_capacity(text.len() + lines.len());
    for (i, line) in lines.iter().enumerate() {
        if line.trim().is_empty() && (out.is_empty() || out.ends_with("\n\n")) {
            continue;
        }
        out.push_str(line);
        out.push('\n');
        if lines
            .get(i + 1)
            .is_some_and(|next| is_prose(line) && is_prose(next))
        {
            out.push('\n');
        }
    }
    out
}

/// Writes the manuscript to `out`: directly for Markdown, otherwise through pandoc.
pub fn export(markdown: &str, out: &Path, format: Format) -> Result<()> {
    if let Some(parent) = out.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    if format == Format::Markdown {
        return fs::write(out, markdown).with_context(|| format!("writing {}", out.display()));
    }
    let mut pandoc = match Command::new("pandoc")
        .args(["--from", "markdown", "--standalone", "--output"])
        .arg(out)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            bail!(
                "{} output needs pandoc (https://pandoc.org); install it or use --format md",
                format.key()
            )
        }
        Err(e) => return Err(e).context("starting pandoc"),
    };
    pandoc
        .stdin
        .take()
        .context("pandoc stdin")?
        .write_all(markdown.as_bytes())
        .context("sending the manuscript to pandoc")?;
    let result = pandoc.wait_with_output().context("waiting for pandoc")?;
    if !result.status.success() {
        bail!(
            "pandoc failed: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vault;
    use crate::contents::Place;
    use crate::note::set_section;
    use crate::project::Form;
    use crate::scaffold::{NewBook, NewProject, add_scene, create_project};
    use crate::structures::FREEFORM;
    use chrono::NaiveDate;
    use tempfile::TempDir;

    #[test]
    fn separates_prose_lines_only() {
        let text = "First line.\nSecond line.\n\n\n\n- item one\n- item two\n> quoted\n> more\n*Italic opening.*\nNext.\n***\nAfter the break.";
        assert_eq!(
            paragraphs(text),
            "First line.\n\nSecond line.\n\n- item one\n- item two\n> quoted\n> more\n*Italic opening.*\n\nNext.\n\n***\n\nAfter the break.\n"
        );
    }

    #[test]
    fn follows_the_contents() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join(".obsidian")).unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 11, 6).unwrap();
        let spec = NewProject {
            world: None,
            book: NewBook {
                title: "Sample Story",
                form: Form::ShortStory,
                target: None,
                structure: &FREEFORM,
                premise: None,
            },
        };
        let (project, book) = create_project(&vault, &spec, today).unwrap();
        for (name, place) in [
            ("Arrival", Place::End),
            ("First Light", Place::Chapter(0)),
            ("Empty", Place::Chapter(0)),
            ("Night", Place::End),
            ("Aside", Place::Unplaced),
        ] {
            let book = Book::load(&book.dir, &project.slug).unwrap().unwrap();
            add_scene(&vault, &project, &book, name, place, today).unwrap();
        }
        for (name, text) in [
            ("arrival", "# Arrival\n\nShe landed.\nIt rained."),
            ("first-light", "Dawn came. %% too short? %%"),
            ("night", "Dark."),
            ("aside", "Not in the book."),
        ] {
            fs::write(
                book.scenes_dir().join(format!("{name}.md")),
                format!("---\nstatus: draft\n---\n{text}\n"),
            )
            .unwrap();
        }
        let book = Book::load(&book.dir, &project.slug).unwrap().unwrap();
        let built = manuscript(&book, Some("A. Writer")).unwrap();
        assert_eq!(
            built.markdown,
            "---\ntitle: \"Sample Story\"\nauthor: \"A. Writer\"\n---\n\n# Chapter 1\n\nShe landed.\n\nIt rained.\n\n***\n\nDawn came.\n\n# Chapter 2\n\nDark.\n"
        );
        assert_eq!((built.chapters, built.scenes, built.words), (2, 3, 7));
        assert_eq!(built.skipped, ["empty"]);

        let content = fs::read_to_string(&book.note.path).unwrap();
        let parts = "### Part One\n\n- The Landing\n  - [[arrival|Arrival]]\n- [[first-light]]\n\n### Part Two\n\n- [[night]]";
        let lines: Vec<String> = parts.lines().map(String::from).collect();
        fs::write(&book.note.path, set_section(&content, "Contents", &lines)).unwrap();
        let book = Book::load(&book.dir, &project.slug).unwrap().unwrap();
        assert_eq!(
            manuscript(&book, None).unwrap().markdown,
            "---\ntitle: \"Sample Story\"\n---\n\n# Part One\n\n## The Landing\n\nShe landed.\n\nIt rained.\n\n## Chapter 1\n\nDawn came.\n\n# Part Two\n\n## Chapter 2\n\nDark.\n"
        );
    }

    #[test]
    fn formats_round_trip() {
        for format in Format::ALL {
            assert_eq!(Format::from_key(format.key()), Some(format));
        }
    }
}
