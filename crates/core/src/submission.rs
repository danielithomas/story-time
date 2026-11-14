//! Standard manuscript format, for sending work to editors and agents, after William Shunn's
//! guide: 12-point Times New Roman, double-spaced, one-inch margins, paragraphs indented half an
//! inch, and a running header with the author's surname, a keyword from the title and the page
//! number. The first page has the author's name and contact details top left, the word count top
//! right, and the title and byline. A novel or novella then starts on a new page, and each chapter
//! starts a new page a third of the way down; a shorter piece starts beneath the title, with a
//! centred `#` between its scenes and chapters.
//!
//! `compile --format docx` writes it through pandoc, with a reference document made here from
//! pandoc's own default: its styles, page and header are all that change.

use std::env;
use std::io::{Cursor, Read, Write};
use std::sync::LazyLock;

use anyhow::{Context, Result};
use regex::Regex;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::compile::{Manuscript, scene_text};
use crate::config::Config;
use crate::project::{Book, Form};
use crate::text::thousands;

/// The paper a manuscript is set for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paper {
    A4,
    Letter,
}

impl Paper {
    /// The bookshelf's `paper` setting, or else US Letter in the United States and Canada (going
    /// by its spelling languages, then the system's locale) and A4 elsewhere.
    pub fn of(config: &Config) -> Paper {
        match config.paper.as_deref().map(str::to_lowercase).as_deref() {
            Some("letter") => return Paper::Letter,
            Some("a4") => return Paper::A4,
            _ => {}
        }
        let american = |code: &str| {
            let code = code.replace('-', "_");
            code.contains("_US") || code.contains("_CA")
        };
        let languages = match &config.spelling {
            Some(languages) if !languages.is_empty() => languages.clone(),
            _ => ["LC_ALL", "LC_PAPER", "LANG"]
                .iter()
                .filter_map(|key| env::var(key).ok())
                .filter(|value| !value.is_empty())
                .take(1)
                .collect(),
        };
        if languages.iter().any(|code| american(code)) {
            Paper::Letter
        } else {
            Paper::A4
        }
    }

    /// Width and height in twentieths of a point.
    fn size(self) -> (u32, u32) {
        match self {
            Paper::A4 => (11906, 16838),
            Paper::Letter => (12240, 15840),
        }
    }
}

/// What the first page and the running header need.
#[derive(Debug, Clone)]
pub struct Details {
    pub author: Option<String>,
    pub contact: Vec<String>,
    pub paper: Paper,
}

impl Details {
    pub fn of(config: &Config) -> Details {
        Details {
            author: config.author.clone().filter(|a| !a.trim().is_empty()),
            contact: config.contact.clone(),
            paper: Paper::of(config),
        }
    }

    /// The surname in the running header: the last word of the author's name.
    fn surname(&self) -> Option<&str> {
        self.author.as_deref()?.split_whitespace().last()
    }
}

/// One inch, the margin all round, in twentieths of a point.
const INCH: u32 = 1440;

/// Whether a book is long enough for a title page of its own and chapters on new pages.
fn is_long(book: &Book) -> bool {
    matches!(book.form(), Some(Form::Novel | Form::Novella))
}

/// The word count on the first page: to the nearest thousand for a novel or novella, else the
/// nearest hundred, as editors expect.
fn about(words: usize, long: bool) -> String {
    let step = if long { 1000 } else { 100 };
    let rounded = ((words + step / 2) / step * step).max(step);
    format!("about {} words", thousands(rounded as i64))
}

/// The keyword from the title in the running header: a short title whole, or else its longest
/// word, leaving out a leading article.
fn keyword(title: &str) -> String {
    let mut words: Vec<&str> = title.split_whitespace().collect();
    if words.len() > 1 && ["the", "a", "an"].contains(&words[0].to_lowercase().as_str()) {
        words.remove(0);
    }
    let keyword = if words.len() <= 2 {
        words.join(" ")
    } else {
        let mut longest = words[0];
        for word in &words[1..] {
            if word.chars().count() > longest.chars().count() {
                longest = word;
            }
        }
        longest.to_string()
    };
    keyword
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_uppercase()
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Raw Word paragraphs in a pandoc Markdown document.
fn raw(paragraphs: &str) -> String {
    format!("```{{=openxml}}\n{paragraphs}\n```\n\n")
}

fn paragraph(style: &str, text: &str) -> String {
    format!(
        "<w:p><w:pPr><w:pStyle w:val=\"{style}\"/></w:pPr><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        xml(text)
    )
}

const SCENE_BREAK: &str =
    "<w:p><w:pPr><w:pStyle w:val=\"SceneBreak\"/></w:pPr><w:r><w:t>#</w:t></w:r></w:p>";

/// The book as pandoc Markdown in standard manuscript format. Unplaced scenes, and scenes with no
/// words yet, are left out, as in any manuscript.
pub fn markdown(book: &Book, details: &Details) -> Result<Manuscript> {
    let contents = book.contents()?;
    let long = is_long(book);
    let (mut chapters, mut scenes, mut words, mut skipped) = (0, 0, 0, Vec::new());
    let mut body = String::new();
    let chapter_heading = if contents.has_parts() { "##" } else { "#" };
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
            if long {
                text.push_str(&format!("{chapter_heading} {}\n\n", chapter.heading()));
            } else if !body.is_empty() || !text.is_empty() {
                text.push_str(&raw(SCENE_BREAK));
            }
            text.push_str(&included.join(&format!("\n{}", raw(SCENE_BREAK))));
            text.push('\n');
            chapters += 1;
            scenes += included.len();
        }
        if text.is_empty() {
            continue;
        }
        if long && let Some(title) = &part.title {
            body.push_str(&format!("# {title}\n\n"));
        }
        body.push_str(&text);
    }

    let (width, _) = details.paper.size();
    let edge = width - 2 * INCH;
    let name = details.author.as_deref().unwrap_or_default();
    let mut first = format!(
        "<w:p><w:pPr><w:pStyle w:val=\"Contact\"/><w:tabs><w:tab w:val=\"right\" w:pos=\"{edge}\"/></w:tabs></w:pPr>\
         <w:r><w:t xml:space=\"preserve\">{}</w:t></w:r><w:r><w:tab/></w:r><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        xml(name),
        about(words, long)
    );
    for line in &details.contact {
        first.push_str(&paragraph("Contact", line));
    }
    first.push_str(&paragraph("Title", &book.title()));
    if let Some(author) = &details.author {
        first.push_str(&paragraph("Author", &format!("by {author}")));
    }
    let markdown = format!("{}{}", raw(&first), body);
    Ok(Manuscript {
        markdown: format!("{}\n", markdown.trim_end()),
        chapters,
        scenes,
        words,
        skipped,
    })
}

static STYLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<w:style\b[^>]*\bw:styleId="([^"]+)"[^>]*>.*?</w:style>"#).unwrap()
});
static DEFAULTS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<w:docDefaults>.*?</w:docDefaults>").unwrap());
static SECTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<w:sectPr\b[^>]*>.*?</w:sectPr>|<w:sectPr\b[^>]*/>").unwrap()
});

const FONT: &str = r#"<w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:eastAsia="Times New Roman" w:cs="Times New Roman"/>"#;

/// Double spacing, and nothing between paragraphs.
const DOUBLE: &str = r#"<w:spacing w:before="0" w:after="0" w:line="480" w:lineRule="auto"/>"#;
const SINGLE: &str = r#"<w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/>"#;

fn style(id: &str, name: &str, paragraph: &str, run: &str) -> String {
    format!(
        r#"<w:style w:type="paragraph" w:customStyle="1" w:styleId="{id}"><w:name w:val="{name}"/><w:basedOn w:val="Normal"/><w:qFormat/><w:pPr>{paragraph}</w:pPr><w:rPr>{FONT}<w:b w:val="0"/><w:i w:val="0"/><w:color w:val="000000"/><w:sz w:val="24"/><w:szCs w:val="24"/>{run}</w:rPr></w:style>"#
    )
}

/// The paragraph styles that make the format, by style ID.
fn styles() -> Vec<(&'static str, String)> {
    let indented = format!(r#"{DOUBLE}<w:ind w:firstLine="720"/>"#);
    let centred = format!(r#"{DOUBLE}<w:ind w:firstLine="0"/><w:jc w:val="center"/>"#);
    // A third of the way down the page, less the top margin.
    let chapter = r#"<w:keepNext/><w:pageBreakBefore/><w:spacing w:before="3840" w:after="480" w:line="480" w:lineRule="auto"/><w:ind w:firstLine="0"/><w:jc w:val="center"/><w:outlineLvl w:val="0"/>"#;
    vec![
        ("Normal", style("Normal", "Normal", DOUBLE, "")),
        ("BodyText", style("BodyText", "Body Text", &indented, "")),
        (
            "FirstParagraph",
            style("FirstParagraph", "First Paragraph", &indented, ""),
        ),
        ("Heading1", style("Heading1", "heading 1", chapter, "")),
        ("Heading2", style("Heading2", "heading 2", chapter, "")),
        ("Heading3", style("Heading3", "heading 3", &centred, "")),
        // The title sits about halfway down the first page.
        (
            "Title",
            style(
                "Title",
                "Title",
                r#"<w:spacing w:before="4320" w:after="0" w:line="480" w:lineRule="auto"/><w:ind w:firstLine="0"/><w:jc w:val="center"/>"#,
                "",
            ),
        ),
        ("Author", style("Author", "Author", &centred, "")),
        (
            "Contact",
            style(
                "Contact",
                "Contact",
                &format!(r#"{SINGLE}<w:ind w:firstLine="0"/>"#),
                "",
            ),
        ),
        (
            "SceneBreak",
            style("SceneBreak", "Scene Break", &centred, ""),
        ),
        (
            "Header",
            style(
                "Header",
                "header",
                &format!(r#"{SINGLE}<w:ind w:firstLine="0"/><w:jc w:val="right"/>"#),
                "",
            ),
        ),
    ]
}

const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;
const HEADER_ID: &str = "rIdManuscriptHeader";

/// The running header: "Surname / KEYWORD / page", on every page but the first.
fn header(details: &Details, title: &str) -> String {
    let mut text = String::new();
    if let Some(surname) = details.surname() {
        text.push_str(&format!("{} / ", xml(surname)));
    }
    text.push_str(&format!("{} / ", xml(&keyword(title))));
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:hdr {W}><w:p><w:pPr><w:pStyle w:val="Header"/></w:pPr><w:r><w:t xml:space="preserve">{text}</w:t></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>1</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p></w:hdr>"#
    )
}

/// A reference document for pandoc, made from its default `reference` (pandoc's own,
/// `pandoc --print-default-data-file reference.docx`): the format's styles, the page size and
/// margins, and the running header, on every page but the first.
pub fn reference(default: &[u8], details: &Details, title: &str) -> Result<Vec<u8>> {
    let mut archive =
        ZipArchive::new(Cursor::new(default)).context("reading pandoc's reference document")?;
    let mut out = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let name = file.name()?.into_owned();
        let patch: Option<fn(&str, &Details) -> String> = match name.as_str() {
            "word/styles.xml" => Some(|text, _| with_styles(text)),
            "word/document.xml" => Some(with_section),
            "word/_rels/document.xml.rels" => Some(|text, _| {
                text.replace(
                    "</Relationships>",
                    &format!(r#"<Relationship Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Id="{HEADER_ID}" Target="header1.xml"/></Relationships>"#),
                )
            }),
            "[Content_Types].xml" => Some(|text, _| {
                text.replace(
                    "</Types>",
                    r#"<Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/></Types>"#,
                )
            }),
            _ => None,
        };
        match patch {
            Some(patch) => {
                let mut text = String::new();
                file.read_to_string(&mut text)?;
                out.start_file(name, options)?;
                out.write_all(patch(&text, details).as_bytes())?;
            }
            None => out.raw_copy_file(file)?,
        }
    }
    out.start_file("word/header1.xml", options)?;
    out.write_all(header(details, title).as_bytes())?;
    Ok(out.finish()?.into_inner())
}

/// pandoc's styles with the format's: Times New Roman 12-point, double-spaced, for everything.
fn with_styles(text: &str) -> String {
    let defaults = format!(
        r#"<w:docDefaults><w:rPrDefault><w:rPr>{FONT}<w:sz w:val="24"/><w:szCs w:val="24"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr>{DOUBLE}<w:widowControl/></w:pPr></w:pPrDefault></w:docDefaults>"#
    );
    let mut text = DEFAULTS.replace(text, defaults.as_str()).into_owned();
    for (id, definition) in styles() {
        let existing = STYLE
            .captures_iter(&text)
            .find(|style| &style[1] == id)
            .map(|style| style.get(0).expect("a match").range());
        match existing {
            Some(range) => text.replace_range(range, &definition),
            None => text = text.replace("</w:styles>", &format!("{definition}</w:styles>")),
        }
    }
    text
}

/// pandoc's document with the format's page: its size, one-inch margins, and the running header,
/// left off the first page.
fn with_section(text: &str, details: &Details) -> String {
    let (width, height) = details.paper.size();
    let section = format!(
        r#"<w:sectPr><w:headerReference w:type="default" r:id="{HEADER_ID}"/><w:pgSz w:w="{width}" w:h="{height}"/><w:pgMar w:top="{INCH}" w:right="{INCH}" w:bottom="{INCH}" w:left="{INCH}" w:header="720" w:footer="720" w:gutter="0"/><w:titlePg/></w:sectPr>"#
    );
    let text = if SECTION.is_match(text) {
        SECTION.replace(text, section.as_str()).into_owned()
    } else {
        text.replace("</w:body>", &format!("{section}</w:body>"))
    };
    if text.contains("xmlns:r=") {
        text
    } else {
        text.replacen(
            "<w:document ",
            r#"<w:document xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" "#,
            1,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_the_word_count_as_editors_expect() {
        assert_eq!(about(3249, false), "about 3,200 words");
        assert_eq!(about(3250, false), "about 3,300 words");
        assert_eq!(about(81_499, true), "about 81,000 words");
        assert_eq!(about(12, false), "about 100 words", "never none");
    }

    #[test]
    fn picks_a_keyword_from_the_title() {
        assert_eq!(keyword("The Deep"), "DEEP");
        assert_eq!(keyword("Sample Story"), "SAMPLE STORY");
        assert_eq!(keyword("A Tour of Story Time"), "STORY");
        assert_eq!(keyword("Leaving: Home?"), "LEAVING: HOME");
    }

    #[test]
    fn chooses_paper_by_setting_then_language() {
        let config = |paper: Option<&str>, spelling: Option<&[&str]>| Config {
            paper: paper.map(String::from),
            spelling: spelling.map(|s| s.iter().map(|c| c.to_string()).collect()),
            ..Config::default()
        };
        assert_eq!(
            Paper::of(&config(Some("Letter"), Some(&["en_AU"]))),
            Paper::Letter
        );
        assert_eq!(Paper::of(&config(Some("a4"), Some(&["en_US"]))), Paper::A4);
        assert_eq!(Paper::of(&config(None, Some(&["en_US"]))), Paper::Letter);
        assert_eq!(Paper::of(&config(None, Some(&["en-CA"]))), Paper::Letter);
        assert_eq!(
            Paper::of(&config(None, Some(&["en_AU", "fr_FR"]))),
            Paper::A4
        );
    }

    use crate::contents::Place;
    use crate::scaffold::{NewBook, NewProject, add_scene, create_project};
    use crate::structures::THREE_ACT;
    use crate::vault::Vault;
    use chrono::NaiveDate;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    /// A book of `form` with two chapters of a scene each, and a third scene in the first.
    fn book(form: Form) -> (TempDir, Vault, Book) {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join(".obsidian")).unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 11, 15).unwrap();
        let new = NewProject {
            world: None,
            book: NewBook {
                title: "The Long Night",
                form,
                target: None,
                structure: &THREE_ACT,
                premise: None,
            },
        };
        let (project, book) = create_project(&vault, &new, today).unwrap();
        for (name, place, text) in [
            ("Arrival", Place::End, "Mara came ashore & waited."),
            ("Departure", Place::End, "She left at dawn."),
            ("Coda", Place::Chapter(0), "The tide came in."),
        ] {
            let current = Book::load(&book.dir, &project.slug).unwrap().unwrap();
            let path = add_scene(&vault, &project, &current, name, place, today)
                .unwrap()
                .path;
            fs::write(
                &path,
                format!("---\ntype: scene\naliases: [{name}]\n---\n# {name}\n\n{text}\n"),
            )
            .unwrap();
        }
        let book = Book::load(&book.dir, &project.slug).unwrap().unwrap();
        (dir, vault, book)
    }

    fn details() -> Details {
        Details {
            author: Some("Dana R. Writer".into()),
            contact: vec!["12 Example Street".into(), "dana@example.com".into()],
            paper: Paper::A4,
        }
    }

    #[test]
    fn a_short_piece_starts_beneath_its_title() {
        let (_dir, _vault, book) = book(Form::ShortStory);
        let built = markdown(&book, &details()).unwrap();
        let text = &built.markdown;
        assert!(text.starts_with("```{=openxml}\n"), "{text}");
        assert!(text.contains(">Dana R. Writer</w:t></w:r><w:r><w:tab/></w:r><w:r><w:t xml:space=\"preserve\">about 100 words<"));
        assert!(text.contains(">12 Example Street<") && text.contains(">dana@example.com<"));
        assert!(
            text.contains(
                "w:val=\"Title\"/></w:pPr><w:r><w:t xml:space=\"preserve\">The Long Night<"
            )
        );
        assert!(text.contains(">by Dana R. Writer<"));
        assert!(
            text.contains("Mara came ashore & waited."),
            "prose stays Markdown"
        );
        assert!(
            !text.contains("\n# "),
            "no chapter headings in a short piece: {text}"
        );
        assert_eq!(
            text.matches(SCENE_BREAK).count(),
            2,
            "between scenes and chapters"
        );
        assert_eq!((built.chapters, built.scenes, built.words), (2, 3, 12));
    }

    #[test]
    fn a_novel_has_a_title_page_and_chapter_headings() {
        let (_dir, _vault, book) = book(Form::Novel);
        let text = markdown(&book, &details()).unwrap().markdown;
        assert!(text.contains(">about 1,000 words<"));
        assert!(
            text.contains("\n# Chapter 1\n") && text.contains("\n# Chapter 2\n"),
            "{text}"
        );
        assert_eq!(
            text.matches(SCENE_BREAK).count(),
            1,
            "between the scenes of a chapter"
        );
    }

    /// The parts of a reference document pandoc's default has that matter here.
    fn default_reference() -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default();
        for (name, text) in [
            ("[Content_Types].xml", "<Types></Types>"),
            (
                "word/_rels/document.xml.rels",
                "<Relationships></Relationships>",
            ),
            (
                "word/document.xml",
                "<w:document xmlns:w=\"w\"><w:body><w:p/><w:sectPr><w:footnotePr/></w:sectPr></w:body></w:document>",
            ),
            (
                "word/styles.xml",
                "<w:styles><w:docDefaults><w:rPrDefault/></w:docDefaults><w:style w:type=\"paragraph\" w:styleId=\"BodyText\"><w:name w:val=\"Body Text\"/></w:style><w:style w:type=\"character\" w:styleId=\"Hyperlink\"><w:name w:val=\"Hyperlink\"/></w:style></w:styles>",
            ),
            ("word/theme/theme1.xml", "<theme/>"),
        ] {
            zip.start_file(name, options).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    fn part(docx: &[u8], name: &str) -> String {
        let mut archive = ZipArchive::new(Cursor::new(docx)).unwrap();
        let mut text = String::new();
        archive
            .by_name(name)
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        text
    }

    #[test]
    fn makes_a_reference_document_from_pandocs() {
        let docx = reference(&default_reference(), &details(), "A Tour of Story Time").unwrap();
        let header = part(&docx, "word/header1.xml");
        assert!(
            header.contains(">Writer / STORY / </w:t>") && header.contains(" PAGE "),
            "{header}"
        );
        let document = part(&docx, "word/document.xml");
        assert!(
            document.contains("<w:pgSz w:w=\"11906\" w:h=\"16838\"/>"),
            "{document}"
        );
        assert!(
            document.contains("<w:titlePg/>") && document.contains("r:id=\"rIdManuscriptHeader\"")
        );
        assert!(
            document.contains("xmlns:r="),
            "the relationships namespace is declared"
        );
        assert!(part(&docx, "word/_rels/document.xml.rels").contains("Target=\"header1.xml\""));
        assert!(part(&docx, "[Content_Types].xml").contains("/word/header1.xml"));
        let styles = part(&docx, "word/styles.xml");
        assert_eq!(
            styles.matches("w:styleId=\"BodyText\"").count(),
            1,
            "replaced, not added again"
        );
        assert!(
            styles.contains("w:styleId=\"Hyperlink\""),
            "others are kept"
        );
        assert!(styles.contains("w:styleId=\"SceneBreak\"") && styles.contains("Times New Roman"));
        assert_eq!(part(&docx, "word/theme/theme1.xml"), "<theme/>");
    }

    #[test]
    fn exports_a_word_manuscript_through_pandoc() {
        if std::process::Command::new("pandoc")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("pandoc isn't installed, so the Word export isn't tried");
            return;
        }
        let (dir, vault, book) = book(Form::Novel);
        let mut config = vault.config.clone();
        config.author = Some("Dana Writer".into());
        config.paper = Some("letter".into());
        let out = dir.path().join("out").join("the-long-night.docx");
        let built =
            crate::compile::write(&book, &config, crate::compile::Format::Docx, &out).unwrap();
        assert_eq!(built.scenes, 3);
        let files: Vec<_> = fs::read_dir(out.parent().unwrap()).unwrap().collect();
        assert_eq!(files.len(), 1, "the reference document is cleaned up");
        let docx = fs::read(&out).unwrap();
        assert!(part(&docx, "word/header1.xml").contains("Writer / LONG NIGHT / "));
        let document = part(&docx, "word/document.xml");
        assert!(document.contains("w:w=\"12240\""), "letter paper");
        assert!(document.contains("Mara came ashore &amp; waited."));
        assert!(Path::new(&out).exists());
    }
}
