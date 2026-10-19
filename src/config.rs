//! The config file (TOML).
//!
//! Option names and defaults follow fzfkasten.nvim's `setup()`. To use one
//! collection from both tools, give both the same values. config.example.toml
//! shows every option (`ttykasten config-example` prints it).
//!
//! A setting ttykasten does not know is reported and ignored rather than
//! refused: the config file may come from a newer ttykasten (synced with
//! dotfiles, say) than the binary reading it, and a typo still shows up.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::i18n::Language;

pub const EXAMPLE: &str = include_str!("../config.example.toml");

#[derive(Debug, Deserialize)]
#[serde(default)]
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
    /// The editor argument that opens a file at a line; `{line}` is replaced.
    /// Empty: files open at the top.
    pub editor_line: String,
    /// Neovim's swap directory, read before every write so that a note open
    /// in Neovim with unsaved changes is not written under it. Unset: Neovim's
    /// default ($XDG_STATE_HOME/nvim/swap, else ~/.local/state/nvim/swap).
    /// Empty: no check.
    pub nvim_swap_dir: Option<String>,
    /// The command that moves a deleted note to the trash; the file is the
    /// argument. Unset: `gio trash`, else `trash`. "" leaves nothing to
    /// delete with, so deleting is refused.
    pub trash: Option<String>,
    /// Where a link goes: "osc52" (the terminal's clipboard, also over ssh and
    /// mosh), "none", or a command that reads the link on stdin (e.g. "wl-copy").
    pub clipboard: String,
    pub notes: Notes,
    pub follow_link: FollowLink,
    pub tasks: Tasks,
    pub block_id: BlockId,
    pub log: Log,
    pub fzf: Fzf,
    pub menu: Menu,
    pub preview: Preview,
    pub calendar: Calendar,
    pub week: Week,
    pub romaji: Romaji,
    pub graph: Graph,
    pub recent: Recent,
    /// Ask which template a new note uses (the default is preselected; Esc
    /// takes `new_note_template`), as the plugin does.
    pub pick_template: bool,
    /// The pager for `digest` (gets coloured text); "" prints to stdout.
    pub pager: String,
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
            editor_line: "+{line}".into(),
            nvim_swap_dir: None,
            trash: None,
            clipboard: "osc52".into(),
            notes: Notes::default(),
            follow_link: FollowLink::default(),
            tasks: Tasks::default(),
            block_id: BlockId::default(),
            log: Log::default(),
            fzf: Fzf::default(),
            menu: Menu::default(),
            preview: Preview::default(),
            calendar: Calendar::default(),
            week: Week::default(),
            romaji: Romaji::default(),
            graph: Graph::default(),
            recent: Recent::default(),
            pick_template: true,
            pager: "less -R".into(),
        }
    }
}

/// Following a link from an editor (`link-at`).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct FollowLink {
    /// Make the note when a link names one that does not exist, in home.
    pub create_nonexisting: bool,
    /// Its template; unset: new_note_template.
    pub new_note_template: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Notes {
    pub daily: Daily,
    pub weekly: Weekly,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
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
#[serde(default)]
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
#[serde(default)]
pub struct Tasks {
    /// Tag added to captured tasks. Unset: none.
    pub require_tag: Option<String>,
    /// Where tasks are captured. Unset: the first entry of `always`.
    pub capture_note: Option<String>,
    pub always: Vec<String>,
    pub new_checkbox: String,
    /// "all": every line outside frontmatter, fences and headings can hold a
    /// task. "headings": only lines under a heading matching `headings`.
    pub scope: String,
    /// Regexes matched against a heading's trimmed, ASCII-lowercased text
    /// (scope = "headings").
    pub headings: Vec<String>,
    pub ignore: TaskIgnore,
    /// Untagged checkboxes in notes dated more than this many days ago are
    /// left out. Unset: no limit. Tagged tasks never age out.
    pub since_days: Option<u32>,
    /// Frontmatter keys read for a note's date, in order.
    pub date_keys: Vec<String>,
    /// Regexes tried on a note's first `date_lines` lines to date it, before
    /// the file name and `date_keys` (the plugin's `tasks.date` hook). The
    /// first YYYY-MM-DD in the match (or in its first group) is the date.
    pub date_patterns: Vec<String>,
    pub date_lines: usize,
    /// Order of the task list: "priority", "due" or "added".
    pub sort: String,
    /// fzf keys for the actions in the task list and inbox.
    pub keys: TaskKeys,
}

/// Keys in the task list (fzf key names). The defaults are the plugin's
/// picker keys; `due` has none there, so it takes alt-l.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct TaskKeys {
    pub done: String,
    pub cancel: String,
    pub tag: String,
    pub due: String,
    pub capture: String,
    pub undo: String,
    pub sort: String,
    pub reverse: String,
    pub closed: String,
    pub filter_tag: String,
    pub filter_due: String,
    pub group: String,
}

impl Default for TaskKeys {
    fn default() -> Self {
        TaskKeys {
            done: "ctrl-x".into(),
            cancel: "ctrl-d".into(),
            tag: "ctrl-t".into(),
            due: "alt-l".into(),
            capture: "alt-a".into(),
            undo: "alt-u".into(),
            sort: "alt-s".into(),
            reverse: "alt-r".into(),
            closed: "alt-x".into(),
            filter_tag: "alt-t".into(),
            filter_due: "alt-e".into(),
            group: "alt-g".into(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct TaskIgnore {
    /// A frontmatter key whose value false / no / off opts a note out.
    pub frontmatter_key: Option<String>,
    /// Directories whose notes are never scanned.
    pub dirs: Vec<String>,
}

impl Default for TaskIgnore {
    fn default() -> Self {
        TaskIgnore {
            frontmatter_key: Some("tasks".into()),
            dirs: vec!["templates".into()],
        }
    }
}

impl Default for Tasks {
    fn default() -> Self {
        Tasks {
            require_tag: None,
            capture_note: None,
            always: Vec::new(),
            new_checkbox: "- [ ] ".into(),
            scope: "all".into(),
            headings: vec![
                r"^tasks?([^A-Za-z]|$)".into(),
                r"^to-?dos?([^A-Za-z]|$)".into(),
                "^タスク".into(),
                "^やること".into(),
            ],
            ignore: TaskIgnore::default(),
            since_days: None,
            date_keys: vec!["date".into(), "created".into()],
            date_patterns: Vec::new(),
            date_lines: 10,
            sort: "priority".into(),
            keys: TaskKeys::default(),
        }
    }
}

impl Tasks {
    /// The capture note, relative to home.
    pub fn capture_target(&self) -> Option<&str> {
        self.capture_note.as_deref().or(self.always.first().map(String::as_str))
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct BlockId {
    pub length: usize,
    pub alphabet: String,
    /// Whether captured tasks get an id.
    pub on_capture: bool,
    /// Whether `tag` (TaskTag) gives a line it turns into a task an id.
    pub on_tag: bool,
    /// Written in front of the link that `link-daily` puts into the daily note.
    pub daily_bullet: String,
    /// Carry the line's text into the link as an alias: `[[note#^id|text]]`.
    pub alias: bool,
    /// Cut the alias to this many characters; 0 keeps it whole.
    pub alias_max: usize,
}

impl Default for BlockId {
    fn default() -> Self {
        BlockId {
            length: 6,
            alphabet: "abcdefghijklmnopqrstuvwxyz0123456789".into(),
            on_capture: false,
            on_tag: false,
            daily_bullet: "- ".into(),
            alias: false,
            alias_max: 40,
        }
    }
}

/// Labels in the list of daily and weekly notes (`log`).
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Log {
    pub day_label: String,
    pub week_label: String,
    /// The key that asks for a date to open.
    pub date_key: String,
}

impl Default for Log {
    fn default() -> Self {
        Log {
            day_label: "%Y-%m-%d (%a)".into(),
            week_label: "%G-W%V".into(),
            date_key: "ctrl-x".into(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
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

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Menu {
    /// Items to list. Unset: every action in the default order.
    pub items: Option<Vec<MenuItem>>,
    /// After `items`, every action they leave out, in the default order, so
    /// an action added to ttykasten shows up without editing the config.
    pub rest: bool,
    /// The menu waits for a key: an item's key, less the start every key
    /// shares (`d` for `kd` when all keys start with `k`), runs it at once.
    /// Typing to search starts after one of `search_keys`.
    pub hotkeys: bool,
    pub search_keys: Vec<String>,
}

impl Default for Menu {
    fn default() -> Self {
        Menu {
            items: None,
            rest: false,
            hotkeys: true,
            search_keys: vec!["space".into(), "/".into()],
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct MenuItem {
    /// A ttykasten action (daily, weekly, capture, find, new, log, link,
    /// link-daily, backlinks).
    pub action: Option<String>,
    /// A command to run instead, with `sh -c`, in home.
    pub command: Option<String>,
    /// A key hint shown in the list (e.g. kd). `ttykasten kd` then runs the item.
    pub key: Option<String>,
    /// The description. Unset: the language's default.
    pub label: Option<String>,
}

impl MenuItem {
    /// An item for an action, with no key and the default label.
    pub fn of(action: &str) -> MenuItem {
        MenuItem {
            action: Some(action.to_string()),
            command: None,
            key: None,
            label: None,
        }
    }
}

pub const ACTIONS: &[&str] = &[
    "daily",
    "weekly",
    "capture",
    "tasks",
    "inbox",
    "capture-guided",
    "tag",
    "mint-ids",
    "undo",
    "week",
    "digest",
    "agenda",
    "grep",
    "tags",
    "recent",
    "rename",
    "note",
    "links",
    "orphans",
    "dead-links",
    "hubs",
    "find",
    "new",
    "log",
    "link",
    "insert-link",
    "link-daily",
    "backlinks",
];

impl Menu {
    pub fn items(&self) -> Vec<MenuItem> {
        match &self.items {
            Some(items) if self.rest => {
                let mut all = items.clone();
                for a in ACTIONS {
                    if !items.iter().any(|i| i.action.as_deref() == Some(*a)) {
                        all.push(MenuItem::of(a));
                    }
                }
                all
            }
            Some(items) => items.clone(),
            None => ACTIONS.iter().map(|a| MenuItem::of(a)).collect(),
        }
    }
}

/// Romaji search through `ttyskk migemo`: a query starting with `/` in
/// `find` and `grep` is turned into a regex (`/kaigi` finds 会議).
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Romaji {
    pub enabled: bool,
    pub command: String,
    pub limit: Option<u32>,
    /// Milliseconds to wait for the command.
    pub timeout: u64,
    /// Match a note's headings as well as its path in `find`.
    pub headings: bool,
    /// A query starting with this character is romaji. "" turns it off.
    pub prefix: String,
}

impl Default for Romaji {
    fn default() -> Self {
        Romaji {
            enabled: true,
            command: "ttyskk".into(),
            limit: None,
            timeout: 5000,
            headings: true,
            prefix: "/".into(),
        }
    }
}

/// The link graph (`links`, `orphans`, `dead-links`, `hubs`).
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Graph {
    /// How many links deep `links` goes.
    pub depth: usize,
    pub ignore_dirs: Vec<String>,
}

impl Default for Graph {
    fn default() -> Self {
        Graph {
            depth: 2,
            ignore_dirs: vec!["templates".into()],
        }
    }
}

/// `recent`: notes by modification time.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Recent {
    pub limit: usize,
    pub ignore_dirs: Vec<String>,
}

impl Default for Recent {
    fn default() -> Self {
        Recent {
            limit: 50,
            ignore_dirs: vec!["templates".into()],
        }
    }
}

/// The calendar (format.md §12), read through gcalcli or a command that
/// prints the same TSV.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Calendar {
    pub enabled: bool,
    /// The calendar(s) to read (`--calendar`); a string or a list.
    pub name: Names,
    pub gcalcli: String,
    /// A shell command used instead of gcalcli; `{from}` and `{to}` (to is
    /// exclusive) are replaced. It must print gcalcli's TSV.
    pub command: Option<String>,
    pub bullet: String,
    /// Milliseconds to wait for the command.
    pub timeout: u64,
    pub cache: CalendarCache,
    pub labels: CalendarLabels,
    pub keys: CalendarKeys,
}

impl Default for Calendar {
    fn default() -> Self {
        Calendar {
            enabled: false,
            name: Names::default(),
            gcalcli: "gcalcli".into(),
            command: None,
            bullet: "- ".into(),
            timeout: 15000,
            cache: CalendarCache::default(),
            labels: CalendarLabels::default(),
            keys: CalendarKeys::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(untagged)]
pub enum Names {
    #[default]
    None,
    One(String),
    Many(Vec<String>),
}

impl Names {
    pub fn names(&self) -> Vec<&str> {
        match self {
            Names::None => Vec::new(),
            Names::One(n) => vec![n.as_str()],
            Names::Many(v) => v.iter().map(String::as_str).collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct CalendarCache {
    /// Seconds a fetched result is reused.
    pub ttl: u64,
    /// Unset: $XDG_CACHE_HOME/ttykasten/calendar.
    pub dir: Option<String>,
}

impl Default for CalendarCache {
    fn default() -> Self {
        CalendarCache { ttl: 900, dir: None }
    }
}

/// Keys in `agenda` (fzf key names; "" unbinds one).
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct CalendarKeys {
    /// Copies the picked events' lines (tab marks several).
    pub copy: String,
    /// Runs the command again instead of reading the cache.
    pub refresh: String,
}

impl Default for CalendarKeys {
    fn default() -> Self {
        CalendarKeys {
            copy: "alt-i".into(),
            refresh: "ctrl-r".into(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct CalendarLabels {
    pub all_day: String,
    pub unavailable: String,
}

impl Default for CalendarLabels {
    fn default() -> Self {
        CalendarLabels {
            all_day: "all day".into(),
            unavailable: "calendar unavailable".into(),
        }
    }
}

/// Week views and the week digest (format.md §11).
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Week {
    pub ignore_dirs: Vec<String>,
    /// Regexes on a note's path relative to home (the plugin's are Lua
    /// patterns: `%.materials%.md$` here is `\.materials\.md$`).
    pub ignore_patterns: Vec<String>,
    /// The key in `week` that opens the week's digest.
    pub digest_key: String,
    pub digest: Digest,
}

impl Default for Week {
    fn default() -> Self {
        Week {
            ignore_dirs: vec!["templates".into()],
            ignore_patterns: Vec::new(),
            digest_key: "ctrl-d".into(),
            digest: Digest::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Digest {
    /// Body lines quoted per note.
    pub lines: usize,
    pub tasks: bool,
    pub marks: DigestMarks,
    pub calendar: bool,
    /// Weeks looked ahead (0: none).
    pub ahead: u32,
    pub sources: Vec<DigestSource>,
    pub labels: DigestLabels,
}

impl Default for Digest {
    fn default() -> Self {
        Digest {
            lines: 8,
            tasks: true,
            marks: DigestMarks::default(),
            calendar: true,
            ahead: 1,
            sources: Vec::new(),
            labels: DigestLabels::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct DigestMarks {
    pub done: String,
    pub open: String,
}

impl Default for DigestMarks {
    fn default() -> Self {
        DigestMarks {
            done: "✓".into(),
            open: "☐".into(),
        }
    }
}

/// An extra section of the digest: the output of a shell command, run in
/// home, with `{from}` `{to}` (the week) and `{ahead_from}` `{ahead_to}`
/// replaced. (The plugin's sources are Lua functions.)
#[derive(Debug, Clone, Deserialize)]
pub struct DigestSource {
    pub label: String,
    pub command: String,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct DigestLabels {
    /// `{}` is the number of notes.
    pub notes: String,
    pub calendar: String,
    pub finished: String,
    pub open: String,
    pub ahead: String,
    pub due: String,
    pub unavailable: String,
}

impl Default for DigestLabels {
    fn default() -> Self {
        DigestLabels {
            notes: "{} notes".into(),
            calendar: "Calendar".into(),
            finished: "Finished this week".into(),
            open: "Still open in this week's notes".into(),
            ahead: "Coming up".into(),
            due: "Due".into(),
            unavailable: "unavailable".into(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Preview {
    pub colors: Colors,
}

/// Colours are SGR parameters (`1;34` is bold blue). An empty string means no colour.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
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
    /// The line picked in a list of lines.
    pub mark: String,
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
            mark: "7".into(),
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

/// Loads the config. A missing file gives the defaults. Unknown settings are
/// reported on stderr, in the config's language, and ignored.
pub fn load() -> Result<Config> {
    let Some(path) = path() else {
        return Ok(Config::default());
    };
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(e) => return Err(e).with_context(|| format!("{}", path.display())),
    };
    let (cfg, unknown) = parse(&text).with_context(|| format!("{}", path.display()))?;
    let msg = cfg.language.msgs();
    for key in unknown {
        eprintln!(
            "ttykasten: {}",
            crate::i18n::fill(msg.unknown_setting, &[&path.display(), &key])
        );
    }
    Ok(cfg)
}

/// Parses a config, returning the settings it has that ttykasten does not
/// know (as dotted paths, e.g. `tasks.date_pattern`).
pub fn parse(text: &str) -> Result<(Config, Vec<String>)> {
    let de = toml::Deserializer::parse(text)?;
    let mut unknown = Vec::new();
    let cfg = serde_ignored::deserialize(de, |path| {
        // serde_ignored marks the inside of an Option with `?`; drop it.
        let dotted = path.to_string();
        unknown.push(dotted.split('.').filter(|s| *s != "?").collect::<Vec<_>>().join("."));
    })?;
    Ok((cfg, unknown))
}

impl Config {
    /// Where the notes live. $TTYKASTEN_HOME overrides everything (for trying things out).
    pub fn home(&self) -> Result<PathBuf> {
        self.home_from().0
    }

    /// The home, and the environment variable it came from, if not `home` (the
    /// doctor says so: a variable left set in one shell is easily forgotten).
    pub fn home_from(&self) -> (Result<PathBuf>, Option<&'static str>) {
        if let Some(h) = env::var_os("TTYKASTEN_HOME").filter(|h| !h.is_empty()) {
            return (Ok(PathBuf::from(h)), Some("TTYKASTEN_HOME"));
        }
        if let Some(h) = &self.home {
            return (expand_tilde(h), None);
        }
        if let Some(h) = env::var_os("ZETTELKASTEN_HOME").filter(|h| !h.is_empty()) {
            return (Ok(PathBuf::from(h)), Some("ZETTELKASTEN_HOME"));
        }
        (expand_tilde("~/notes"), None)
    }

    /// The swap directory to check, or None when checking is off.
    pub fn swap_dir(&self) -> Option<PathBuf> {
        match self.nvim_swap_dir.as_deref() {
            Some("") => None,
            Some(dir) => expand_tilde(dir).ok(),
            None => env::var_os("XDG_STATE_HOME")
                .filter(|p| !p.is_empty())
                .map(PathBuf::from)
                .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
                .map(|base| base.join("nvim").join("swap")),
        }
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

/// `~/x` as a path under $HOME; anything else as it is.
pub fn expand_tilde(p: &str) -> Result<PathBuf> {
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
        let (c, unknown) = parse(EXAMPLE).unwrap();
        assert!(unknown.is_empty(), "{unknown:?}");
        assert_eq!(c.notes.daily.dir, "daily");
    }

    #[test]
    fn empty_is_plugin_defaults() {
        let (c, _) = parse("").unwrap();
        assert_eq!(c.language, Language::En);
        assert_eq!(c.hdate_format, "%B %d, %Y");
        assert_eq!(c.notes.weekly.format, "%G-W%V");
        assert_eq!(c.tasks.capture_target(), None);
        assert!(!c.block_id.on_capture);
    }

    #[test]
    fn partial_tables_keep_defaults() {
        let (c, _) = parse(
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
    fn unknown_settings_are_reported_not_refused() {
        let (c, unknown) = parse("languge = \"ja\"\n[notes.daily]\ndri = \"x\"\ndir = \"j\"").unwrap();
        assert_eq!(unknown, vec!["languge", "notes.daily.dri"]);
        assert_eq!(c.notes.daily.dir, "j");
        let (_, unknown) = parse("[[menu.items]]\naction = \"daily\"\nhotkey = \"x\"").unwrap();
        assert_eq!(unknown, vec!["menu.items.0.hotkey"]);
        // A wrong value is still an error.
        assert!(parse("language = \"fr\"").is_err());
        assert!(parse("[block_id]\nlength = \"six\"").is_err());
    }

    #[test]
    fn menu_rest_follows_the_items() {
        let (cfg, _) = parse("[menu]\nrest = true\n[[menu.items]]\naction = \"tasks\"\nkey = \"kt\"\n").unwrap();
        let items = cfg.menu.items();
        assert_eq!(items[0].key.as_deref(), Some("kt"));
        assert_eq!(items.len(), ACTIONS.len());
        assert_eq!(items.iter().filter(|i| i.action.as_deref() == Some("tasks")).count(), 1);
        let (cfg, _) = parse("[[menu.items]]\naction = \"tasks\"\n").unwrap();
        assert_eq!(cfg.menu.items().len(), 1);
    }
}
