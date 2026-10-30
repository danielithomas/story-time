//! Note templates. The vault's own copies (in `Templates/` by default) take precedence over the
//! built-in versions, so a template edited in Obsidian changes what story-time creates too.
//!
//! Obsidian's Templates plugin fills `{{title}}`, `{{date}}` and `{{time}}`. story-time fills those
//! plus `{{project}}`, `{{project_slug}}`, `{{slug}}`, `{{form}}`, `{{target}}`, `{{premise}}`,
//! `{{structure}}` and `{{structure_name}}`, and leaves any other placeholder as it is.

use std::fs;
use std::sync::LazyLock;

use anyhow::{Context, Result};
use regex::{Captures, Regex};

use crate::note::split_frontmatter;
use crate::project::NoteKind;
use crate::text::yaml_escape;
use crate::vault::Vault;

pub struct Template {
    pub file_name: &'static str,
    pub default: &'static str,
}

pub const PROJECT: Template = Template {
    file_name: "Project.md",
    default: include_str!("../templates/Project.md"),
};
pub const FLOW: Template = Template {
    file_name: "Story Flow.md",
    default: include_str!("../templates/Story Flow.md"),
};
pub const CHAPTER: Template = Template {
    file_name: "Chapter.md",
    default: include_str!("../templates/Chapter.md"),
};
pub const CHARACTER: Template = Template {
    file_name: "Character.md",
    default: include_str!("../templates/Character.md"),
};
pub const LOCATION: Template = Template {
    file_name: "Location.md",
    default: include_str!("../templates/Location.md"),
};
pub const WORLD: Template = Template {
    file_name: "World.md",
    default: include_str!("../templates/World.md"),
};

pub const ALL: [&Template; 6] = [&PROJECT, &FLOW, &CHAPTER, &CHARACTER, &LOCATION, &WORLD];

pub fn for_kind(kind: NoteKind) -> &'static Template {
    match kind {
        NoteKind::Chapter => &CHAPTER,
        NoteKind::Character => &CHARACTER,
        NoteKind::Location => &LOCATION,
        NoteKind::World => &WORLD,
    }
}

impl Template {
    /// The vault's copy if it has one, else the built-in default.
    pub fn load(&self, vault: &Vault) -> Result<String> {
        let path = vault.templates_dir().join(self.file_name);
        if path.is_file() {
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))
        } else {
            Ok(self.default.to_string())
        }
    }
}

static PLACEHOLDER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{\s*([A-Za-z_]+)(?::[^}]*)?\s*\}\}").unwrap());

/// Fills `{{name}}` placeholders. Values placed in the frontmatter are escaped for double-quoted
/// YAML, so a title containing quotes cannot break the note's properties.
pub fn render(template: &str, vars: &[(&str, &str)]) -> String {
    let fill = |text: &str, in_yaml: bool| {
        PLACEHOLDER
            .replace_all(text, |c: &Captures| {
                match vars.iter().find(|(name, _)| *name == &c[1]) {
                    Some((_, value)) if in_yaml => yaml_escape(value),
                    Some((_, value)) => value.to_string(),
                    None => c[0].to_string(),
                }
            })
            .into_owned()
    };
    match split_frontmatter(template) {
        (Some(yaml), body) => format!("---\n{}---\n{}", fill(yaml, true), fill(body, false)),
        (None, body) => fill(body, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note::Note;
    use std::path::Path;

    #[test]
    fn fills_known_placeholders_only() {
        let out = render(
            "---\ntitle: \"{{title}}\"\n---\n# {{title}} on {{date:YYYY-MM-DD}} {{unknown}}\n",
            &[("title", "The \"Sample\""), ("date", "2026-10-30")],
        );
        assert_eq!(
            out,
            "---\ntitle: \"The \\\"Sample\\\"\"\n---\n# The \"Sample\" on 2026-10-30 {{unknown}}\n"
        );
        let note = Note::parse(Path::new("x.md"), &out);
        assert_eq!(
            note.frontmatter.str("title").as_deref(),
            Some("The \"Sample\"")
        );
    }

    #[test]
    fn built_in_templates_have_valid_frontmatter() {
        let vars = [
            ("title", "Sample: of \"Stories\""),
            ("project", "Sample"),
            ("project_slug", "sample"),
            ("slug", "sample"),
            ("form", "novel"),
            ("target", "90000"),
            ("date", "2026-10-30"),
            ("premise", ""),
            ("structure", ""),
            ("structure_name", "three-act"),
        ];
        for template in ALL {
            let note = Note::parse(Path::new("x.md"), &render(template.default, &vars));
            assert!(
                note.frontmatter_error.is_none(),
                "{}: {:?}",
                template.file_name,
                note.frontmatter_error
            );
            assert!(
                note.frontmatter.str("type").is_some(),
                "{} has no type",
                template.file_name
            );
        }
    }
}
