//! The config file (TOML).
//!
//! Option names and defaults follow fzfkasten.nvim's `setup()`. To use one
//! collection from both tools, give both the same values. config.example.toml
//! shows every option (`ttykasten config-example` prints it).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::i18n::Language;

pub const EXAMPLE: &str = include_str!("../config.example.toml");

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Language of messages. Does not affect date names written into notes (`%A` etc.).
    pub language: Language,
    /// Where the notes live. Unset: $ZETTELKASTEN_HOME, else ~/notes.
    pub home: Option<String>,
    pub extension: String,
    pub hdate_format: String,
    pub new_note_template: Option<String>,
    /// Editor that opens notes. Unset: $EDITOR, else nvim.
    pub editor: Option<String>,
    pub notes: Notes,
    pub tasks: Tasks,
    pub block_id: BlockId,
    pub log: Log,
    pub fzf: Fzf,
    pub menu: Menu,
    pub preview: Preview,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            language: Language::En,
            home: None,
            extension: "md".into(),
            hdate_format: "%B %d, %Y".into(),
            new_note_template: None,
            editor: None,
            notes: Notes::default(),
            tasks: Tasks::default(),
            block_id: BlockId::default(),
            log: Log::default(),
            fzf: Fzf::default(),
            menu: Menu::default(),
            preview: Preview::default(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Notes {
    pub daily: Daily,
    pub weekly: Weekly,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Daily {
    pub dir: String,
    pub format: String,
    pub template: String,
    pub lookback_days: u32,
}

impl Default for Daily {
    fn default() -> Self {
        Daily {
            dir: "daily".into(),
            format: "%Y-%m-%d".into(),
            template: "daily.md".into(),
            lookback_days: 30,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Weekly {
    pub dir: String,
    pub format: String,
    pub template: String,
    pub lookback_weeks: u32,
}

impl Default for Weekly {
    fn default() -> Self {
        Weekly {
            dir: "weekly".into(),
            format: "%G-W%V".into(),
            template: "weekly.md".into(),
            lookback_weeks: 8,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Tasks {
    /// Tag added to captured tasks. Unset: none.
    pub require_tag: Option<String>,
    /// Where tasks are captured. Unset: the first entry of `always`.
    pub capture_note: Option<String>,
    pub always: Vec<String>,
    pub new_checkbox: String,
}

impl Default for Tasks {
    fn default() -> Self {
        Tasks {
            require_tag: None,
            capture_note: None,
            always: Vec::new(),
            new_checkbox: "- [ ] ".into(),
        }
    }
}

impl Tasks {
    /// The capture note, relative to home.
    pub fn capture_target(&self) -> Option<&str> {
        self.capture_note
            .as_deref()
            .or(self.always.first().map(String::as_str))
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BlockId {
    pub length: usize,
    pub alphabet: String,
    /// Whether captured tasks get an id.
    pub on_capture: bool,
}

impl Default for BlockId {
    fn default() -> Self {
        BlockId {
            length: 6,
            alphabet: "abcdefghijklmnopqrstuvwxyz0123456789".into(),
            on_capture: false,
        }
    }
}

/// Labels in the list of daily and weekly notes (`log`).
#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Log {
    pub day_label: String,
    pub week_label: String,
}

impl Default for Log {
    fn default() -> Self {
        Log {
            day_label: "%Y-%m-%d (%a)".into(),
            week_label: "%G-W%V".into(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Fzf {
    /// The fzf executable.
    pub command: String,
    /// Extra arguments for every fzf. They come after ttykasten's own, so they
    /// win when both set the same option.
    pub opts: Vec<String>,
    /// Whether to show the note next to the list.
    pub preview: bool,
}

impl Default for Fzf {
    fn default() -> Self {
        Fzf {
            command: "fzf".into(),
            opts: Vec::new(),
            preview: true,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Menu {
    /// Items to list. Unset: every action in the default order.
    pub items: Option<Vec<MenuItem>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MenuItem {
    /// A ttykasten action (daily, weekly, capture, find, new, log).
    pub action: Option<String>,
    /// A command to run instead, with `sh -c`, in home.
    pub command: Option<String>,
    /// A key hint shown in the list (e.g. kd). `ttykasten kd` then runs the item.
    pub key: Option<String>,
    /// The description. Unset: the language's default.
    pub label: Option<String>,
}

pub const ACTIONS: &[&str] = &["daily", "weekly", "capture", "find", "new", "log"];

impl Menu {
    pub fn items(&self) -> Vec<MenuItem> {
        match &self.items {
            Some(items) => items.clone(),
            None => ACTIONS
                .iter()
                .map(|a| MenuItem {
                    action: Some(a.to_string()),
                    command: None,
                    key: None,
                    label: None,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Preview {
    pub colors: Colors,
}

/// Colours are SGR parameters (`1;34` is bold blue). An empty string means no colour.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Colors {
    pub heading: String,
    pub tag: String,
    pub link: String,
    pub open: String,
    pub done: String,
    pub priority: String,
    pub due: String,
    pub dim: String,
    pub cancelled: String,
}

impl Default for Colors {
    fn default() -> Self {
        Colors {
            heading: "1;34".into(),
            tag: "36".into(),
            link: "4;34".into(),
            open: "33".into(),
            done: "32".into(),
            priority: "1;33".into(),
            due: "31".into(),
            dim: "2".into(),
            cancelled: "2;9".into(),
        }
    }
}

/// Where the config file is: $TTYKASTEN_CONFIG, then
/// $XDG_CONFIG_HOME/ttykasten/config.toml, then ~/.config/ttykasten/config.toml.
pub fn path() -> Option<PathBuf> {
    if let Some(p) = env::var_os("TTYKASTEN_CONFIG").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    let base = env::var_os("XDG_CONFIG_HOME")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("ttykasten").join("config.toml"))
}

/// Loads the config. A missing file gives the defaults.
pub fn load() -> Result<Config> {
    let Some(path) = path() else {
        return Ok(Config::default());
    };
    match fs::read_to_string(&path) {
        Ok(text) => parse(&text).with_context(|| format!("{}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(e).with_context(|| format!("{}", path.display())),
    }
}

pub fn parse(text: &str) -> Result<Config> {
    Ok(toml::from_str(text)?)
}

impl Config {
    /// Where the notes live. $TTYKASTEN_HOME overrides everything (for trying things out).
    pub fn home(&self) -> Result<PathBuf> {
        if let Some(h) = env::var_os("TTYKASTEN_HOME").filter(|h| !h.is_empty()) {
            return Ok(PathBuf::from(h));
        }
        if let Some(h) = &self.home {
            return expand_tilde(h);
        }
        if let Some(h) = env::var_os("ZETTELKASTEN_HOME").filter(|h| !h.is_empty()) {
            return Ok(PathBuf::from(h));
        }
        expand_tilde("~/notes")
    }

    /// The editor command.
    pub fn editor(&self) -> String {
        self.editor
            .clone()
            .or_else(|| env::var("EDITOR").ok())
            .filter(|e| !e.trim().is_empty())
            .unwrap_or_else(|| "nvim".into())
    }
}

fn expand_tilde(p: &str) -> Result<PathBuf> {
    match p.strip_prefix("~/") {
        Some(rest) => {
            let home = env::var_os("HOME").context("HOME")?;
            Ok(Path::new(&home).join(rest))
        }
        None if p == "~" => Ok(PathBuf::from(env::var_os("HOME").context("HOME")?)),
        None => Ok(PathBuf::from(p)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_parses() {
        let c = parse(EXAMPLE).unwrap();
        assert_eq!(c.notes.daily.dir, "daily");
    }

    #[test]
    fn empty_is_plugin_defaults() {
        let c = parse("").unwrap();
        assert_eq!(c.language, Language::En);
        assert_eq!(c.hdate_format, "%B %d, %Y");
        assert_eq!(c.notes.weekly.format, "%G-W%V");
        assert_eq!(c.tasks.capture_target(), None);
        assert!(!c.block_id.on_capture);
    }

    #[test]
    fn partial_tables_keep_defaults() {
        let c = parse(
            r#"
            language = "ja"
            [notes.daily]
            dir = "lognote"
            [tasks]
            always = ["tasks/active.md"]
            [[menu.items]]
            action = "daily"
            key = "kd"
            "#,
        )
        .unwrap();
        assert_eq!(c.language, Language::Ja);
        assert_eq!(c.notes.daily.dir, "lognote");
        assert_eq!(c.notes.daily.format, "%Y-%m-%d");
        assert_eq!(c.tasks.capture_target(), Some("tasks/active.md"));
        assert_eq!(c.menu.items().len(), 1);
    }

    #[test]
    fn typos_are_errors() {
        assert!(parse("languge = \"ja\"").is_err());
        assert!(parse("language = \"fr\"").is_err());
        assert!(parse("[notes.daily]\ndri = \"x\"").is_err());
    }
}
