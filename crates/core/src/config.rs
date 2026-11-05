//! Settings: per-vault in `.story-time/config.toml`, and a per-machine default vault.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// Folder inside the vault for story-time's own files. Obsidian ignores dot-folders.
pub const META_DIR: &str = ".story-time";
pub const CONFIG_FILE: &str = "config.toml";

/// The newest vault format this build understands (docs/vault-format.md). A vault without a
/// `format` setting uses the unversioned layout, format 0.
pub const FORMAT: u32 = 0;

/// The config file written by `story-time init`; it documents every setting.
pub const DEFAULT_CONFIG: &str = r#"# story-time settings for this vault.

# Words you aim to write each day (used by `story-time today` and `story-time history`).
daily_goal = 500

# Manuscript estimates. A standard manuscript page holds about 250 words,
# and adults read fiction at about 250 words a minute.
words_per_page = 250
reading_wpm = 250

# Where `story-time idea` captures ideas, relative to the vault.
ideas_file = "Scratchpad/Ideas.md"

# Note templates, shared with Obsidian's core Templates plugin.
templates_dir = "Templates"

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
            ideas_file: "Scratchpad/Ideas.md".to_string(),
            templates_dir: "Templates".to_string(),
            author: None,
            spelling: None,
        }
    }
}

impl Config {
    /// Loads the vault's config, or the defaults if it has none.
    pub fn load(vault_root: &Path) -> Result<Config> {
        let path = vault_root.join(META_DIR).join(CONFIG_FILE);
        if !path.exists() {
            return Ok(Config::default());
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
        assert_eq!(parsed, Config::default());
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
