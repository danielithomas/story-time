//! Settings: per-vault in `.story-time/config.toml`, and a per-machine default vault.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// Folder inside the vault for story-time's own files. Obsidian ignores dot-folders.
pub const META_DIR: &str = ".story-time";
pub const CONFIG_FILE: &str = "config.toml";

/// The newest vault format this build understands (docs/vault-format.md). A vault without a
/// `format` setting uses the unversioned layout of story-time 0.1, format 0, which
/// `story-time upgrade` converts.
pub const FORMAT: u32 = 1;

/// Introduces the `format` setting in the config file.
const FORMAT_COMMENT: &str =
    "# The layout of this vault, so story-time reads it right. `story-time upgrade` changes it.";
/// Introduces the `scratchpad` setting in the config file.
const SCRATCHPAD_COMMENT: &str =
    "# The vault's scratchpad: fragments not tied to any world (`story-time scrap add`).";

/// The config file written by `story-time init`; it documents every setting.
pub const DEFAULT_CONFIG: &str = r#"# story-time settings for this vault.

# The layout of this vault, so story-time reads it right. `story-time upgrade` changes it.
format = 1

# Words you aim to write each day (used by `story-time today` and `story-time history`).
daily_goal = 500

# Manuscript estimates. A standard manuscript page holds about 250 words,
# and adults read fiction at about 250 words a minute.
words_per_page = 250
reading_wpm = 250

# Where `story-time idea` captures ideas, relative to the vault.
ideas_file = "scratchpad/ideas.md"

# The vault's scratchpad: fragments not tied to any world (`story-time scrap add`).
scratchpad = "scratchpad"

# Note templates, shared with Obsidian's core Templates plugin.
templates_dir = "templates"

# Name printed on compiled manuscripts.
# author = "Your Name"

# Spellcheck language(s) for the desktop app's editor, as language_COUNTRY codes.
# Defaults to the system language.
# spelling = ["en_AU"]
"#;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Config {
    pub format: u32,
    pub daily_goal: u64,
    pub words_per_page: u64,
    pub reading_wpm: u64,
    pub ideas_file: String,
    pub scratchpad: String,
    pub templates_dir: String,
    pub author: Option<String>,
    pub spelling: Option<Vec<String>>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            format: 0,
            daily_goal: 500,
            words_per_page: 250,
            reading_wpm: 250,
            ideas_file: "scratchpad/ideas.md".to_string(),
            scratchpad: "scratchpad".to_string(),
            templates_dir: "templates".to_string(),
            author: None,
            spelling: None,
        }
    }
}

impl Config {
    /// Loads the vault's config, or the defaults if it has none. A vault story-time has never
    /// set up is new, so it is in the current format.
    pub fn load(vault_root: &Path) -> Result<Config> {
        let path = vault_root.join(META_DIR).join(CONFIG_FILE);
        if !path.exists() {
            return Ok(Config {
                format: FORMAT,
                ..Config::default()
            });
        }
        let text =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    /// Fails for a vault in a newer format than this build understands, so nothing misreads or
    /// edits it.
    pub fn ensure_supported(&self) -> Result<()> {
        if self.format > FORMAT {
            bail!(
                "this vault uses format {}, which is newer than this version of story-time \
                 understands (up to format {FORMAT}). Update story-time to open it.",
                self.format
            );
        }
        Ok(())
    }

    /// Whether the vault is in an older format, which `story-time upgrade` converts.
    pub fn needs_upgrade(&self) -> bool {
        self.format < FORMAT
    }

    /// Fails for a vault in an older format, which story-time would misread until it's upgraded.
    pub fn ensure_current(&self) -> Result<()> {
        if self.needs_upgrade() {
            bail!(
                "this vault is laid out the way story-time 0.1 did it. Run `story-time upgrade` \
                 to convert it; it backs the vault up first."
            );
        }
        Ok(())
    }
}

/// A format-0 config file upgraded to format 1: `format` added before the first setting, the ideas
/// file and templates pointed at their new folders, and the scratchpad set after the ideas file.
/// Comments and other settings stay as written.
pub fn upgraded_config(text: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let find =
        |lines: &[String], key: &str| lines.iter().position(|line| setting(line) == Some(key));
    for (key, value) in [
        ("ideas_file", "scratchpad/ideas.md"),
        ("templates_dir", "templates"),
    ] {
        let line = format!("{key} = \"{value}\"");
        match find(&lines, key) {
            Some(i) => lines[i] = line,
            None => lines.extend([String::new(), line]),
        }
    }
    if find(&lines, "scratchpad").is_none() {
        let at = find(&lines, "ideas_file").expect("set above") + 1;
        let mut block = vec!["", SCRATCHPAD_COMMENT, "scratchpad = \"scratchpad\""];
        if lines.get(at).is_some_and(|line| !line.trim().is_empty()) {
            block.push("");
        }
        lines.splice(at..at, block.into_iter().map(String::from));
    }
    let format = format!("format = {FORMAT}");
    match find(&lines, "format") {
        Some(i) => lines[i] = format,
        None => {
            let first = lines
                .iter()
                .position(|line| setting(line).is_some())
                .unwrap_or(lines.len());
            let at = (0..first)
                .rev()
                .take_while(|&i| lines[i].starts_with('#'))
                .last()
                .unwrap_or(first);
            lines.splice(at..at, [FORMAT_COMMENT.to_string(), format, String::new()]);
        }
    }
    lines.join("\n") + "\n"
}

/// The key a line of the config file sets, if it sets one.
fn setting(line: &str) -> Option<&str> {
    if line.starts_with(['#', ' ', '\t', '[']) {
        return None;
    }
    line.split_once('=').map(|(key, _)| key.trim())
}

/// Per-machine settings, so commands find the vault from any directory (for example a vault kept on OneDrive).
#[derive(Debug, Default, Deserialize)]
struct UserConfig {
    vault: Option<String>,
}

/// `~/.config/story-time/config.toml` on Linux; the platform equivalent elsewhere.
pub fn user_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("story-time").join(CONFIG_FILE))
}

/// The `vault` set in the user config file, if there is one.
pub fn user_default_vault() -> Result<Option<PathBuf>> {
    let Some(path) = user_config_path().filter(|path| path.is_file()) else {
        return Ok(None);
    };
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let config: UserConfig =
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    Ok(config.vault.map(|vault| expand_home(&vault)))
}

/// Remembers `vault` as this machine's default, keeping anything else in the user config file.
pub fn set_user_default_vault(vault: &Path) -> Result<()> {
    let path = user_config_path().context("this system has no config folder")?;
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let setting = format!(
        "vault = {}",
        toml::Value::String(vault.display().to_string())
    );
    let mut lines: Vec<String> = existing
        .lines()
        .filter(|line| {
            !line.trim_start().starts_with("vault ") && !line.trim_start().starts_with("vault=")
        })
        .map(String::from)
        .collect();
    lines.insert(0, setting);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(&path, lines.join("\n") + "\n").with_context(|| format!("writing {}", path.display()))
}

fn expand_home(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_file_matches_defaults() {
        let parsed: Config = toml::from_str(DEFAULT_CONFIG).unwrap();
        assert_eq!(
            parsed,
            Config {
                format: FORMAT,
                ..Config::default()
            }
        );
    }

    #[test]
    fn upgrades_config_files() {
        let old = "# story-time settings for this vault.\n\n# Words you aim to write each day (used by `story-time today` and `story-time history`).\ndaily_goal = 500\n\n# Manuscript estimates. A standard manuscript page holds about 250 words,\n# and adults read fiction at about 250 words a minute.\nwords_per_page = 250\nreading_wpm = 250\n\n# Where `story-time idea` captures ideas, relative to the vault.\nideas_file = \"Scratchpad/Ideas.md\"\n\n# Note templates, shared with Obsidian's core Templates plugin.\ntemplates_dir = \"Templates\"\n\n# Name printed on compiled manuscripts.\n# author = \"Your Name\"\n\n# Spellcheck language(s) for the desktop app's editor, as language_COUNTRY codes.\n# Defaults to the system language.\n# spelling = [\"en_AU\"]\n";
        assert_eq!(
            upgraded_config(old),
            DEFAULT_CONFIG,
            "story-time 0.1's config"
        );
        let upgraded: Config = toml::from_str(&upgraded_config("daily_goal = 900\n")).unwrap();
        assert_eq!(
            upgraded,
            Config {
                format: FORMAT,
                daily_goal: 900,
                ..Config::default()
            }
        );
    }

    #[test]
    fn partial_files_fill_in_defaults() {
        let parsed: Config =
            toml::from_str("daily_goal = 1000\nauthor = \"A. Writer\"\nspelling = [\"en_AU\"]")
                .unwrap();
        assert_eq!(parsed.spelling, Some(vec!["en_AU".to_string()]));
        assert_eq!(parsed.daily_goal, 1000);
        assert_eq!(parsed.author.as_deref(), Some("A. Writer"));
        assert_eq!(parsed.words_per_page, 250);
        assert_eq!(
            parsed.format, 0,
            "no format setting means the unversioned layout"
        );
    }

    #[test]
    fn refuses_newer_formats() {
        let current: Config = toml::from_str(&format!("format = {FORMAT}")).unwrap();
        assert!(current.ensure_supported().is_ok());
        let newer: Config = toml::from_str(&format!("format = {}", FORMAT + 1)).unwrap();
        let message = newer.ensure_supported().unwrap_err().to_string();
        assert!(message.contains("newer"), "{message}");
    }
}
