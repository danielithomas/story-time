//! A Markdown note and its YAML frontmatter (Obsidian "properties").

use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::NaiveDate;
use yaml_rust2::{Yaml, YamlLoader};

use crate::text::name_from_file;
use crate::words;

/// Splits a note into its frontmatter (without the `---` fences) and its body.
pub fn split_frontmatter(content: &str) -> (Option<&str>, &str) {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let mut lines = content.split_inclusive('\n');
    match lines.next() {
        Some(first) if first.trim_end() == "---" => {
            let start = first.len();
            let mut end = start;
            for line in lines {
                let fence = line.trim_end();
                if fence == "---" || fence == "..." {
                    return (Some(&content[start..end]), &content[end + line.len()..]);
                }
                end += line.len();
            }
            (None, content)
        }
        _ => (None, content),
    }
}

/// Replaces a note's body, keeping its frontmatter exactly as written.
pub fn replace_body(content: &str, body: &str) -> String {
    match split_frontmatter(content) {
        (Some(yaml), _) => format!("---\n{yaml}---\n{body}"),
        (None, _) => body.to_string(),
    }
}

/// The lines under a `## heading` in a note's body, up to the next heading at the same or a
/// higher level, or `None` if the body has no such heading.
pub fn section<'a>(body: &'a str, heading: &str) -> Option<Vec<&'a str>> {
    let lines: Vec<&str> = body.lines().collect();
    let (start, end) = find_section(&lines, heading)?;
    Some(lines[start + 1..end].to_vec())
}

/// A note's text with the lines under `## heading` replaced by `lines`, or with the section
/// added at the end if the body has none. The section keeps one blank line after its heading and
/// before the next. The frontmatter and the rest of the body stay as written.
pub fn set_section(content: &str, heading: &str, lines: &[String]) -> String {
    let (_, body) = split_frontmatter(content);
    let mut out: Vec<String> = body.lines().map(String::from).collect();
    let mut replacement = vec![String::new()];
    replacement.extend(
        lines
            .iter()
            .skip_while(|line| line.trim().is_empty())
            .cloned(),
    );
    while replacement
        .last()
        .is_some_and(|line| line.trim().is_empty())
    {
        replacement.pop();
    }
    let borrowed: Vec<&str> = out.iter().map(String::as_str).collect();
    match find_section(&borrowed, heading) {
        Some((start, end)) => {
            if end < out.len() {
                replacement.push(String::new());
            }
            out.splice(start + 1..end, replacement);
        }
        None => {
            while out.last().is_some_and(|line| line.trim().is_empty()) {
                out.pop();
            }
            if !out.is_empty() {
                out.push(String::new());
            }
            out.push(format!("## {heading}"));
            out.extend(replacement);
        }
    }
    replace_body(content, &(out.join("\n") + "\n"))
}

/// The index of the `## heading` line and of the line that ends its section.
fn find_section(lines: &[&str], heading: &str) -> Option<(usize, usize)> {
    let start = lines.iter().position(|line| {
        heading_level(line) == Some(2)
            && line.trim_start()[2..].trim().eq_ignore_ascii_case(heading)
    })?;
    let end = (start + 1..lines.len())
        .find(|&i| heading_level(lines[i]).is_some_and(|level| level <= 2))
        .unwrap_or(lines.len());
    Some((start, end))
}

/// The level of a Markdown heading line (`## Books` is 2), or `None` for other lines.
fn heading_level(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let level = trimmed.chars().take_while(|c| *c == '#').count();
    let rest = &trimmed[level..];
    ((1..=6).contains(&level) && (rest.is_empty() || rest.starts_with(' '))).then_some(level)
}

/// Identifies a note's content, to tell whether the file changed since it was read. Hashes are
/// comparable within one run of a program, not across versions of it.
pub fn content_hash(content: &str) -> String {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Saved {
    Written {
        hash: String,
        words: usize,
    },
    /// The file changed since it was read, so nothing was written. `hash` is its content now.
    Conflict {
        hash: String,
    },
}

/// Saves edits to a note: a new body, property values, or both. The file is left alone if its
/// content no longer matches `base_hash` (it was changed elsewhere, in Obsidian or by a sync
/// service), unless `force` is set. Writes go through a temporary file, so a crash can't leave
/// half a note behind.
pub fn save(
    path: &Path,
    body: Option<&str>,
    properties: &[(&str, PropertyValue)],
    base_hash: &str,
    force: bool,
) -> Result<Saved> {
    let current =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let current_hash = content_hash(&current);
    if !force && current_hash != base_hash {
        return Ok(Saved::Conflict { hash: current_hash });
    }
    let mut content = if properties.is_empty() {
        current.clone()
    } else {
        set_properties(&current, properties)
    };
    if let Some(body) = body {
        content = replace_body(&content, body);
    }
    if content != current {
        write_atomically(path, &content)?;
    }
    Ok(Saved::Written {
        hash: content_hash(&content),
        words: Note::parse(path, &content).words(),
    })
}

/// Replaces the lines under `## heading` in a note, as [`set_section`] does. Like [`save`], it
/// leaves the file alone if its content no longer matches `base_hash`.
pub fn save_section(
    path: &Path,
    heading: &str,
    lines: &[String],
    base_hash: &str,
) -> Result<Saved> {
    let current =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let current_hash = content_hash(&current);
    if current_hash != base_hash {
        return Ok(Saved::Conflict { hash: current_hash });
    }
    let content = set_section(&current, heading, lines);
    if content != current {
        write_atomically(path, &content)?;
    }
    Ok(Saved::Written {
        hash: content_hash(&content),
        words: Note::parse(path, &content).words(),
    })
}

fn write_atomically(path: &Path, content: &str) -> Result<()> {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temp = path.with_file_name(format!(".{name}.story-time-tmp"));
    fs::write(&temp, content).with_context(|| format!("writing {}", temp.display()))?;
    fs::rename(&temp, path).with_context(|| format!("replacing {}", path.display()))
}

/// A property value as the app reads and writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertyValue {
    Text(String),
    List(Vec<String>),
}

/// Sets properties in a note's text. Only the lines of the properties being set change, so
/// comments, ordering and formatting elsewhere survive. New properties go at the end of the
/// frontmatter; a note without frontmatter gets some. An empty text value leaves the key blank.
pub fn set_properties(content: &str, updates: &[(&str, PropertyValue)]) -> String {
    let (yaml, body) = split_frontmatter(content);
    let mut lines: Vec<String> = yaml
        .map(|yaml| yaml.lines().map(String::from).collect())
        .unwrap_or_default();
    for (key, value) in updates {
        let rendered = render_property(key, value);
        match lines
            .iter()
            .position(|line| top_level_key(line) == Some(*key))
        {
            Some(start) => {
                let end = (start + 1..lines.len())
                    .find(|&i| !continues_value(&lines[i]))
                    .unwrap_or(lines.len());
                lines.splice(start..end, rendered);
            }
            None => lines.extend(rendered),
        }
    }
    let yaml: String = lines.iter().map(|line| format!("{line}\n")).collect();
    format!("---\n{yaml}---\n{body}")
}

/// Removes properties from a note's text, leaving every other line as written.
pub fn remove_properties(content: &str, keys: &[&str]) -> String {
    let (Some(yaml), body) = split_frontmatter(content) else {
        return content.to_string();
    };
    let mut kept = String::new();
    let mut removing = false;
    for line in yaml.lines() {
        match top_level_key(line) {
            Some(key) => removing = keys.contains(&key),
            None if !continues_value(line) => removing = false,
            None => {}
        }
        if !removing {
            kept.push_str(line);
            kept.push('\n');
        }
    }
    format!("---\n{kept}---\n{body}")
}

/// The key a frontmatter line starts, if it starts one at the top level.
fn top_level_key(line: &str) -> Option<&str> {
    if line.starts_with([' ', '\t', '-', '#']) {
        return None;
    }
    let key = line.split_once(':')?.0.trim();
    Some(key.trim_matches(|c| c == '"' || c == '\''))
}

/// Whether a line belongs to the value of the key above it: indented lines and block-list items.
fn continues_value(line: &str) -> bool {
    line.starts_with([' ', '\t']) || line.starts_with("- ") || line == "-"
}

fn render_property(key: &str, value: &PropertyValue) -> Vec<String> {
    match value {
        PropertyValue::Text(text) if text.trim().is_empty() => vec![format!("{key}:")],
        PropertyValue::Text(text) if text.trim().contains('\n') => {
            let mut lines = vec![format!("{key}: |-")];
            lines.extend(
                text.trim()
                    .lines()
                    .map(|line| format!("  {line}").trim_end().to_string()),
            );
            lines
        }
        PropertyValue::Text(text) => vec![format!("{key}: {}", yaml_scalar(text.trim()))],
        PropertyValue::List(items) => {
            let items: Vec<&str> = items
                .iter()
                .map(|item| item.trim())
                .filter(|item| !item.is_empty())
                .collect();
            if items.is_empty() {
                return vec![format!("{key}: []")];
            }
            let mut lines = vec![format!("{key}:")];
            lines.extend(
                items
                    .iter()
                    .map(|item| format!("  - {}", yaml_scalar(item))),
            );
            lines
        }
    }
}

/// A value as YAML, quoted only when it would otherwise be misread.
fn yaml_scalar(value: &str) -> String {
    let reserved = ["true", "false", "yes", "no", "on", "off", "null", "~"];
    let needs_quotes = value
        .starts_with(|c: char| "-?:,[]{}#&*!|>'\"%@`".contains(c) || c.is_whitespace())
        || value.contains(": ")
        || value.contains(" #")
        || value.ends_with(':')
        || reserved.contains(&value.to_lowercase().as_str());
    if needs_quotes {
        format!("\"{}\"", crate::text::yaml_escape(value))
    } else {
        value.to_string()
    }
}

/// Read-only access to frontmatter values, forgiving about how they were typed.
#[derive(Debug, Clone, Default)]
pub struct Frontmatter {
    doc: Option<Yaml>,
}

impl Frontmatter {
    pub fn parse(yaml: &str) -> Result<Frontmatter, String> {
        let docs = YamlLoader::load_from_str(yaml).map_err(|e| e.to_string())?;
        let doc = docs
            .into_iter()
            .next()
            .filter(|doc| matches!(doc, Yaml::Hash(_)));
        Ok(Frontmatter { doc })
    }

    fn get(&self, key: &str) -> Option<&Yaml> {
        let value = &self.doc.as_ref()?[key];
        (!value.is_badvalue() && !value.is_null()).then_some(value)
    }

    /// A value as text; numbers and booleans are converted, and blank values count as missing.
    pub fn str(&self, key: &str) -> Option<String> {
        scalar(self.get(key)?)
    }

    /// A whole number, accepting `90000`, `90,000` or `"90 000"`.
    pub fn number(&self, key: &str) -> Option<u64> {
        match self.get(key)? {
            Yaml::Integer(i) => u64::try_from(*i).ok(),
            Yaml::Real(r) => r
                .parse::<f64>()
                .ok()
                .filter(|f| *f >= 0.0)
                .map(|f| f.round() as u64),
            Yaml::String(s) => s
                .chars()
                .filter(|c| !matches!(c, ',' | '_' | ' '))
                .collect::<String>()
                .parse()
                .ok(),
            _ => None,
        }
    }

    /// A list of values; a single value is treated as a list of one.
    pub fn list(&self, key: &str) -> Vec<String> {
        match self.get(key) {
            Some(Yaml::Array(items)) => items.iter().filter_map(scalar).collect(),
            Some(value) => scalar(value).into_iter().collect(),
            None => Vec::new(),
        }
    }

    /// Every top-level property whose value is text, a number, a list or blank, in file order.
    pub fn entries(&self) -> Vec<(String, PropertyValue)> {
        let Some(Yaml::Hash(hash)) = &self.doc else {
            return Vec::new();
        };
        hash.iter()
            .filter_map(|(key, value)| {
                let value = match value {
                    Yaml::Array(items) => {
                        PropertyValue::List(items.iter().filter_map(scalar).collect())
                    }
                    Yaml::Null => PropertyValue::Text(String::new()),
                    Yaml::Hash(_) | Yaml::Alias(_) | Yaml::BadValue => return None,
                    other => PropertyValue::Text(scalar(other).unwrap_or_default()),
                };
                Some((scalar(key)?, value))
            })
            .collect()
    }

    pub fn date(&self, key: &str) -> Option<NaiveDate> {
        NaiveDate::parse_from_str(&self.str(key)?, "%Y-%m-%d").ok()
    }
}

fn scalar(value: &Yaml) -> Option<String> {
    let text = match value {
        Yaml::String(s) | Yaml::Real(s) => s.clone(),
        Yaml::Integer(i) => i.to_string(),
        Yaml::Boolean(b) => b.to_string(),
        _ => return None,
    };
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

#[derive(Debug, Clone)]
pub struct Note {
    pub path: PathBuf,
    /// The file name without `.md`, which is what Obsidian links resolve against.
    pub name: String,
    pub frontmatter: Frontmatter,
    /// Set when the frontmatter exists but is not valid YAML.
    pub frontmatter_error: Option<String>,
    pub body: String,
}

impl Note {
    pub fn load(path: &Path) -> Result<Note> {
        let content =
            fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Ok(Note::parse(path, &content))
    }

    pub fn parse(path: &Path, content: &str) -> Note {
        let (yaml, body) = split_frontmatter(content);
        let (frontmatter, frontmatter_error) = match yaml.map(Frontmatter::parse) {
            None => (Frontmatter::default(), None),
            Some(Ok(frontmatter)) => (frontmatter, None),
            Some(Err(error)) => (Frontmatter::default(), Some(error)),
        };
        Note {
            path: path.to_path_buf(),
            name: path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            frontmatter,
            frontmatter_error,
            body: body.to_string(),
        }
    }

    /// The first level-one heading in the body.
    pub fn heading(&self) -> Option<String> {
        self.body
            .lines()
            .find_map(|line| line.strip_prefix("# "))
            .map(|heading| words::plain_text(heading.trim()))
            .filter(|heading| !heading.is_empty())
    }

    /// The note's name as a reader sees it: the `title` property (projects and books), else the
    /// first alias, else the first heading, else one made from the file name.
    pub fn title(&self) -> String {
        self.given_name()
            .or_else(|| self.heading())
            .unwrap_or_else(|| name_from_file(&self.name))
    }

    /// The name set in the note's properties: its `title`, else its first alias. The file name
    /// is made from this, so `check` compares the two.
    pub fn given_name(&self) -> Option<String> {
        self.frontmatter
            .str("title")
            .or_else(|| self.aliases().into_iter().next())
    }

    pub fn aliases(&self) -> Vec<String> {
        self.frontmatter.list("aliases")
    }

    pub fn words(&self) -> usize {
        words::count_words(&self.body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_frontmatter() {
        let (yaml, body) = split_frontmatter("---\nstatus: draft\n---\n# One\n");
        assert_eq!(yaml, Some("status: draft\n"));
        assert_eq!(body, "# One\n");
        assert_eq!(
            split_frontmatter("No frontmatter\n---\n"),
            (None, "No frontmatter\n---\n")
        );
        assert_eq!(
            split_frontmatter("---\nunclosed: true\n"),
            (None, "---\nunclosed: true\n")
        );
        assert_eq!(
            split_frontmatter("---\r\na: 1\r\n---\r\nBody"),
            (Some("a: 1\r\n"), "Body")
        );
    }

    #[test]
    fn removes_properties() {
        let content = "---\ntype: chapter\nproject: \"[[old]]\"\naliases:\n  - A\n# kept\nstatus: draft\n---\nBody\n";
        assert_eq!(
            remove_properties(content, &["project", "aliases"]),
            "---\ntype: chapter\n# kept\nstatus: draft\n---\nBody\n"
        );
        assert_eq!(remove_properties("Body\n", &["type"]), "Body\n");
    }

    #[test]
    fn reads_and_replaces_sections() {
        let content = "---\ntype: project\n---\n# World\n\n## Books\n\n- [[one]]\n\n### Not the end\n\n## Notes\n\nKept.\n";
        let (_, body) = split_frontmatter(content);
        assert_eq!(
            section(body, "books"),
            Some(vec!["", "- [[one]]", "", "### Not the end", ""])
        );
        assert_eq!(section(body, "Themes"), None);
        let lines = ["- [[one]]".to_string(), "- [[two]]".to_string()];
        assert_eq!(
            set_section(content, "Books", &lines),
            "---\ntype: project\n---\n# World\n\n## Books\n\n- [[one]]\n- [[two]]\n\n## Notes\n\nKept.\n"
        );
        assert_eq!(
            set_section("---\na: 1\n---\n# World\n\n", "Books", &lines),
            "---\na: 1\n---\n# World\n\n## Books\n\n- [[one]]\n- [[two]]\n",
            "a missing section is added at the end"
        );
        assert_eq!(
            set_section("# World\n\n## Books\n- [[one]]\n", "Books", &lines),
            "# World\n\n## Books\n\n- [[one]]\n- [[two]]\n"
        );
    }

    #[test]
    fn reads_loosely_typed_values() {
        let note = Note::parse(
            Path::new("notes/Mara Chen.md"),
            "---\ntarget: \"90,000\"\nstatus: draft\npov:\naliases: [Mara, Captain Chen]\nsolo: Reyes\ncreated: 2026-10-30\n---\nText\n",
        );
        let fm = &note.frontmatter;
        assert_eq!(fm.number("target"), Some(90_000));
        assert_eq!(fm.str("status").as_deref(), Some("draft"));
        assert_eq!(fm.str("pov"), None);
        assert_eq!(fm.list("aliases"), ["Mara", "Captain Chen"]);
        assert_eq!(fm.list("solo"), ["Reyes"]);
        assert_eq!(fm.date("created"), NaiveDate::from_ymd_opt(2026, 10, 30));
        assert_eq!(note.name, "Mara Chen");
        assert_eq!(note.title(), "Mara", "the first alias is the note's name");
        let untitled = Note::parse(Path::new("notes/the-long-night.md"), "Text\n");
        assert_eq!(untitled.title(), "The long night");
        assert_eq!(untitled.given_name(), None);
    }

    #[test]
    fn title_prefers_property_then_heading() {
        let with_heading = Note::parse(Path::new("01 x.md"), "# The [[Harbour]]\n\nText");
        assert_eq!(with_heading.title(), "The Harbour");
        let with_property = Note::parse(
            Path::new("01 x.md"),
            "---\ntitle: Opening\n---\n# Heading\n",
        );
        assert_eq!(with_property.title(), "Opening");
    }

    #[test]
    fn sets_properties_without_disturbing_others() {
        let content = "---\ntype: chapter\n# keep this comment\nstatus: outline\naliases:\n  - Old\n  - Name\npov:\n---\n# One\n\nText.\n";
        let updated = set_properties(
            content,
            &[
                ("status", PropertyValue::Text("draft".into())),
                ("pov", PropertyValue::Text("[[Mara Chen]]".into())),
                (
                    "aliases",
                    PropertyValue::List(vec!["Mara".into(), " ".into()]),
                ),
                (
                    "synopsis",
                    PropertyValue::Text("She learns: the catch.".into()),
                ),
            ],
        );
        assert_eq!(
            updated,
            "---\ntype: chapter\n# keep this comment\nstatus: draft\naliases:\n  - Mara\npov: \"[[Mara Chen]]\"\nsynopsis: \"She learns: the catch.\"\n---\n# One\n\nText.\n"
        );
        let note = Note::parse(Path::new("x.md"), &updated);
        assert_eq!(
            note.frontmatter.str("pov").as_deref(),
            Some("[[Mara Chen]]")
        );
        assert_eq!(
            note.frontmatter.str("synopsis").as_deref(),
            Some("She learns: the catch.")
        );
        assert_eq!(note.aliases(), ["Mara"]);
    }

    #[test]
    fn sets_properties_on_plain_notes_and_edge_values() {
        let updated = set_properties(
            "Just text\n",
            &[
                ("status", PropertyValue::Text("yes".into())),
                ("aliases", PropertyValue::List(vec![])),
                ("synopsis", PropertyValue::Text("Line one\nLine two".into())),
                ("pov", PropertyValue::Text("  ".into())),
            ],
        );
        assert_eq!(
            updated,
            "---\nstatus: \"yes\"\naliases: []\nsynopsis: |-\n  Line one\n  Line two\npov:\n---\nJust text\n"
        );
        let note = Note::parse(Path::new("x.md"), &updated);
        assert_eq!(note.frontmatter.str("status").as_deref(), Some("yes"));
        assert_eq!(
            note.frontmatter.str("synopsis").as_deref(),
            Some("Line one\nLine two")
        );
        assert_eq!(note.frontmatter.str("pov"), None);
        assert_eq!(note.body, "Just text\n");
    }

    #[test]
    fn lists_entries_and_replaces_bodies() {
        let content = "---\nstatus: draft\ntarget: 90000\naliases: [A, B]\npov:\nnested:\n  a: 1\n---\nOld body\n";
        let note = Note::parse(Path::new("x.md"), content);
        assert_eq!(
            note.frontmatter.entries(),
            [
                ("status".to_string(), PropertyValue::Text("draft".into())),
                ("target".to_string(), PropertyValue::Text("90000".into())),
                (
                    "aliases".to_string(),
                    PropertyValue::List(vec!["A".into(), "B".into()])
                ),
                ("pov".to_string(), PropertyValue::Text(String::new())),
            ]
        );
        let replaced = replace_body(content, "New body\n");
        assert!(
            replaced.starts_with("---\nstatus: draft\n") && replaced.ends_with("---\nNew body\n")
        );
        assert_eq!(replace_body("No frontmatter", "New"), "New");
    }

    #[test]
    fn saves_edits_and_refuses_to_overwrite_changes_made_elsewhere() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("01 One.md");
        let original = "---\nstatus: outline\n---\n# One\n";
        fs::write(&path, original).unwrap();
        let hash = content_hash(original);

        let saved = save(
            &path,
            Some("# One\n\nTwo more words.\n"),
            &[("status", PropertyValue::Text("draft".into()))],
            &hash,
            false,
        )
        .unwrap();
        let written = fs::read_to_string(&path).unwrap();
        assert_eq!(
            written,
            "---\nstatus: draft\n---\n# One\n\nTwo more words.\n"
        );
        assert_eq!(
            saved,
            Saved::Written {
                hash: content_hash(&written),
                words: 3
            }
        );
        assert_eq!(
            fs::read_dir(dir.path()).unwrap().count(),
            1,
            "no temporary file is left behind"
        );

        // Edited elsewhere: a save based on the old content is refused...
        fs::write(
            &path,
            "---\nstatus: draft\n---\n# One\n\nChanged in Obsidian.\n",
        )
        .unwrap();
        let refused = save(&path, Some("Mine.\n"), &[], &content_hash(&written), false).unwrap();
        assert!(matches!(refused, Saved::Conflict { .. }));
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("Changed in Obsidian.")
        );
        // ...unless forced, which keeps the other side's properties and replaces the body.
        save(&path, Some("Mine.\n"), &[], "stale", true).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "---\nstatus: draft\n---\nMine.\n"
        );
    }

    #[test]
    fn keeps_body_when_frontmatter_is_broken() {
        let note = Note::parse(
            Path::new("x.md"),
            "---\nstatus: [unclosed\n---\nThree words here\n",
        );
        assert!(note.frontmatter_error.is_some());
        assert_eq!(note.words(), 3);
    }
}
