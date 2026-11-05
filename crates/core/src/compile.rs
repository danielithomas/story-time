//! Stitching a book's chapters into one manuscript, and exporting it with pandoc.

use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::LazyLock;

use anyhow::{Context, Result, bail};
use regex::Regex;

use crate::project::Book;
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
    pub words: usize,
    /// Chapters left out because they have no words yet.
    pub skipped: Vec<String>,
}

/// The chapters in order, each under a level-one heading, with notes-to-self removed.
pub fn manuscript(book: &Book, author: Option<&str>) -> Result<Manuscript> {
    let mut markdown = format!("---\ntitle: \"{}\"\n", yaml_escape(&book.title()));
    if let Some(author) = author {
        markdown.push_str(&format!("author: \"{}\"\n", yaml_escape(author)));
    }
    markdown.push_str("---\n\n");
    let (mut chapters, mut words, mut skipped) = (0, 0, Vec::new());
    for chapter in book.chapters()? {
        if chapter.words == 0 {
            skipped.push(chapter.note.name.clone());
            continue;
        }
        let text = paragraphs(prose(&chapter.note.body).trim());
        if !text.starts_with("# ") {
            markdown.push_str(&format!("# {}\n\n", chapter.title()));
        }
        markdown.push_str(&text);
        markdown.push('\n');
        chapters += 1;
        words += chapter.words;
    }
    Ok(Manuscript {
        markdown,
        chapters,
        words,
        skipped,
    })
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

    #[test]
    fn separates_prose_lines_only() {
        let text = "First line.\nSecond line.\n\n\n\n- item one\n- item two\n> quoted\n> more\n*Italic opening.*\nNext.\n***\nAfter the break.";
        assert_eq!(
            paragraphs(text),
            "First line.\n\nSecond line.\n\n- item one\n- item two\n> quoted\n> more\n*Italic opening.*\n\nNext.\n\n***\n\nAfter the break.\n"
        );
    }

    #[test]
    fn formats_round_trip() {
        for format in Format::ALL {
            assert_eq!(Format::from_key(format.key()), Some(format));
        }
    }
}
