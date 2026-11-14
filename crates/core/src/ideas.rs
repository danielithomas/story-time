//! The ideas file: capturing ideas, and promoting one into a project.
//!
//! Ideas are list items under headings such as `### Fiction`, written as `Name: description`.
//! Nothing here deletes an idea; promoting one only appends a link to the project it became.

use std::fs;
use std::io;
use std::sync::LazyLock;

use anyhow::{Context, Result};
use regex::Regex;

use crate::vault::Vault;

/// The vault's ideas file, or nothing if it doesn't exist yet.
pub fn read(vault: &Vault) -> Result<String> {
    let path = vault.ideas_path();
    match fs::read_to_string(&path) {
        Ok(content) => Ok(content),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

/// Adds an idea to the vault's ideas file under `section`, creating the file if needed.
pub fn capture(vault: &Vault, section: &str, idea: &str) -> Result<()> {
    let path = vault.ideas_path();
    let content = add_idea(&read(vault)?, section, idea.trim());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(&path, content).with_context(|| format!("writing {}", path.display()))
}

static LIST_ITEM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:([-*+])|(\d+)[.)])\s+(.*)$").unwrap());

/// Adds `idea` as the last item under the heading named `section`, matching the list style
/// already there. The section is created at the end of the file if it does not exist.
pub fn add_idea(content: &str, section: &str, idea: &str) -> String {
    let mut lines: Vec<String> = content.lines().map(String::from).collect();
    match find_section(&lines, section) {
        Some((heading, end)) => {
            let item = format!("{} {idea}", list_marker(&lines[heading + 1..end]));
            match (heading + 1..end)
                .rev()
                .find(|&i| !lines[i].trim().is_empty())
            {
                Some(last) if LIST_ITEM.is_match(&lines[last]) => lines.insert(last + 1, item),
                Some(last) => lines
                    .splice(last + 1..last + 1, [String::new(), item])
                    .for_each(drop),
                None => lines
                    .splice(heading + 1..heading + 1, [String::new(), item])
                    .for_each(drop),
            }
        }
        None => {
            if lines.last().is_some_and(|line| !line.trim().is_empty()) {
                lines.push(String::new());
            }
            lines.extend([
                format!("### {section}"),
                String::new(),
                format!("1. {idea}"),
            ]);
        }
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// The heading line for `name` and the line where its section ends.
fn find_section(lines: &[String], name: &str) -> Option<(usize, usize)> {
    let level = |line: &str| {
        let hashes = line.chars().take_while(|c| *c == '#').count();
        (hashes > 0 && line[hashes..].starts_with(' ')).then_some(hashes)
    };
    let heading = lines.iter().position(|line| {
        level(line).is_some()
            && line
                .trim_start_matches('#')
                .trim()
                .eq_ignore_ascii_case(name)
    })?;
    let heading_level = level(&lines[heading])?;
    let end = (heading + 1..lines.len())
        .find(|&i| level(&lines[i]).is_some_and(|l| l <= heading_level))
        .unwrap_or(lines.len());
    Some((heading, end))
}

/// The marker for a new item: the section's bullet, "1." when every item is numbered 1 (letting
/// Markdown number them), or the next number.
fn list_marker(section: &[String]) -> String {
    let items: Vec<_> = section
        .iter()
        .filter_map(|line| LIST_ITEM.captures(line))
        .collect();
    match items.last() {
        None => "1.".to_string(),
        Some(last) => match (last.get(1), last.get(2)) {
            (Some(bullet), _) => bullet.as_str().to_string(),
            (None, Some(_))
                if items
                    .iter()
                    .all(|item| item.get(2).is_some_and(|n| n.as_str() == "1")) =>
            {
                "1.".to_string()
            }
            (None, Some(number)) => format!("{}.", number.as_str().parse::<u64>().unwrap_or(0) + 1),
            (None, None) => "1.".to_string(),
        },
    }
}

/// An idea as listed in the ideas file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdeaEntry {
    /// The heading it sits under, such as "Fiction".
    pub section: String,
    /// Its name: the text before the first `:` or `.`, or all of it if short.
    pub name: String,
    /// The whole idea, without any project link.
    pub text: String,
    /// The project it became, if it has been linked to one.
    pub project: Option<String>,
}

static PROJECT_LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s*→\s*\[\[([^\]|]+)(?:\|[^\]]*)?\]\]\s*$").unwrap());

/// An idea's text without the links to the project it became, and that project: the last link,
/// since renaming a project appends a link to its new name.
fn without_project_links(item: &str) -> (String, Option<String>) {
    let mut text = item.trim().to_string();
    let mut project = None;
    while let Some(link) = PROJECT_LINK.captures(&text) {
        project.get_or_insert_with(|| link[1].trim().to_string());
        let start = link.get(0).expect("a match").start();
        text.truncate(start);
    }
    (text.trim().to_string(), project)
}

/// Every list item in the ideas file, with the heading it belongs to.
pub fn list_ideas(content: &str) -> Vec<IdeaEntry> {
    let mut section = String::new();
    let mut ideas = Vec::new();
    for line in content.lines() {
        if line.starts_with('#') {
            section = line.trim_start_matches('#').trim().to_string();
            continue;
        }
        let Some(item) = LIST_ITEM.captures(line).and_then(|c| c.get(3)) else {
            continue;
        };
        let (text, project) = without_project_links(item.as_str());
        if text.is_empty() {
            continue;
        }
        let name = match text.find([':', '.']) {
            Some(end) if end > 0 => text[..end].trim().to_string(),
            _ if text.chars().count() <= 60 => text.clone(),
            _ => text.chars().take(60).collect::<String>().trim().to_string() + "…",
        };
        ideas.push(IdeaEntry {
            section: section.clone(),
            name,
            text,
            project,
        });
    }
    ideas
}

#[derive(Debug, PartialEq, Eq)]
pub struct Idea {
    /// Line index in the file.
    pub line: usize,
    /// The description after the idea's name.
    pub premise: String,
}

/// Finds the idea called `name`: a list item starting with the name followed by `:`, `.`, `-`
/// or a dash. Matching ignores case.
pub fn find_idea(content: &str, name: &str) -> Option<Idea> {
    let name = name.trim();
    content.lines().enumerate().find_map(|(line, text)| {
        let item = LIST_ITEM.captures(text)?.get(3)?.as_str().trim();
        let head = item.get(..name.len())?;
        if !head.eq_ignore_ascii_case(name) && head.to_lowercase() != name.to_lowercase() {
            return None;
        }
        let rest = &item[name.len()..];
        let premise = if rest.trim().is_empty() {
            ""
        } else {
            rest.trim_start()
                .strip_prefix(&[':', '.', '-', '—', '–'][..])?
        };
        let premise = premise.split(" → [[").next().unwrap_or_default().trim();
        Some(Idea {
            line,
            premise: premise.to_string(),
        })
    })
}

/// Appends `link`, a link to the project an idea became, unless the idea already has it.
pub fn link_idea(content: &str, line: usize, link: &str) -> String {
    let mut out: Vec<String> = content
        .lines()
        .enumerate()
        .map(|(i, text)| {
            if i == line && !text.contains(link) {
                format!("{} → {link}", text.trim_end())
            } else {
                text.to_string()
            }
        })
        .collect::<Vec<_>>();
    if content.ends_with('\n') {
        out.push(String::new());
    }
    out.join("\n")
}

/// After the project at `old` (its file name, as ideas link to it) was renamed, appends `link`, a
/// link to it under its new name, to each idea that became it. Nothing is removed or reworded, so
/// the old link stays before the new one. Returns the new text and how many ideas changed.
pub fn link_renamed_project(content: &str, old: &str, link: &str) -> (String, usize) {
    let mut content = content.to_string();
    let lines: Vec<usize> = content
        .lines()
        .enumerate()
        .filter_map(|(i, line)| {
            let item = LIST_ITEM.captures(line)?.get(3)?;
            let (_, project) = without_project_links(item.as_str());
            project.filter(|p| p.eq_ignore_ascii_case(old)).map(|_| i)
        })
        .collect();
    for &line in &lines {
        content = link_idea(&content, line, link);
    }
    (content, lines.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDEAS: &str = "\n# Ideas\n\n## Ideas\n\n### Fiction\n\n1. Sample Story: A premise, with a catch.\n1. Second: Another premise.\n1. Third. A premise with _emphasis_.    \n\n### Non-Fiction\n\n\n";

    #[test]
    fn appends_to_a_list_in_its_style() {
        let out = add_idea(IDEAS, "Fiction", "Added: A new premise.");
        assert!(out.contains(
            "1. Third. A premise with _emphasis_.    \n1. Added: A new premise.\n\n### Non-Fiction"
        ));
        let numbered = add_idea("## Fiction\n1. One\n2. Two\n", "fiction", "Three");
        assert_eq!(numbered, "## Fiction\n1. One\n2. Two\n3. Three\n");
        let bullets = add_idea("## Fiction\n- One\n\n## Other\n- X\n", "Fiction", "Two");
        assert_eq!(bullets, "## Fiction\n- One\n- Two\n\n## Other\n- X\n");
    }

    #[test]
    fn fills_an_empty_or_missing_section() {
        let out = add_idea(IDEAS, "Non-Fiction", "How writers plan.");
        assert!(
            out.ends_with("### Non-Fiction\n\n1. How writers plan.\n\n\n"),
            "{out:?}"
        );
        assert_eq!(
            add_idea("# Ideas\n", "Fiction", "One"),
            "# Ideas\n\n### Fiction\n\n1. One\n"
        );
        assert_eq!(add_idea("", "Fiction", "One"), "### Fiction\n\n1. One\n");
    }

    #[test]
    fn finds_ideas_by_name() {
        let sample = find_idea(IDEAS, "sample story").unwrap();
        assert_eq!(
            sample,
            Idea {
                line: 7,
                premise: "A premise, with a catch.".to_string()
            }
        );
        assert_eq!(
            find_idea(IDEAS, "Third").unwrap().premise,
            "A premise with _emphasis_."
        );
        assert!(
            find_idea(IDEAS, "Sample").is_none(),
            "a partial name is not a match"
        );
        assert!(find_idea(IDEAS, "Missing").is_none());
    }

    #[test]
    fn lists_ideas_with_their_projects() {
        let linked = link_idea(IDEAS, 7, "[[sample-story-world|Sample Story]]");
        let ideas = list_ideas(&format!("{linked}- A loose note\n"));
        let names: Vec<_> = ideas.iter().map(|idea| idea.name.as_str()).collect();
        assert_eq!(names, ["Sample Story", "Second", "Third", "A loose note"]);
        assert_eq!(ideas[0].project.as_deref(), Some("sample-story-world"));
        assert_eq!(ideas[0].text, "Sample Story: A premise, with a catch.");
        assert_eq!(ideas[0].section, "Fiction");
        assert_eq!(ideas[3].section, "Non-Fiction");
        assert!(ideas[1].project.is_none());
        for idea in &ideas[..3] {
            assert!(
                find_idea(&linked, &idea.name).is_some(),
                "{} should be findable by name",
                idea.name
            );
        }
    }

    #[test]
    fn follows_a_renamed_project_by_adding_to_its_line() {
        let linked = link_idea(IDEAS, 7, "[[sample-story-world|Sample Story]]");
        let (renamed, count) =
            link_renamed_project(&linked, "sample-story-world", "[[the-deep|The Deep]]");
        assert_eq!(count, 1);
        assert!(renamed.contains(
            "1. Sample Story: A premise, with a catch. → [[sample-story-world|Sample Story]] → [[the-deep|The Deep]]\n"
        ));
        let idea = &list_ideas(&renamed)[0];
        assert_eq!(idea.project.as_deref(), Some("the-deep"));
        assert_eq!(idea.text, "Sample Story: A premise, with a catch.");
        assert_eq!(
            find_idea(&renamed, "Sample Story").unwrap().premise,
            "A premise, with a catch."
        );
        assert_eq!(
            link_renamed_project(&renamed, "elsewhere", "[[x|X]]"),
            (renamed.clone(), 0)
        );
    }

    #[test]
    fn links_an_idea_once() {
        let linked = link_idea(IDEAS, 9, "[[third|Third]]");
        assert!(linked.contains("1. Third. A premise with _emphasis_. → [[third|Third]]\n"));
        assert_eq!(link_idea(&linked, 9, "[[third|Third]]"), linked);
        assert_eq!(
            find_idea(&linked, "Third").unwrap().premise,
            "A premise with _emphasis_."
        );
        assert!(linked.ends_with("### Non-Fiction\n\n\n"));
    }
}
