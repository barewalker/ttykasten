//! `ttykasten doctor`: what is missing or wrong in the setup, and what it
//! costs (the plugin's `:checkhealth fzfkasten`).
//!
//! What ttykasten needs is mostly outside it: a notes directory, fzf and rg on
//! PATH, templates at the paths the config names, gcalcli for the calendar.
//! When one of those is missing the symptom is a list that comes up empty or
//! an action that fails, which does not say which. This says.
//!
//! The report is read once and then not again, so its sentences live here, in
//! both languages, instead of in the message tables.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::command::on_path;
use crate::config::{self, ACTIONS, Config};
use crate::i18n::Language;
use crate::tasks::Scanner;

/// The oldest fzf with everything the lists use: `transform`, `$FZF_PROMPT`
/// for the keys' commands, `change-header` from a transform.
const FZF_MIN: (u32, u32) = (0, 46);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    Info,
    Warn,
    Error,
}

/// One finding, with lines of advice under it.
#[derive(Debug, Clone)]
pub struct Finding {
    pub level: Level,
    pub text: String,
    pub advice: Vec<String>,
}

/// The findings, under section headings.
#[derive(Debug, Default)]
pub struct Report {
    pub sections: Vec<(String, Vec<Finding>)>,
}

impl Report {
    pub fn errors(&self) -> usize {
        self.sections
            .iter()
            .flat_map(|(_, f)| f)
            .filter(|f| f.level == Level::Error)
            .count()
    }
}

struct Doctor {
    lang: Language,
    report: Report,
}

impl Doctor {
    fn say(&self, en: &str, ja: &str) -> String {
        match self.lang {
            Language::En => en.to_string(),
            Language::Ja => ja.to_string(),
        }
    }

    fn section(&mut self, en: &str, ja: &str) {
        let title = self.say(en, ja);
        self.report.sections.push((title, Vec::new()));
    }

    fn add(&mut self, level: Level, text: String, advice: Vec<String>) {
        if let Some((_, f)) = self.report.sections.last_mut() {
            f.push(Finding { level, text, advice });
        }
    }

    fn ok(&mut self, text: String) {
        self.add(Level::Ok, text, Vec::new());
    }

    fn info(&mut self, text: String) {
        self.add(Level::Info, text, Vec::new());
    }
}

/// Runs every check. The config is read here rather than handed in, so a
/// config that does not parse is reported instead of stopping the run.
pub fn check() -> Report {
    let path = config::path();
    let text = path.as_ref().map(fs::read_to_string);
    let mut unknown = Vec::new();
    let mut parse_error = None;
    let cfg = match &text {
        Some(Ok(t)) => match config::parse(t) {
            Ok((c, u)) => {
                unknown = u;
                c
            }
            Err(e) => {
                parse_error = Some(e);
                Config::default()
            }
        },
        _ => Config::default(),
    };
    let mut d = Doctor {
        lang: cfg.language,
        report: Report::default(),
    };

    d.section("Configuration", "設定");
    match (&path, &text, &parse_error) {
        (None, _, _) => d.info(d.say(
            "no config file location ($HOME is unset); the defaults are in use",
            "設定ファイルの場所がわからない ($HOME が未設定)。既定値で動く",
        )),
        (Some(p), Some(Err(e)), _) if e.kind() == std::io::ErrorKind::NotFound => {
            let text = d.say("no config file at ", "設定ファイルがない: ") + &p.display().to_string();
            let advice = vec![d.say(
                "The defaults are in use; `ttykasten config-example` prints every option.",
                "既定値で動いています。`ttykasten config-example` で全項目の見本が出ます。",
            )];
            d.add(Level::Info, text, advice);
        }
        (Some(p), Some(Err(e)), _) => {
            let text = format!("{}: {e}", p.display());
            d.add(Level::Error, text, Vec::new());
        }
        (Some(p), _, Some(e)) => {
            let text = d.say("the config file does not parse: ", "設定ファイルを読めない: ") + &p.display().to_string();
            let advice = vec![
                format!("{e:#}"),
                d.say(
                    "Every command stops at this until it is fixed; the checks below use the defaults.",
                    "直すまで、どのコマンドもここで止まります。以下の点検は既定値で行いました。",
                ),
            ];
            d.add(Level::Error, text, advice);
        }
        (Some(p), _, None) => {
            d.ok(format!("{}", p.display()));
            for key in &unknown {
                let text = d.say("unknown setting ignored: ", "知らない設定 (無視される): ") + key;
                let advice = vec![d.say(
                    "A typo, or a setting newer than this ttykasten.",
                    "書き間違いか、この ttykasten より新しい設定です。",
                )];
                d.add(Level::Warn, text, advice);
            }
        }
    }
    let (home, var) = cfg.home_from();
    let from = var
        .map(|v| d.say(", from $", ", $") + v + &d.say("", " から"))
        .unwrap_or_default();
    match &home {
        Ok(h) if h.is_dir() => {
            let n = count_notes(h, &cfg.extension);
            d.ok(format!("home: {} ({n} .{}{from})", h.display(), cfg.extension));
            if n == 0 {
                d.info(d.say(
                    "No notes there yet, so every list comes up empty.",
                    "まだノートがないので、どの一覧も空になる。",
                ));
            }
        }
        Ok(h) => {
            let text =
                d.say("home is not a directory: ", "home がフォルダではない: ") + &h.display().to_string() + &from;
            let advice = vec![d.say(
                "Nothing is found until it exists. Create it, or set `home`.",
                "作るか、`home` を設定するまで何も見つかりません。",
            )];
            d.add(Level::Error, text, advice);
        }
        Err(e) => {
            let text = format!("home: {e:#}");
            d.add(Level::Error, text, Vec::new());
        }
    }
    let home = home.unwrap_or_default();

    d.section("Programs", "外部のプログラム");
    check_fzf(&mut d, &cfg);
    if on_path("rg") {
        d.ok("rg".into());
    } else {
        let text = d.say("rg (ripgrep) is not on PATH", "rg (ripgrep) が PATH にない");
        let advice = vec![d.say(
            "The task list, grep, tags, link and week all run it, and fail without it.",
            "タスク一覧、grep、tags、link、week がこれを使うので、どれも動きません。",
        )];
        d.add(Level::Error, text, advice);
    }
    let editor = cfg.editor();
    match first_word(&editor) {
        Some(w) if on_path(w) => d.ok(d.say("editor: ", "エディタ: ") + &editor),
        _ => {
            let text = d.say("the editor is not on PATH: ", "エディタが PATH にない: ") + &editor;
            let advice = vec![d.say(
                "Notes cannot be opened. Set `editor`, or $EDITOR.",
                "ノートを開けません。`editor` か $EDITOR を設定してください。",
            )];
            d.add(Level::Error, text, advice);
        }
    }
    if let Some(w) = first_word(&cfg.pager)
        && !on_path(w)
    {
        let text = d.say("the pager is not on PATH: ", "pager が PATH にない: ") + &cfg.pager;
        let advice = vec![d.say(
            "`digest` cannot be shown on a terminal. Set `pager`, or \"\" to print.",
            "`digest` を端末に出せません。`pager` を直すか、\"\" にすると直接表示します。",
        )];
        d.add(Level::Warn, text, advice);
    }
    match cfg.clipboard.as_str() {
        "osc52" => d.ok(d.say(
            "clipboard: the terminal's (OSC 52)",
            "クリップボード: 端末の機能 (OSC 52)",
        )),
        "none" | "" => d.info(d.say(
            "clipboard is \"none\": `link` copies nothing",
            "clipboard が \"none\": `link` は何もコピーしない",
        )),
        c => match first_word(c) {
            Some(w) if on_path(w) => d.ok(d.say("clipboard: ", "クリップボード: ") + c),
            _ => {
                let text = d.say(
                    "the clipboard command is not on PATH: ",
                    "clipboard のコマンドが PATH にない: ",
                ) + c;
                d.add(Level::Warn, text, Vec::new());
            }
        },
    }

    d.section("Templates", "テンプレート");
    let templates = [
        ("new note", "新しいノート", cfg.new_note_template.clone()),
        ("daily", "日誌", Some(cfg.notes.daily.template.clone())),
        ("weekly", "週まとめ", Some(cfg.notes.weekly.template.clone())),
    ];
    for (en, ja, rel) in templates {
        let Some(rel) = rel.filter(|r| !r.is_empty()) else {
            continue;
        };
        let rest = rel.strip_prefix("templates/").unwrap_or(&rel);
        let full = home.join("templates").join(rest);
        if full.is_file() {
            d.ok(format!("{}: {rel}", d.say(en, ja)));
        } else {
            let text = format!("{}: {rel} {}", d.say(en, ja), d.say("is not there", "が見つからない"));
            let advice = vec![
                d.say("Looked for ", "探した場所: ") + &full.display().to_string(),
                d.say(
                    "Notes made from it start as just a heading.",
                    "このテンプレートから作るノートは見出しだけになります。",
                ),
            ];
            d.add(Level::Warn, text, advice);
        }
    }

    d.section("Tasks", "タスク");
    if let Err(e) = Scanner::new(&cfg) {
        let text = d.say(
            "a bad regex in tasks.date_patterns or tasks.headings",
            "tasks.date_patterns か tasks.headings に正しくない正規表現がある",
        );
        let advice = vec![
            e.to_string(),
            d.say(
                "The task list cannot be drawn until it is fixed.",
                "直すまでタスク一覧を出せません。",
            ),
        ];
        d.add(Level::Error, text, advice);
    }
    match &cfg.tasks.require_tag {
        Some(t) => d.ok(d.say("require_tag: #", "require_tag: #") + t),
        None => d.info(d.say(
            "require_tag is unset, so every checkbox is a task and the inbox is empty",
            "require_tag が未設定。すべてのチェックボックスがタスクになり、inbox は空になる",
        )),
    }
    match cfg
        .tasks
        .capture_note
        .clone()
        .or_else(|| cfg.tasks.always.first().cloned())
    {
        Some(n) if home.join(&n).is_file() => d.ok(d.say("captures go to ", "記録先: ") + &n),
        Some(n) => d.ok(d.say("captures go to ", "記録先: ")
            + &n
            + &d.say(
                " (not there yet; the first capture makes it)",
                " (まだ無い。最初の記録で作られる)",
            )),
        None => {
            let text = d.say(
                "no capture note: `capture` has nowhere to write",
                "記録先のノートがない: `capture` が書き込めない",
            );
            let advice = vec![d.say(
                "Set tasks.capture_note, or put a note first in tasks.always.",
                "tasks.capture_note を設定するか、tasks.always の先頭にノートを書いてください。",
            )];
            d.add(Level::Warn, text, advice);
        }
    }
    for rel in &cfg.tasks.always {
        if !home.join(rel).is_file() {
            d.info(d.say("tasks.always: not there yet: ", "tasks.always: まだ無い: ") + rel);
        }
    }

    d.section("Menu", "メニュー");
    let items = cfg.menu.items();
    let mut bad = 0;
    let mut keys: Vec<&str> = Vec::new();
    for item in &items {
        match (&item.action, &item.command) {
            (Some(a), _) if !ACTIONS.contains(&a.as_str()) => {
                bad += 1;
                let text = d.say("unknown action: ", "知らない action: ") + a;
                let advice = vec![d.say("Known: ", "使えるもの: ") + &ACTIONS.join(" ")];
                d.add(Level::Error, text, advice);
            }
            (None, None) => {
                bad += 1;
                let text = d.say(
                    "an item has neither action nor command",
                    "action も command もない項目がある",
                );
                let advice = item.label.iter().cloned().collect();
                d.add(Level::Error, text, advice);
            }
            _ => {}
        }
        if let Some(k) = item.key.as_deref() {
            if keys.contains(&k) {
                let text = d.say("the key is used twice: ", "キーが重なっている: ") + k;
                let advice = vec![d.say(
                    "`ttykasten KEY` runs the first item with it.",
                    "`ttykasten キー` は先に書いた項目を実行します。",
                )];
                d.add(Level::Warn, text, advice);
            }
            keys.push(k);
        }
    }
    if bad == 0 {
        d.ok(format!("{} {}", items.len(), d.say("items", "項目")));
    }

    d.section("Neovim", "Neovim");
    match cfg.swap_dir() {
        None => d.info(d.say(
            "nvim_swap_dir is \"\": notes are written even while Neovim has unsaved changes in them",
            "nvim_swap_dir が \"\": Neovim で未保存のノートにも書き込む",
        )),
        Some(dir) if dir.is_dir() => d.ok(d.say("swap files read from ", "swap の場所: ") + &dir.display().to_string()),
        Some(dir) => d.info(d.say("no swap directory yet at ", "swap の場所がまだ無い: ") + &dir.display().to_string()),
    }

    d.section("Calendar", "カレンダー");
    check_calendar(&mut d, &cfg);

    d.section("Romaji", "ローマ字検索");
    check_romaji(&mut d, &cfg);

    d.report
}

fn check_fzf(d: &mut Doctor, cfg: &Config) {
    let Some(exe) = first_word(&cfg.fzf.command).filter(|w| on_path(w)) else {
        let text = d.say("fzf is not on PATH: ", "fzf が PATH にない: ") + &cfg.fzf.command;
        let advice = vec![d.say(
            "Every list runs it; nothing but `capture` works without it.",
            "どの一覧もこれを使うので、`capture` 以外は動きません。",
        )];
        d.add(Level::Error, text, advice);
        return;
    };
    let out = crate::command::run(&[exe.to_string(), "--version".into()], Duration::from_secs(5));
    let version = out
        .as_deref()
        .ok()
        .and_then(|v| v.split_whitespace().next())
        .unwrap_or("")
        .to_string();
    let mut parts = version.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    let (major, minor) = (parts.next().unwrap_or(0), parts.next().unwrap_or(0));
    if (major, minor) >= FZF_MIN {
        d.ok(format!("fzf {version}"));
    } else {
        let text = format!(
            "fzf {version}: {} {}.{}",
            d.say("too old; ttykasten needs", "古い。必要なのは"),
            FZF_MIN.0,
            FZF_MIN.1
        );
        let advice = vec![d.say(
            "The task list's keys and the romaji search rely on newer actions.",
            "タスク一覧のキーとローマ字検索が新しい機能を使います。",
        )];
        d.add(Level::Error, text, advice);
    }
}

fn check_calendar(d: &mut Doctor, cfg: &Config) {
    let cal = &cfg.calendar;
    if !cal.enabled {
        d.info(d.say(
            "calendar.enabled is false: `agenda`, {{agenda}} and the digest's calendar are off",
            "calendar.enabled が false: `agenda`、{{agenda}}、週まとめの予定は使わない",
        ));
        return;
    }
    if let Some(c) = cal.command.as_deref().filter(|c| !c.is_empty()) {
        d.ok(d.say("calendar.command: ", "calendar.command: ") + c);
        return;
    }
    if !on_path(&cal.gcalcli) {
        let text = d.say("gcalcli is not on PATH: ", "gcalcli が PATH にない: ") + &cal.gcalcli;
        let advice = vec![d.say(
            "Install it and run `gcalcli init`, or set calendar.gcalcli or calendar.command.",
            "入れて `gcalcli init` を実行するか、calendar.gcalcli か calendar.command を設定してください。",
        )];
        d.add(Level::Error, text, advice);
        return;
    }
    d.ok(format!("gcalcli: {}", cal.gcalcli));
    let token = std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share/gcalcli/oauth"));
    match token.as_ref().and_then(|t| fs::metadata(t).ok()) {
        Some(m) => {
            use std::os::unix::fs::PermissionsExt;
            if m.permissions().mode() & 0o077 != 0 {
                let t = token.unwrap();
                let text = d.say(
                    "the gcalcli token is readable by others: ",
                    "gcalcli の認証情報を他の人も読める: ",
                ) + &t.display().to_string();
                let advice = vec![format!("chmod 600 {}", t.display())];
                d.add(Level::Warn, text, advice);
            }
        }
        None => {
            let text = d.say(
                "no gcalcli token at ~/.local/share/gcalcli/oauth",
                "gcalcli の認証情報が ~/.local/share/gcalcli/oauth にない",
            );
            let advice = vec![d.say(
                "Run `gcalcli init` once; every fetch fails until then.",
                "一度 `gcalcli init` を実行してください。それまで予定は取れません。",
            )];
            d.add(Level::Warn, text, advice);
        }
    }
    let names = cal.name.names();
    if names.is_empty() {
        d.info(d.say(
            "calendar.name is unset, so every calendar gcalcli can see is read",
            "calendar.name が未設定。gcalcli から見えるカレンダーをすべて読む",
        ));
    } else {
        d.ok(format!("calendar.name: {}", names.join(", ")));
    }
}

fn check_romaji(d: &mut Doctor, cfg: &Config) {
    let r = &cfg.romaji;
    if !r.enabled {
        d.info(d.say("romaji.enabled is false", "romaji.enabled が false"));
        return;
    }
    if r.prefix.chars().count() > 1 || r.prefix.chars().any(|c| c.is_whitespace() || ",:+()'\"\\".contains(c)) {
        let text = d.say(
            "romaji.prefix cannot be bound by fzf: ",
            "romaji.prefix を fzf に割り当てられない: ",
        ) + &r.prefix;
        let advice = vec![d.say(
            "Use one character other than a space and , : + ( ) ' \" \\.",
            "空白と , : + ( ) ' \" \\ 以外の 1 文字にしてください。",
        )];
        d.add(Level::Error, text, advice);
    }
    if !on_path(&r.command) {
        d.info(
            d.say("no romaji search: ", "ローマ字検索なし: ")
                + &r.command
                + &d.say(" is not on PATH", " が PATH にない"),
        );
        return;
    }
    let started = Instant::now();
    let re = crate::romaji::regex(r, "kaigi");
    let ms = started.elapsed().as_millis();
    match re {
        None => {
            let text = format!(
                "{} {}",
                r.command,
                d.say(
                    "answered nothing for `kaigi`; romaji queries will match nothing",
                    "が `kaigi` に答えない。ローマ字の検索は何も見つけない"
                )
            );
            d.add(Level::Warn, text, Vec::new());
        }
        Some(_) if ms > 100 => {
            let text = format!(
                "{} {ms}ms {}",
                d.say("romaji search: slow,", "ローマ字検索: 遅い。1 回"),
                d.say("per query, paid on every keystroke", "かかり、打つたびに待つ")
            );
            let advice = vec![d.say(
                "ttyskk reads its dictionaries every time unless it has an index: `ttyskk migemo --build-index`.",
                "ttyskk は索引がないと毎回辞書を読みます。`ttyskk migemo --build-index` を一度実行してください。",
            )];
            d.add(Level::Warn, text, advice);
        }
        Some(_) => d.ok(format!(
            "{} ({ms}ms)",
            d.say("romaji search via ", "ローマ字検索: ") + &r.command
        )),
    }
}

/// The first word of a command line, as a shell would run it.
fn first_word(command: &str) -> Option<&str> {
    command.split_whitespace().next()
}

/// Notes under `dir`, .git and other dot-directories left out.
fn count_notes(dir: &Path, ext: &str) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let mut n = 0;
    for e in entries.flatten() {
        let name = e.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let path = e.path();
        if path.is_dir() {
            n += count_notes(&path, ext);
        } else if path.extension().is_some_and(|x| x == ext) {
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finding_programs_and_notes() {
        assert!(on_path("sh"));
        assert!(!on_path("ttykasten-no-such-program"));
        assert!(on_path("/bin/sh"));
        assert_eq!(first_word("  less -R"), Some("less"));
        assert_eq!(first_word(""), None);

        let dir = std::env::temp_dir().join(format!("ttykasten-doctor-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("a/.git")).unwrap();
        for f in ["x.md", "a/y.md", "a/z.txt", "a/.git/w.md"] {
            fs::write(dir.join(f), "").unwrap();
        }
        assert_eq!(count_notes(&dir, "md"), 2);
        fs::remove_dir_all(&dir).unwrap();
    }
}
