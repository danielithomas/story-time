//! Settings: per-vault in `.story-time/config.toml`, and a per-machine default vault.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// Folder inside the vault for story-time's own files. Obsidian ignores dot-folders.
pub const META_DIR: &str = ".story-time";
pub const CONFIG_FILE: &str = "config.toml";

/// The newest vault format this build understands (docs/bookshelf-format.md). A vault without a
/// `format` setting uses the unversioned layout of story-time 0.1, format 0, which
/// `story-time upgrade` converts.
pub const FORMAT: u32 = 1;

/// Introduces the `format` setting in the config file.
const FORMAT_COMMENT: &str = "# The layout of this bookshelf, so story-time reads it right. `story-time upgrade` changes it.";
/// Documents the `contact` and `paper` settings, which story-time 0.1's config file lacks.
const SUBMISSION_SETTINGS: [&str; 6] = [
    "",
    "# For a manuscript in standard format (`compile --format docx`): the lines under your name on its",
    "# first page, such as your address, phone and email, and its paper, \"a4\" or \"letter\". The paper",
    "# defaults to letter in the United States and Canada and A4 elsewhere.",
    "# contact = [\"12 Example Street, Sydney NSW 2000\", \"you@example.com\"]",
    "# paper = \"a4\"",
];

/// Introduces the `scratchpad` setting in the config file.
const SCRATCHPAD_COMMENT: &str =
    "# The bookshelf's scratchpad: fragments not tied to any world (`story-time scrap add`).";
/// Comments in story-time 0.1's config file that have been reworded since.
const REWORDED: [(&str, &str); 2] = [
    (
        "# story-time settings for this vault.",
        "# story-time settings for this bookshelf.",
    ),
    (
        "# Where `story-time idea` captures ideas, relative to the vault.",
        "# Where `story-time idea` captures ideas, relative to the bookshelf.",
    ),
];

/// The config file written by `story-time init`; it documents every setting.
pub const DEFAULT_CONFIG: &str = r#"# story-time settings for this bookshelf.

# The layout of this bookshelf, so story-time reads it right. `story-time upgrade` changes it.
format = 1

# Words you aim to write each day (used by `story-time today` and `story-time history`).
daily_goal = 500

# Manuscript estimates. A standard manuscript page holds about 250 words,
# and adults read fiction at about 250 words a minute.
words_per_page = 250
reading_wpm = 250

# Where `story-time idea` captures ideas, relative to the bookshelf.
ideas_file = "scratchpad/ideas.md"

# The bookshelf's scratchpad: fragments not tied to any world (`story-time scrap add`).
scratchpad = "scratchpad"

# Note templates, shared with Obsidian's core Templates plugin.
templates_dir = "templates"

# Name printed on compiled manuscripts.
# author = "Your Name"

# For a manuscript in standard format (`compile --format docx`): the lines under your name on its
# first page, such as your address, phone and email, and its paper, "a4" or "letter". The paper
# defaults to letter in the United States and Canada and A4 elsewhere.
# contact = ["12 Example Street, Sydney NSW 2000", "you@example.com"]
# paper = "a4"

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
    /// Lines under the author's name on a submission's first page: address, phone, email.
    pub contact: Vec<String>,
    /// "a4" or "letter" for a submission; unset, it follows the spelling languages or locale.
    pub paper: Option<String>,
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
            contact: Vec::new(),
            paper: None,
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
                "this bookshelf uses format {}, which is newer than this version of story-time \
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
                "this bookshelf is laid out the way story-time 0.1 did it. Run `story-time upgrade` \
                 to convert it; it backs the bookshelf up first."
            );
        }
        Ok(())
    }
}

/// A format-0 config file upgraded to format 1: `format` added before the first setting, the ideas
/// file and templates pointed at their new folders, and the scratchpad set after the ideas file.
/// Other settings and the writer's comments stay as written.
pub fn upgraded_config(text: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    for line in &mut lines {
        if let Some((_, new)) = REWORDED.iter().find(|(old, _)| line == old) {
            *line = new.to_string();
        }
    }
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
    let mentions = |key: &str| {
        lines.iter().any(|line| {
            let line = line.trim_start_matches(['#', ' ']);
            line.split_once('=').is_some_and(|(k, _)| k.trim() == key)
        })
    };
    if !mentions("contact") && !mentions("paper") {
        let author = lines.iter().position(|line| {
            let line = line.trim_start_matches(['#', ' ']);
            line.split_once('=')
                .is_some_and(|(key, _)| key.trim() == "author")
        });
        let at = author.map_or(lines.len(), |i| i + 1);
        lines.splice(at..at, SUBMISSION_SETTINGS.map(String::from));
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

/// Per-machine settings: the default bookshelf, so commands find it from any directory (for
/// example one kept on OneDrive), and how this computer backs bookshelves up. Before 0.3 the
/// bookshelf's key was `vault`, which still works.
#[derive(Debug, Default, Deserialize)]
pub struct UserConfig {
    #[serde(alias = "vault")]
    bookshelf: Option<String>,
    /// Whether the app backs up the open bookshelf by itself. On unless set to false.
    backup: Option<bool>,
    /// Where backups go, instead of Story Time's data folder.
    backup_folder: Option<String>,
}

impl UserConfig {
    /// The user config file's settings, or the defaults if there is no file.
    pub fn load() -> Result<UserConfig> {
        let Some(path) = user_config_path().filter(|path| path.is_file()) else {
            return Ok(UserConfig::default());
        };
        let text =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn bookshelf(&self) -> Option<PathBuf> {
        self.bookshelf.as_deref().map(expand_home)
    }

    pub fn backs_up(&self) -> bool {
        self.backup.unwrap_or(true)
    }

    pub fn backup_folder(&self) -> Option<PathBuf> {
        self.backup_folder.as_deref().map(expand_home)
    }
}

/// `~/.config/story-time/config.toml` on Linux; the platform equivalent elsewhere.
pub fn user_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("story-time").join(CONFIG_FILE))
}

/// The `bookshelf` set in the user config file, if there is one.
pub fn user_default_vault() -> Result<Option<PathBuf>> {
    Ok(UserConfig::load()?.bookshelf())
}

/// Remembers `vault` as this machine's default bookshelf.
pub fn set_user_default_vault(vault: &Path) -> Result<()> {
    set_user_setting(
        "bookshelf",
        Some(toml::Value::String(vault.display().to_string())),
    )
}

/// Sets `key` in the user config file, or with `None` removes it, keeping everything else.
pub fn set_user_setting(key: &str, value: Option<toml::Value>) -> Result<()> {
    let path = user_config_path().context("this system has no config folder")?;
    let existing = fs::read_to_string(&path).unwrap_or_default();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(&path, with_setting(&existing, key, value.as_ref()))
        .with_context(|| format!("writing {}", path.display()))
}

/// Sets `key` in a bookshelf's config file, or with `None` unsets it, keeping everything else.
pub fn set_bookshelf_setting(root: &Path, key: &str, value: Option<toml::Value>) -> Result<()> {
    let path = root.join(META_DIR).join(CONFIG_FILE);
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    fs::write(&path, with_setting(&text, key, value.as_ref()))
        .with_context(|| format!("writing {}", path.display()))
}

/// A config file's `text` with `key` set to `value`, keeping its comments and other settings. The
/// setting goes where it already is, else on a commented-out line for it (as the default file has
/// `# author = "Your Name"`), else at the top. With `None` it is commented out. Setting
/// `bookshelf` also replaces the old `vault` key.
fn with_setting(text: &str, key: &str, value: Option<&toml::Value>) -> String {
    let is_key = |found: &str| found == key || (key == "bookshelf" && found == "vault");
    let sets = |line: &str| setting(line.trim_start()).is_some_and(is_key);
    let suggests = |line: &str| {
        line.strip_prefix('#')
            .and_then(|rest| setting(rest.trim_start()))
            .is_some_and(is_key)
    };
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    match value {
        Some(value) => {
            let line = format!("{key} = {value}");
            let at = lines.iter().position(|l| sets(l));
            match at.or_else(|| lines.iter().position(|l| suggests(l))) {
                Some(i) => lines[i] = line,
                // A new key goes after the others, before any [table] so it stays top-level.
                None => {
                    let table = lines.iter().position(|l| l.trim_start().starts_with('['));
                    let end = table.unwrap_or(lines.len());
                    let last = lines[..end].iter().rposition(|l| !l.trim().is_empty());
                    let at = last.map_or(0, |i| i + 1);
                    let mut new = vec![line];
                    if at > 0 {
                        new.insert(0, String::new());
                    }
                    if lines.get(at).is_some_and(|next| !next.trim().is_empty()) {
                        new.push(String::new());
                    }
                    lines.splice(at..at, new);
                }
            }
            // Any other line setting it, such as an old `vault`, goes.
            let mut first = true;
            lines.retain(|l| !sets(l) || std::mem::take(&mut first));
        }
        None => {
            for line in lines.iter_mut().filter(|l| sets(l)) {
                *line = format!("# {line}");
            }
        }
    }
    lines.join("\n") + "\n"
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
    fn edits_the_user_config_in_place() {
        let text = "# mine\nvault = \"/old\"\nbackup = true\n";
        let shelf = toml::Value::String("/stories".into());
        assert_eq!(
            with_setting(text, "bookshelf", Some(&shelf)),
            "# mine\nbookshelf = \"/stories\"\nbackup = true\n"
        );
        assert_eq!(
            with_setting(text, "backup", Some(&toml::Value::Boolean(false))),
            "# mine\nvault = \"/old\"\nbackup = false\n"
        );
        assert_eq!(
            with_setting(text, "backup_folder", Some(&shelf)),
            "# mine\nvault = \"/old\"\nbackup = true\n\nbackup_folder = \"/stories\"\n"
        );
        assert_eq!(
            with_setting(
                "a = 1\n\n[table]\nb = 2\n",
                "c",
                Some(&toml::Value::Integer(3))
            ),
            "a = 1\n\nc = 3\n\n[table]\nb = 2\n",
            "before any table, so it stays top-level"
        );
        assert_eq!(
            with_setting(text, "backup", None),
            "# mine\nvault = \"/old\"\n# backup = true\n"
        );
        assert_eq!(with_setting("", "backup", None), "\n");
    }

    #[test]
    fn edits_a_bookshelf_config_keeping_its_comments() {
        let name = toml::Value::String("A. Writer".into());
        let named = with_setting(DEFAULT_CONFIG, "author", Some(&name));
        assert!(
            named.contains("# Name printed on compiled manuscripts.\nauthor = \"A. Writer\"\n"),
            "{named}"
        );
        let goal = with_setting(&named, "daily_goal", Some(&toml::Value::Integer(800)));
        let parsed: Config = toml::from_str(&goal).unwrap();
        assert_eq!(
            (parsed.daily_goal, parsed.author.as_deref()),
            (800, Some("A. Writer"))
        );
        let unnamed = with_setting(&goal, "author", None);
        assert!(
            unnamed.contains("\n# author = \"A. Writer\"\n"),
            "{unnamed}"
        );
        assert_eq!(toml::from_str::<Config>(&unnamed).unwrap().author, None);
        assert_eq!(
            with_setting(&unnamed, "author", Some(&name)),
            goal,
            "set again where it was"
        );
    }

    #[test]
    fn reads_the_default_bookshelf_by_its_old_name_too() {
        for text in ["bookshelf = \"/stories\"", "vault = \"/stories\""] {
            let config: UserConfig = toml::from_str(text).unwrap();
            assert_eq!(config.bookshelf.as_deref(), Some("/stories"), "{text}");
        }
    }

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
