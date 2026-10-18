//! ttykasten: fzfkasten.nvim's notes and tasks, from the terminal outside Neovim.
//!
//! Run with no arguments, it lists the actions in fzf. Subcommands are an
//! interface for key bindings and scripts, not something people are expected
//! to learn (docs/design.md). Locations, formats, the menu, the language and
//! the colours all come from the config file (config.rs).

use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::SystemTime;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{Local, NaiveDate, NaiveDateTime};
use unicode_width::UnicodeWidthStr;

use ttykasten::config::{self, Config, MenuItem};
use ttykasten::i18n::{Msgs, fill};
use ttykasten::journal::{Entry, Journal, Undo};
use ttykasten::note::{self, Periodic};
use ttykasten::tasks::{self, DueFilter, Group, Scanner, Sort, State, Want};
use ttykasten::{blockid, calendar, fileio, link, nvimswap, preview, romaji, task, template, trim, week};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ttykasten: {e:#}");
            ExitCode::FAILURE
        }
    }
}

struct App {
    cfg: Config,
    home: PathBuf,
    msg: &'static Msgs,
}

fn run() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    let (cmd, rest) = match args.split_first() {
        Some((c, r)) => (c.as_str(), r),
        None => ("", &[][..]),
    };
    if matches!(cmd, "--version" | "-V" | "version") {
        println!("{}", version());
        return Ok(());
    }
    if cmd == "doctor" {
        return doctor(std::io::stdout().is_terminal());
    }
    if cmd == "config-example" {
        print!("{}", config::EXAMPLE);
        return Ok(());
    }
    let cfg = config::load()?;
    let msg = cfg.language.msgs();
    if matches!(cmd, "-h" | "--help" | "help") {
        print!("{}", msg.usage);
        return Ok(());
    }
    let home = cfg.home()?;
    let app = App { cfg, home, msg };
    if cmd == "preview" {
        // Called by fzf as `preview FILE [LINE]`, so the arguments stay apart.
        let line = rest.get(1).and_then(|l| l.parse::<usize>().ok());
        return app.preview(rest.first().map(String::as_str).unwrap_or(""), line);
    }
    match cmd {
        // Called by fzf to fill and re-sort the task list.
        "task-lines" => return app.print_task_lines(rest),
        "find-change" => return app.find_change(rest),
        "find-romaji" => return app.find_romaji(rest),
        "find-all" => return app.find_all(rest),
        "grep-lines" => return app.grep_lines(rest.first().map_or("", String::as_str)),
        "task-key" => return app.task_key(rest),
        "task-due" => return app.task_due(rest),
        "task-tag-filter" => return app.task_tag_filter(rest),
        "task-search" => return app.task_search(rest),
        "task-capture" => {
            let message = app.capture_guided()?.unwrap_or_default();
            app.set_status(&message);
            return Ok(());
        }
        _ => {}
    }
    let arg = (!rest.is_empty()).then(|| rest.join(" "));
    app.dispatch(cmd, arg)
}

impl App {
    fn dispatch(&self, cmd: &str, arg: Option<String>) -> Result<()> {
        match cmd {
            "" | "menu" => self.menu(),
            "daily" => self.open_periodic(Periodic::Daily, now()),
            "weekly" => self.open_periodic(Periodic::Weekly, now()),
            "capture" => self.capture(arg),
            "find" => self.find(),
            "new" => self.new_note(arg),
            "log" => self.log(),
            "tasks" => self.task_list(false),
            "inbox" => self.task_list(true),
            "capture-guided" => {
                if let Some(m) = self.capture_guided()? {
                    println!("{m}");
                }
                Ok(())
            }
            "tag" => self.tag_lines(),
            "mint-ids" => self.mint_ids(),
            "undo" => {
                println!("{}", self.undo_message()?);
                Ok(())
            }
            "week" => self.week(arg.as_deref().unwrap_or("")),
            "digest" => self.digest(arg.as_deref().unwrap_or("")),
            "agenda" => self.agenda_list(arg.as_deref().unwrap_or("")),
            "grep" => self.grep(),
            "tags" => self.tags(),
            "recent" => self.recent(),
            "rename" => self.rename(),
            "links" => self.links(),
            "orphans" => self.orphans(),
            "dead-links" => self.dead_links(),
            "hubs" => self.hubs(),
            "link" => self.link(),
            "link-daily" => self.link_daily(),
            "backlinks" => self.backlinks(),
            _ => {
                // A key set in the menu also works as an action name.
                let item = self
                    .cfg
                    .menu
                    .items()
                    .into_iter()
                    .find(|i| i.key.as_deref() == Some(cmd));
                match item {
                    Some(item) => self.run_item(&item, arg),
                    None => bail!("{}\n\n{}", fill(self.msg.unknown_action, &[&cmd]), self.msg.usage),
                }
            }
        }
    }

    fn run_item(&self, item: &MenuItem, arg: Option<String>) -> Result<()> {
        if let Some(command) = &item.command {
            return self.shell(command, &[]);
        }
        match item.action.as_deref() {
            // Only built-in actions are dispatched from here, so a key never loops back.
            Some(a) if config::ACTIONS.contains(&a) => self.dispatch(a, arg),
            Some(a) => bail!(fill(self.msg.unknown_action, &[&a])),
            None => bail!(self.msg.menu_item_empty),
        }
    }

    // ---- Actions ----

    fn menu(&self) -> Result<()> {
        let items = self.cfg.menu.items();
        let width = items
            .iter()
            .filter_map(|i| i.key.as_deref())
            .map(|k| k.chars().count())
            .max()
            .unwrap_or(0);
        let lines: String = items
            .iter()
            .enumerate()
            .map(|(n, item)| {
                let label = item
                    .label
                    .as_deref()
                    .or_else(|| item.action.as_deref().and_then(|a| self.msg.action_desc(a)))
                    .or(item.command.as_deref())
                    .unwrap_or("");
                let key = item.key.as_deref().unwrap_or("");
                if width == 0 {
                    format!("{n}\t{label}\n")
                } else {
                    let pad = width - key.chars().count();
                    format!("{n}\t{key}{:pad$}  {label}\n", "")
                }
            })
            .collect();
        let Some(out) = self.fzf(
            &lines,
            &[
                "--delimiter=\t",
                "--with-nth=2..",
                &format!("--prompt={}", self.msg.prompt_menu),
                "--height=~40%",
                "--layout=reverse",
            ],
        )?
        else {
            return Ok(());
        };
        let chosen = out
            .first()
            .and_then(|l| l.split('\t').next())
            .and_then(|n| n.parse::<usize>().ok())
            .and_then(|n| items.get(n));
        match chosen {
            Some(item) => self.run_item(item, None),
            None => Ok(()),
        }
    }

    fn new_note(&self, arg: Option<String>) -> Result<()> {
        let Some(title) = self.arg_or_input(arg, self.msg.prompt_title)? else {
            return Ok(());
        };
        if note::title_is_blank(&title) {
            return Ok(());
        }
        let Some(name) = note::sanitize_filename(&title) else {
            bail!(fill(self.msg.no_file_name, &[&format!("{title:?}")]));
        };
        let path = note::new_note_path(&self.home, &name, &self.cfg.extension);
        // An existing note is opened as it is, not overwritten from the template (format.md Q3).
        if path.exists() {
            eprintln!("ttykasten: {}", fill(self.msg.opening_existing, &[&path.display()]));
        } else {
            let picked = if self.cfg.pick_template {
                self.pick_template()?
            } else {
                None
            };
            let content = match picked.as_ref().or(self.cfg.new_note_template.as_ref()) {
                Some(t) => {
                    let now = now();
                    template::load(&self.home, &self.cfg.hdate_format, t, &title, now, &|k| {
                        self.agenda(k, now)
                    })
                }
                None => format!("# {title}"),
            };
            self.guard(&path)?;
            fileio::write_text(&path, &content).with_context(|| fill(self.msg.cannot_write, &[&path.display()]))?;
        }
        self.edit(&path)
    }

    fn capture(&self, arg: Option<String>) -> Result<()> {
        let Some(rel) = self.cfg.tasks.capture_target() else {
            bail!(self.msg.no_capture_note);
        };
        let Some(text) = self.arg_or_input(arg, self.msg.prompt_task)? else {
            return Ok(());
        };
        if let Some(m) = self.capture_text(rel, &text, None)? {
            println!("{m}");
        }
        Ok(())
    }

    /// `open_note(type, t)` (§5). A missing note is created from its template and written at once.
    fn open_periodic(&self, kind: Periodic, t: NaiveDateTime) -> Result<()> {
        let cfg = &self.cfg;
        let path = self.home.join(kind.rel(cfg, t));
        if !path.exists() {
            fs::create_dir_all(self.home.join(kind.dir(cfg)))?;
            let content = template::load(
                &self.home,
                &cfg.hdate_format,
                kind.template(cfg),
                &kind.name(cfg, t),
                t,
                &|k| self.agenda(k, t),
            );
            self.guard(&path)?;
            fileio::write_text(&path, &content).with_context(|| fill(self.msg.cannot_write, &[&path.display()]))?;
        }
        self.edit(&path)
    }

    /// The list of daily and weekly notes (§6).
    fn log(&self) -> Result<()> {
        let cfg = &self.cfg;
        let now = now();
        let mut entries: Vec<(Periodic, NaiveDateTime, String)> = Vec::new();
        for i in 0..cfg.notes.daily.lookback_days {
            let t = note::days_from(now, -i64::from(i));
            entries.push((Periodic::Daily, t, t.format(&cfg.log.day_label).to_string()));
        }
        for i in 0..cfg.notes.weekly.lookback_weeks {
            let t = note::days_from(now, -7 * i64::from(i));
            entries.push((Periodic::Weekly, t, t.format(&cfg.log.week_label).to_string()));
        }
        let lines: String = entries
            .iter()
            .enumerate()
            .map(|(i, (kind, t, label))| {
                let rel = kind.rel(cfg, *t);
                let mark = if self.home.join(&rel).is_file() { '✓' } else { ' ' };
                format!("{i}\t{rel}\t{mark} {label}\n")
            })
            .collect();
        let preview = self.preview_arg("{2}")?;
        let mut args = vec![
            "--delimiter=\t".to_string(),
            "--with-nth=3".into(),
            "--no-sort".into(),
            format!(
                "--header={}{}",
                self.msg.log_header,
                key_hint(self.msg.log_date_hint, &self.cfg.log.date_key)
            ),
            format!("--prompt={}", self.msg.prompt_log),
        ];
        args.extend(expect(&self.cfg.log.date_key));
        args.extend(preview);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let Some(out) = self.fzf(&lines, &args)? else {
            return Ok(());
        };
        if pressed(&out, &self.cfg.log.date_key) {
            return self.open_date(now);
        }
        let Some((kind, t, _)) = out
            .get(1)
            .and_then(|l| l.split('\t').next())
            .and_then(|i| i.parse::<usize>().ok())
            .and_then(|i| entries.get(i))
        else {
            return Ok(());
        };
        self.open_periodic(*kind, *t)
    }

    /// ctrl-x: asks for a date and opens that day's daily note at 12:00. Future dates are refused.
    fn open_date(&self, now: NaiveDateTime) -> Result<()> {
        let Some(s) = self.input(self.msg.prompt_date)? else {
            return Ok(());
        };
        let s = s.trim();
        let well_formed = s.len() == 10
            && s.bytes().enumerate().all(|(i, b)| {
                if i == 4 || i == 7 {
                    b == b'-'
                } else {
                    b.is_ascii_digit()
                }
            });
        let date = well_formed
            .then(|| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
            .flatten()
            .ok_or_else(|| anyhow!(fill(self.msg.bad_date, &[&s])))?;
        if date > now.date() {
            bail!(fill(self.msg.future_date, &[&s]));
        }
        self.open_periodic(Periodic::Daily, note::days_from(date.and_time(now.time()), 0))
    }

    /// Finds a note (§7) by path with fzf. With romaji on, a query starting
    /// with `/` is turned into a regex by `ttyskk migemo` and matched against
    /// each note's path and headings instead.
    fn find(&self) -> Result<()> {
        let files = self.note_files("--no-ignore-vcs")?;
        let items: String = files.iter().map(|f| format!("{f}\n")).collect();
        let mut args = vec![
            "--scheme=path".to_string(),
            format!("--prompt={}", self.msg.prompt_find),
        ];
        let prefix = self.romaji_prefix()?;
        let index = if prefix.is_some() && romaji::available(&self.cfg.romaji) {
            Some(self.write_find_index(&files)?)
        } else {
            None
        };
        if let Some(index) = &index {
            let exe = self.self_command()?;
            let idx = shell_quote(&index.to_string_lossy());
            let p = prefix.unwrap_or_default();
            args.push(format!("--header={}", fill(self.msg.romaji_hint, &[&p, &p])));
            args.push("--bind=start:unbind(change)".into());
            args.push(format!("--bind=change:transform:{exe} find-change {idx} {{q}}"));
            args.push(format!(
                "--bind={p}:transform:[ -z {{q}} ] && echo 'rebind(change)+put({p})' || echo 'put({p})'"
            ));
        }
        args.extend(self.preview_arg("{}")?);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = self.fzf(&items, &args);
        if let Some(index) = &index {
            let _ = fs::remove_file(index);
        }
        match out?.as_ref().and_then(|o| o.first()) {
            Some(rel) if !rel.is_empty() => self.edit(&self.home.join(rel)),
            _ => Ok(()),
        }
    }

    /// The romaji index for `find`: each note's path, then its path and
    /// heading lines as one line, tab separated.
    fn write_find_index(&self, files: &[String]) -> Result<PathBuf> {
        let mut headings: HashMap<String, Vec<String>> = HashMap::new();
        if self.cfg.romaji.headings {
            let glob = format!("*.{}", self.cfg.extension);
            let out = Command::new("rg")
                .args([
                    "--no-messages",
                    "--no-ignore-vcs",
                    "--heading",
                    "--with-filename",
                    "--no-line-number",
                ])
                .args(["--color", "never", "--glob", &glob, "-e", r"^#+\s+\S"])
                .current_dir(&self.home)
                .output()
                .with_context(|| fill(self.msg.cannot_start, &[&"rg"]))?;
            let text = String::from_utf8_lossy(&out.stdout);
            let mut current: Option<String> = None;
            for line in text.lines() {
                match &current {
                    _ if line.is_empty() => current = None,
                    None => current = Some(line.to_string()),
                    Some(rel) => headings.entry(rel.clone()).or_default().push(line.to_string()),
                }
            }
        }
        let mut tsv = String::new();
        for rel in files {
            let mut text = rel.clone();
            for h in headings.get(rel).into_iter().flatten() {
                text.push(' ');
                text.push_str(h);
            }
            tsv.push_str(&format!("{rel}\t{}\n", text.replace(['\t', '\n'], " ")));
        }
        let path = env::temp_dir().join(format!("ttykasten-find-{}.tsv", std::process::id()));
        fs::write(&path, tsv)?;
        Ok(path)
    }

    /// `find-change INDEX QUERY`: what a changed query means to fzf. While it
    /// starts with `/`, fzf's own matching is off and the list is the romaji
    /// match; once it doesn't, fzf matches again and stops asking.
    fn find_change(&self, args: &[String]) -> Result<()> {
        let index = shell_quote(args.first().map_or("", String::as_str));
        let exe = self.self_command()?;
        let prefix = self.romaji_prefix()?;
        if prefix.is_some_and(|p| args.get(1).is_some_and(|q| q.starts_with(p))) {
            println!("disable-search+reload({exe} find-romaji {index} {{q}})");
        } else {
            println!("enable-search+unbind(change)+reload({exe} find-all {index})");
        }
        Ok(())
    }

    /// `find-romaji INDEX QUERY`: the notes whose path or headings match the
    /// romaji after the `/`. Anything it can't answer lists every note, since
    /// an empty list would read as "nothing matched".
    fn find_romaji(&self, args: &[String]) -> Result<()> {
        let text = fs::read_to_string(args.first().map_or("", String::as_str)).unwrap_or_default();
        let query = args.get(1).map_or("", String::as_str);
        let query = self
            .romaji_prefix()?
            .and_then(|p| query.strip_prefix(p))
            .unwrap_or(query);
        let re = romaji::regex(&self.cfg.romaji, query).and_then(|r| regex::Regex::new(&r).ok());
        let mut out = std::io::stdout().lock();
        for line in text.lines() {
            let (rel, hay) = line.split_once('\t').unwrap_or((line, line));
            if re.as_ref().is_none_or(|re| re.is_match(hay)) {
                let _ = writeln!(out, "{rel}");
            }
        }
        Ok(())
    }

    /// `find-all INDEX`: every note in the index.
    fn find_all(&self, args: &[String]) -> Result<()> {
        let text = fs::read_to_string(args.first().map_or("", String::as_str)).unwrap_or_default();
        let mut out = std::io::stdout().lock();
        for line in text.lines() {
            let _ = writeln!(out, "{}", line.split_once('\t').map_or(line, |(rel, _)| rel));
        }
        Ok(())
    }

    /// Full-text search (SearchContent): rg runs again as the query changes.
    /// A query starting with `/` is romaji. Enter opens the line.
    fn grep(&self) -> Result<()> {
        let exe = self.self_command()?;
        let mut args = vec![
            "--disabled".to_string(),
            "--delimiter=\t".into(),
            "--with-nth=3..".into(),
            format!("--prompt={}", self.msg.prompt_grep),
            format!("--bind=change:reload:{exe} grep-lines {{q}}"),
            "--preview-window=+{2}-/2".into(),
        ];
        if let Some(p) = self.romaji_prefix()?.filter(|_| self.cfg.romaji.enabled) {
            args.push(format!("--header={}", fill(self.msg.romaji_hint, &[&p, &p])));
        }
        args.extend(self.preview_arg("{1} {2}")?);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let Some(out) = self.fzf("", &args)? else {
            return Ok(());
        };
        self.open_listed(out.first())
    }

    /// Opens `rel\tline\t…` as picked from a list.
    fn open_listed(&self, picked: Option<&String>) -> Result<()> {
        let mut fields = picked.map(|l| l.split('\t')).into_iter().flatten();
        match (fields.next(), fields.next().and_then(|n| n.parse::<usize>().ok())) {
            (Some(rel), Some(lineno)) if !rel.is_empty() => self.edit_at(&self.home.join(rel), lineno),
            _ => Ok(()),
        }
    }

    /// `grep-lines QUERY`: rg over every note (ignore files disregarded, as
    /// the plugin's grep does), as `rel\tline\trel:line: text`.
    fn grep_lines(&self, query: &str) -> Result<()> {
        let query = trim(query);
        let prefix = self.romaji_prefix()?;
        if query.is_empty() || Some(query) == prefix {
            return Ok(());
        }
        let pattern = match prefix.and_then(|p| query.strip_prefix(p)) {
            Some(r) => romaji::regex(&self.cfg.romaji, r).unwrap_or_else(|| regex::escape(r)),
            None => query.to_string(),
        };
        let print = |fixed: bool| -> Result<bool> {
            let glob = format!("*.{}", self.cfg.extension);
            let mut cmd = Command::new("rg");
            cmd.args([
                "--no-ignore",
                "--no-messages",
                "--null",
                "--line-number",
                "--no-heading",
                "--color=never",
            ])
            .args(["--smart-case", "--glob", &glob]);
            if fixed {
                cmd.arg("--fixed-strings");
            }
            let out = cmd.args(["-e", &pattern]).current_dir(&self.home).output()?;
            if out.status.code() == Some(2) && !fixed {
                return Ok(false);
            }
            let text = String::from_utf8_lossy(&out.stdout);
            let mut w = std::io::stdout().lock();
            for record in text.lines() {
                if let Some((file, rest)) = record.split_once('\0')
                    && let Some((lineno, line)) = rest.split_once(':')
                {
                    let _ = writeln!(
                        w,
                        "{file}\t{lineno}\t{file}:{lineno}: {}",
                        trim(line).replace('\t', " ")
                    );
                }
            }
            Ok(true)
        };
        // A query that isn't a valid regex is searched as plain text.
        if !print(false)? {
            print(true)?;
        }
        Ok(())
    }

    /// Tags (SearchByTag): every `#tag` with how often it occurs, then the
    /// lines that carry the one picked.
    fn tags(&self) -> Result<()> {
        let glob = format!("*.{}", self.cfg.extension);
        let out = Command::new("rg")
            .args([
                "--no-ignore",
                "--no-messages",
                "--no-filename",
                "--no-line-number",
                "-o",
                "--glob",
                &glob,
            ])
            .args(["-e", "#[A-Za-z0-9_-]+"])
            .current_dir(&self.home)
            .output()
            .with_context(|| fill(self.msg.cannot_start, &[&"rg"]))?;
        let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
        for tag in String::from_utf8_lossy(&out.stdout).lines() {
            *counts.entry(tag.to_string()).or_default() += 1;
        }
        if counts.is_empty() {
            eprintln!("ttykasten: {}", self.msg.nothing_found);
            return Ok(());
        }
        let items: String = counts.iter().map(|(t, n)| format!("{t}\t{t}  ({n})\n")).collect();
        let args = [
            "--delimiter=\t",
            "--with-nth=2..",
            &format!("--prompt={}", self.msg.prompt_tag),
        ];
        let Some(out) = self.fzf(&items, &args)? else {
            return Ok(());
        };
        let Some(tag) = out.first().and_then(|l| l.split('\t').next()).filter(|t| !t.is_empty()) else {
            return Ok(());
        };
        let pattern = format!("{}([^A-Za-z0-9_-]|$)", regex::escape(tag));
        let out = Command::new("rg")
            .args([
                "--no-ignore",
                "--no-messages",
                "--null",
                "--line-number",
                "--no-heading",
                "--color=never",
            ])
            .args(["--glob", &glob, "-e", &pattern])
            .current_dir(&self.home)
            .output()?;
        let mut lines: Vec<(String, usize, String)> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|r| {
                let (file, rest) = r.split_once('\0')?;
                let (n, text) = rest.split_once(':')?;
                Some((file.to_string(), n.parse().ok()?, trim(text).replace('\t', " ")))
            })
            .collect();
        lines.sort();
        let items: String = lines
            .iter()
            .map(|(f, n, t)| format!("{f}\t{n}\t{f}:{n}: {t}\n"))
            .collect();
        let mut args = vec![
            "--delimiter=\t".to_string(),
            "--with-nth=3..".into(),
            format!("--prompt={tag}> "),
            "--preview-window=+{2}-/2".into(),
        ];
        args.extend(self.preview_arg("{1} {2}")?);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let Some(out) = self.fzf(&items, &args)? else {
            return Ok(());
        };
        self.open_listed(out.first())
    }

    /// Recent notes (`:FzfKastenRecent`): every note under home, `.git`
    /// pruned and `recent.ignore_dirs` left out, newest first.
    fn recent(&self) -> Result<()> {
        let ext = format!(".{}", self.cfg.extension);
        let mut found: Vec<(SystemTime, String)> = Vec::new();
        let mut stack = vec![self.home.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for e in entries.flatten() {
                let path = e.path();
                let Ok(ft) = e.file_type() else {
                    continue;
                };
                if ft.is_dir() {
                    if e.file_name() != ".git" {
                        stack.push(path);
                    }
                    continue;
                }
                let Ok(rel) = path.strip_prefix(&self.home) else {
                    continue;
                };
                let rel = rel.to_string_lossy().into_owned();
                let ignored = self
                    .cfg
                    .recent
                    .ignore_dirs
                    .iter()
                    .any(|d| rel == *d || rel.strip_prefix(d.as_str()).is_some_and(|r| r.starts_with('/')));
                if rel.ends_with(&ext)
                    && !ignored
                    && let Ok(m) = e.metadata().and_then(|m| m.modified())
                {
                    found.push((m, rel));
                }
            }
        }
        found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        found.truncate(self.cfg.recent.limit);
        let today = Local::now().date_naive();
        let items: String = found
            .iter()
            .map(|(m, rel)| {
                let t: chrono::DateTime<Local> = (*m).into();
                let when = if t.date_naive() == today {
                    format!("{} {}", self.msg.recent_today, t.format("%H:%M"))
                } else {
                    t.format("%m-%d %a").to_string()
                };
                format!("{rel}\t{when:<11} {rel}\n")
            })
            .collect();
        let mut args = vec![
            "--delimiter=\t".to_string(),
            "--with-nth=2..".into(),
            "--no-sort".into(),
            format!("--prompt={}", self.msg.prompt_recent),
        ];
        args.extend(self.preview_arg("{1}")?);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let Some(out) = self.fzf(&items, &args)? else {
            return Ok(());
        };
        match out.first().and_then(|l| l.split('\t').next()) {
            Some(rel) if !rel.is_empty() => self.edit(&self.home.join(rel)),
            _ => Ok(()),
        }
    }

    /// Picks a note from the finder's list (with preview); its path relative
    /// to home.
    fn pick_note(&self, prompt: &str) -> Result<Option<String>> {
        let files = self.note_files("--no-ignore-vcs")?;
        let items: String = files.iter().map(|f| format!("{f}\n")).collect();
        let mut args = vec!["--scheme=path".to_string(), format!("--prompt={prompt}")];
        args.extend(self.preview_arg("{}")?);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        Ok(self
            .fzf(&items, &args)?
            .and_then(|o| o.into_iter().next())
            .filter(|r| !r.is_empty()))
    }

    /// The template for a new note, picked from `home/templates/` with the
    /// default first. None (Esc) means the default.
    fn pick_template(&self) -> Result<Option<String>> {
        let dir = self.home.join("templates");
        let out = Command::new("rg")
            .args(["--files", "--no-messages", "--no-ignore"])
            .current_dir(&dir)
            .output();
        let Ok(out) = out else {
            return Ok(None);
        };
        let mut files: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_string)
            .collect();
        if files.is_empty() {
            return Ok(None);
        }
        files.sort();
        let default = self
            .cfg
            .new_note_template
            .as_deref()
            .map(|t| t.strip_prefix("templates/").unwrap_or(t).to_string());
        if let Some(d) = &default
            && let Some(i) = files.iter().position(|f| f == d)
        {
            let f = files.remove(i);
            files.insert(0, f);
        }
        let items: String = files.iter().map(|f| format!("{f}\n")).collect();
        let mut args = vec![
            "--no-sort".to_string(),
            format!("--prompt={}", self.msg.prompt_template),
        ];
        args.extend(self.preview_arg("templates/{}")?);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        // Only the basename is kept, as in the plugin.
        Ok(self
            .fzf(&items, &args)?
            .and_then(|o| o.into_iter().next())
            .filter(|f| !f.is_empty())
            .map(|f| f.rsplit('/').next().unwrap_or(&f).to_string()))
    }

    /// Rename (§9.2): moves the note, sanitising the new name, then rewrites
    /// every link to its old name in every note. Refuses before changing
    /// anything when a note involved has unsaved changes in Neovim.
    fn rename(&self) -> Result<()> {
        let Some(rel) = self.pick_note(self.msg.prompt_find)? else {
            return Ok(());
        };
        let old_path = self.home.join(&rel);
        let old_name = link::note_name(Path::new(&rel));
        let out = self.run_fzf(
            Stdio::null(),
            None,
            &[
                &format!("--prompt={}", self.msg.prompt_rename),
                &format!("--query={old_name}"),
                "--print-query",
                "--disabled",
                "--info=hidden",
                "--no-separator",
                "--height=1",
                "--layout=reverse",
            ],
        )?;
        let Some(raw) = out.and_then(|o| o.into_iter().next()) else {
            return Ok(());
        };
        let Some(new_name) = note::sanitize_filename(&raw) else {
            bail!(fill(self.msg.no_file_name, &[&format!("{raw:?}")]));
        };
        if new_name == old_name {
            return Ok(());
        }
        let ext = old_path
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default();
        let new_path = old_path.with_file_name(format!("{new_name}.{ext}"));
        if new_path.exists() {
            bail!(fill(
                self.msg.rename_exists,
                &[&new_path.strip_prefix(&self.home).unwrap_or(&new_path).display()]
            ));
        }
        // Every rewrite is worked out first, so nothing moves if one is refused.
        let mut rewrites: Vec<(PathBuf, Vec<String>)> = Vec::new();
        for file in self.note_files("--no-ignore")? {
            let path = self.home.join(&file);
            let Ok(lines) = fileio::read_lines(&path) else {
                continue;
            };
            let mut changed = false;
            let new_lines: Vec<String> = lines
                .iter()
                .map(
                    |l| match link::retarget_line(l, &self.cfg.extension, &old_name, &new_name) {
                        Some(n) => {
                            changed = true;
                            n
                        }
                        None => l.clone(),
                    },
                )
                .collect();
            if changed {
                rewrites.push((path, new_lines));
            }
        }
        self.guard(&old_path)?;
        for (path, _) in &rewrites {
            self.guard(path)?;
        }
        fs::rename(&old_path, &new_path).with_context(|| fill(self.msg.cannot_write, &[&new_path.display()]))?;
        for (path, lines) in &rewrites {
            let target = if *path == old_path { &new_path } else { path };
            fileio::write_lines(target, lines).with_context(|| fill(self.msg.cannot_write, &[&target.display()]))?;
        }
        println!("{}", fill(self.msg.renamed, &[&old_name, &new_name, &rewrites.len()]));
        self.edit(&new_path)
    }

    /// The link graph over every note (the plugin's glob: hidden files skipped,
    /// ignore files disregarded), in path order.
    fn graph(&self) -> Result<ttykasten::graph::Graph> {
        let mut files = self.note_files("--no-ignore")?;
        files.sort();
        let notes: Vec<(String, Vec<String>)> = files
            .into_iter()
            .filter_map(|rel| fileio::read_lines(&self.home.join(&rel)).ok().map(|l| (rel, l)))
            .collect();
        Ok(ttykasten::graph::build(
            &notes,
            &self.cfg.extension,
            &self.cfg.graph.ignore_dirs,
        ))
    }

    /// A list of `rel\tline\tshown` rows in fzf; enter opens the line.
    fn pick_rows(&self, rows: &[(String, usize, String)], prompt: &str, header: &str) -> Result<()> {
        if rows.is_empty() {
            eprintln!("ttykasten: {}", self.msg.nothing_found);
            return Ok(());
        }
        let items: String = rows.iter().map(|(r, n, t)| format!("{r}\t{n}\t{t}\n")).collect();
        let mut args = vec![
            "--delimiter=\t".to_string(),
            "--with-nth=3..".into(),
            "--no-sort".into(),
            format!("--prompt={prompt}"),
            format!("--header={header}"),
            "--preview-window=+{2}-/2".into(),
        ];
        args.extend(self.preview_arg("{1} {2}")?);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let Some(out) = self.fzf(&items, &args)? else {
            return Ok(());
        };
        self.open_listed(out.first())
    }

    /// LinkTree: a picked note and what it is linked with, `graph.depth`
    /// links deep.
    fn links(&self) -> Result<()> {
        let Some(rel) = self.pick_note(self.msg.prompt_find)? else {
            return Ok(());
        };
        let g = self.graph()?;
        let root = link::note_name(Path::new(&rel));
        let depth = self.cfg.graph.depth.max(1);
        let mut rows = vec![(rel.clone(), 1, format!("● {root}"))];
        rows.extend(
            ttykasten::graph::tree(&g, &root, depth)
                .into_iter()
                .map(|r| (r.rel, r.lineno, r.display)),
        );
        if rows.len() == 1 {
            eprintln!("ttykasten: {}", self.msg.nothing_found);
            return Ok(());
        }
        self.pick_rows(&rows, &fill(self.msg.links_prompt, &[&depth]), self.msg.links_header)
    }

    fn orphans(&self) -> Result<()> {
        let rels = ttykasten::graph::orphans(&self.graph()?);
        let rows: Vec<(String, usize, String)> = rels.into_iter().map(|r| (r.clone(), 1, r)).collect();
        self.pick_rows(&rows, &fill(self.msg.orphans_prompt, &[&rows.len()]), "")
    }

    fn dead_links(&self) -> Result<()> {
        let g = self.graph()?;
        let rows: Vec<(String, usize, String)> = ttykasten::graph::dead_links(&g)
            .into_iter()
            .map(|e| (e.rel.clone(), e.lineno, format!("{}  ←  {}", e.to, e.line)))
            .collect();
        self.pick_rows(&rows, &fill(self.msg.dead_prompt, &[&rows.len()]), self.msg.dead_header)
    }

    fn hubs(&self) -> Result<()> {
        let g = self.graph()?;
        let rows: Vec<(String, usize, String)> = ttykasten::graph::hubs(&g)
            .into_iter()
            .map(|(name, rel, back, out)| (rel, 1, format!("{back:3} ←  {out:3} →   {name}")))
            .collect();
        self.pick_rows(&rows, &fill(self.msg.hubs_prompt, &[&rows.len()]), self.msg.hubs_header)
    }

    /// The task list or the inbox in fzf (§8). Enter opens the note at the
    /// task's line; the keys in `tasks.keys` act on the task under the cursor
    /// or change the view, and reload the list.
    fn task_list(&self, inbox: bool) -> Result<()> {
        let view = View {
            inbox,
            sort: Sort::parse(&self.cfg.tasks.sort),
            ..View::default()
        };
        let lines = self.task_lines(&view)?;
        let exe = self.self_command()?;
        let kind = view.kind();
        let k = &self.cfg.tasks.keys;
        let line = "{1} {2} {3}";
        let refresh = format!("transform:{exe} task-key refresh {kind}");
        let view_key = |name: &str| format!("transform:{exe} task-key {name} {kind}");
        let binds = [
            (&k.done, format!("transform:{exe} task-key done {kind} {line}")),
            (&k.cancel, format!("transform:{exe} task-key cancel {kind} {line}")),
            (&k.tag, format!("transform:{exe} task-key tag {kind} {line}")),
            (&k.undo, view_key("undo")),
            (&k.sort, view_key("sort")),
            (&k.reverse, view_key("reverse")),
            (&k.closed, view_key("closed")),
            (&k.filter_due, view_key("due-filter")),
            (&k.group, view_key("group")),
            (
                &k.filter_tag,
                format!("execute({exe} task-tag-filter {kind})+{refresh}"),
            ),
            (&k.due, format!("execute({exe} task-due {line})+{refresh}")),
            (&k.capture, format!("execute({exe} task-capture)+{refresh}")),
        ];
        let mut args = vec![
            "--delimiter=\t".to_string(),
            "--with-nth=4..".into(),
            "--ansi".into(),
            "--no-sort".into(),
            format!("--prompt={}", self.view_prompt(&view)),
            format!("--header={}", self.keys_header()),
            "--preview-window=+{2}-/2".into(),
        ];
        args.extend(
            binds
                .iter()
                .filter(|(key, _)| !key.is_empty())
                .map(|(key, act)| format!("--bind={key}:{act}")),
        );
        if let Some(p) = self.task_romaji_prefix()? {
            // As in `find`: a query starting with the prefix switches fzf's
            // matching off and has the list narrowed by romaji instead.
            args.push("--bind=start:unbind(change)".into());
            args.push(format!("--bind=change:transform:{exe} task-search {kind} {{q}}"));
            args.push(format!(
                "--bind={p}:transform:[ -z {{q}} ] && echo 'rebind(change)+put({p})' || echo 'put({p})'"
            ));
        }
        args.extend(self.preview_arg("{1} {2}")?);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let Some(out) = self.fzf(&lines, &args)? else {
            return Ok(());
        };
        let mut fields = out.first().map(|l| l.split('\t')).into_iter().flatten();
        match (fields.next(), fields.next().and_then(|n| n.parse::<usize>().ok())) {
            (Some(rel), Some(lineno)) => self.edit_at(&self.home.join(rel), lineno),
            _ => Ok(()),
        }
    }

    fn keys_header(&self) -> String {
        let k = &self.cfg.tasks.keys;
        let keys: [&dyn std::fmt::Display; 12] = [
            &k.done,
            &k.cancel,
            &k.tag,
            &k.due,
            &k.capture,
            &k.undo,
            &k.sort,
            &k.reverse,
            &k.closed,
            &k.filter_tag,
            &k.filter_due,
            &k.group,
        ];
        let mut header = fill(self.msg.tasks_keys, &keys);
        if let Ok(Some(p)) = self.task_romaji_prefix() {
            header = format!("{header}\n{}", fill(self.msg.romaji_hint, &[&p, &p]));
        }
        header
    }

    /// The romaji prefix when the task list offers romaji narrowing.
    fn task_romaji_prefix(&self) -> Result<Option<&str>> {
        Ok(self.romaji_prefix()?.filter(|_| romaji::installed(&self.cfg.romaji)))
    }

    /// `task-search KIND QUERY`, run by fzf as the query changes while it
    /// starts with the romaji prefix: the list narrowed by romaji, or fzf's
    /// own matching back once the prefix is gone.
    fn task_search(&self, args: &[String]) -> Result<()> {
        let kind = args.first().map_or("tasks", String::as_str);
        let view = View::from_prompt(kind == "inbox", &env::var("FZF_PROMPT").unwrap_or_default());
        let exe = self.self_command()?;
        let spec = shell_quote(&view.encode());
        let query = args.get(1).map_or("", String::as_str);
        if self.romaji_prefix()?.is_some_and(|p| query.starts_with(p)) {
            println!("disable-search+reload({exe} task-lines {kind} {spec} {{q}})");
        } else {
            println!("enable-search+unbind(change)+reload({exe} task-lines {kind} {spec})");
        }
        Ok(())
    }

    /// The prompt carries the view, so the keys' commands can read it back
    /// from $FZF_PROMPT: `tasks [due, reversed, #budget, overdue]> `.
    fn view_prompt(&self, view: &View) -> String {
        let name = if view.inbox {
            self.msg.prompt_inbox
        } else {
            self.msg.prompt_tasks
        };
        format!("{name} [{}]> ", view.encode())
    }

    /// The scanned tasks (all of them, before any filter) and the notes'
    /// lines, by path.
    fn scan_tasks(&self, scanner: &Scanner) -> Result<Scanned> {
        let cutoff = self
            .cfg
            .tasks
            .since_days
            .map(|n| note::days_from(now(), -i64::from(n)).format("%Y-%m-%d").to_string());
        let mut all = Vec::new();
        let mut texts: HashMap<String, Vec<String>> = HashMap::new();
        for rel in self.checkbox_notes()? {
            if let Ok(lines) = fileio::read_lines(&self.home.join(&rel)) {
                scanner.scan_note(&rel, &lines, cutoff.as_deref(), &mut all);
                texts.insert(rel, lines);
            }
        }
        Ok((all, texts))
    }

    fn scanner(&self) -> Result<Scanner<'_>> {
        Scanner::new(&self.cfg).map_err(|e| {
            anyhow!(fill(
                self.msg.bad_pattern,
                &[&"tasks.date_patterns / tasks.headings", &e]
            ))
        })
    }

    /// The tasks a view shows, in order.
    fn view_tasks(&self, scanner: &Scanner, view: &View) -> Result<Scanned> {
        let (all, texts) = self.scan_tasks(scanner)?;
        let shown = tasks::select(
            all,
            Want {
                inbox: view.inbox,
                ..Want::default()
            },
            &view.filter(),
            view.sort,
            view.reverse,
        );
        Ok((shown, texts))
    }

    /// The list's lines for fzf: path, line number, the line's fingerprint,
    /// and what is shown (the group, the entry, then the note and line
    /// dimmed). fzf has no rows that are not entries, so in a grouped view
    /// the group's name leads each row, which also lets a query narrow to it.
    fn task_lines(&self, view: &View) -> Result<String> {
        self.task_lines_matching(view, "")
    }

    /// `task_lines`, and when `query` starts with the romaji prefix, only the
    /// tasks whose text the romaji after it matches (all of them when the
    /// backend cannot answer, as `find` does).
    fn task_lines_matching(&self, view: &View, query: &str) -> Result<String> {
        let scanner = self.scanner()?;
        let (mut shown, texts) = self.view_tasks(&scanner, view)?;
        if let Some(rest) = self.romaji_prefix()?.and_then(|p| query.strip_prefix(p))
            && let Some(re) = romaji::regex(&self.cfg.romaji, rest).and_then(|r| regex::Regex::new(&r).ok())
        {
            shown.retain(|t| re.is_match(&t.text));
        }
        let row = |t: &tasks::Task, label: &str, orphaned: bool| {
            let raw = texts
                .get(&t.rel)
                .and_then(|l| l.get(t.lineno - 1))
                .map_or("", String::as_str);
            format!(
                "{}\t{}\t{}\t{label}{}  \x1b[2m{}:{}\x1b[0m\n",
                t.rel,
                t.lineno,
                tasks::line_hash(raw),
                scanner.entry_text_as(t, orphaned),
                t.rel,
                t.lineno
            )
        };
        let mut out = String::new();
        let Some(by) = view.group else {
            for t in &shown {
                out.push_str(&row(t, "", t.orphaned));
            }
            return Ok(out);
        };
        let f = view.filter();
        let groups = tasks::group(&shown, by, &f.today, &f.week_end);
        let labels: Vec<String> = groups.iter().map(|(key, _)| self.group_label(key)).collect();
        let width = labels.iter().map(|l| l.width()).max().unwrap_or(0);
        for ((_, members), label) in groups.iter().zip(&labels) {
            let label = format!("{label}{} │ ", " ".repeat(width - label.width()));
            for &(i, orphaned) in members {
                out.push_str(&row(&shown[i], &label, orphaned));
            }
        }
        Ok(out)
    }

    fn group_label(&self, key: &tasks::GroupKey) -> String {
        use tasks::{Bucket, GroupKey};
        match key {
            GroupKey::Note(rel) => rel.clone(),
            GroupKey::Tag(name) => format!("#{name}"),
            GroupKey::NoTag => self.msg.group_no_tag.to_string(),
            GroupKey::Due(b) => match b {
                Bucket::Overdue => self.msg.group_overdue,
                Bucket::Today => self.msg.group_today,
                Bucket::Week => self.msg.group_week,
                Bucket::Later => self.msg.group_later,
                Bucket::None => self.msg.group_no_due,
            }
            .to_string(),
        }
    }

    /// Notes that contain a checkbox, as the plugin finds them: rg honours
    /// .gitignore here (§2.2).
    fn checkbox_notes(&self) -> Result<Vec<String>> {
        let glob = format!("*.{}", self.cfg.extension);
        let out = Command::new("rg")
            .args(["--files-with-matches", "--no-messages", "--glob", &glob])
            .args(["-e", r"^\s*[-*]\s+\[[ xX-]\]\s+"])
            .current_dir(&self.home)
            .output()
            .with_context(|| fill(self.msg.cannot_start, &[&"rg"]))?;
        let text = String::from_utf8_lossy(&out.stdout);
        Ok(text.lines().map(str::to_string).collect())
    }

    /// `task-lines KIND VIEW`, for fzf's reload; VIEW is what the prompt
    /// carries between the brackets.
    fn print_task_lines(&self, args: &[String]) -> Result<()> {
        let view = View::decode(
            args.first().map(String::as_str) == Some("inbox"),
            args.get(1).map_or("", String::as_str),
        );
        let lines = self.task_lines_matching(&view, args.get(2).map_or("", String::as_str))?;
        let _ = std::io::stdout().lock().write_all(lines.as_bytes());
        Ok(())
    }

    /// `task-key ACTION KIND [REL LINENO HASH]`, run by fzf's `transform`:
    /// does the action, then prints the fzf actions that reload the list and
    /// show the outcome in the header. The view is read from $FZF_PROMPT.
    fn task_key(&self, args: &[String]) -> Result<()> {
        let arg = |i: usize| args.get(i).map_or("", String::as_str);
        let mut view = View::from_prompt(arg(1) == "inbox", &env::var("FZF_PROMPT").unwrap_or_default());
        let message = match arg(0) {
            "sort" => {
                view.sort = match view.sort {
                    Sort::Priority => Sort::Due,
                    Sort::Due => Sort::Added,
                    Sort::Added => Sort::Priority,
                };
                String::new()
            }
            "reverse" => {
                view.reverse = !view.reverse;
                String::new()
            }
            "closed" => {
                view.state = match view.state {
                    State::Open => State::All,
                    State::All => State::Closed,
                    State::Closed => State::Open,
                };
                String::new()
            }
            "due-filter" => {
                view.due = match view.due {
                    None => Some(DueFilter::Overdue),
                    Some(DueFilter::Overdue) => Some(DueFilter::Today),
                    Some(DueFilter::Today) => Some(DueFilter::Week),
                    Some(DueFilter::Week) => Some(DueFilter::None),
                    Some(DueFilter::None) => None,
                };
                String::new()
            }
            "group" => {
                view.group = match view.group {
                    None => Some(Group::Note),
                    Some(Group::Note) => Some(Group::Tag),
                    Some(Group::Tag) => Some(Group::Due),
                    Some(Group::Due) => None,
                };
                String::new()
            }
            "refresh" => {
                if let Some(tag) = self.take_tag_filter() {
                    view.tag = (!tag.is_empty()).then_some(tag);
                }
                self.take_status()
            }
            "undo" => self.undo_message()?,
            action => self.act_on_line(action, arg(2), arg(3), arg(4))?,
        };
        let exe = self.self_command()?;
        let header = if message.is_empty() {
            self.keys_header()
        } else {
            // The outcome on a line of its own above the key help, so neither
            // is cut off in a narrow window.
            format!("{message}\n{}", self.keys_header())
        };
        // change-header takes the rest of the output, newlines included, so
        // it goes last.
        println!(
            "change-prompt({})+reload({exe} task-lines {} {} {{q}})+change-header:{header}",
            self.view_prompt(&view),
            view.kind(),
            shell_quote(&view.encode())
        );
        Ok(())
    }

    /// `task-tag-filter KIND`, run by fzf's `execute`: picks one of the tags
    /// the tasks in view are filed under (the view without its tag filter)
    /// and leaves it for the list's refresh to apply; the first row clears
    /// the filter. Esc leaves the view as it is.
    fn task_tag_filter(&self, args: &[String]) -> Result<()> {
        let mut view = View::from_prompt(
            args.first().map(String::as_str) == Some("inbox"),
            &env::var("FZF_PROMPT").unwrap_or_default(),
        );
        view.tag = None;
        let scanner = self.scanner()?;
        let (shown, _) = self.view_tasks(&scanner, &view)?;
        let mut rows = vec![self.msg.all_tags.to_string()];
        rows.extend(tasks::tags_in(&shown).iter().map(|t| format!("#{t}")));
        let prompt = format!("--prompt={}", self.msg.prompt_tag);
        let Some(out) = self.fzf(&(rows.join("\n") + "\n"), &["--no-sort", &prompt])? else {
            return Ok(());
        };
        let picked = out.first().map_or("", String::as_str);
        let tag = if picked == self.msg.all_tags {
            ""
        } else {
            picked.trim_start_matches('#')
        };
        if let Some(p) = self.status_path() {
            if let Some(dir) = p.parent() {
                let _ = fs::create_dir_all(dir);
            }
            let _ = fs::write(p.with_file_name("tag-filter"), tag);
        }
        Ok(())
    }

    fn take_tag_filter(&self) -> Option<String> {
        let p = self.status_path()?.with_file_name("tag-filter");
        let tag = fs::read_to_string(&p).ok()?;
        let _ = fs::remove_file(p);
        Some(tag.trim().to_string())
    }

    /// Done, cancel or tag on one listed line. Refuses when the line is no
    /// longer the one the list showed. Returns the message for the header.
    fn act_on_line(&self, action: &str, rel: &str, lineno: &str, hash: &str) -> Result<String> {
        let Ok(lineno) = lineno.parse::<usize>() else {
            return Ok(self.msg.no_selection.to_string());
        };
        let tag = self.cfg.tasks.require_tag.clone();
        let now = now();
        self.rewrite_listed(rel, lineno, hash, |line| match action {
            "done" => task::toggle_line(line, now),
            "cancel" => task::cancel_line(line, now.date()),
            "tag" => {
                let Some(tag) = &tag else {
                    return Err(task::Refusal::NothingToTag);
                };
                // From a list, only an open or done task is tagged (tag_at).
                match task::checkbox(line) {
                    Some((_, b' ' | b'x' | b'X')) => task::tag_line(line, tag, &self.cfg.tasks.new_checkbox),
                    _ => Err(task::Refusal::NoCheckbox),
                }
            }
            _ => Err(task::Refusal::NoCheckbox),
        })
        .map(|r| match (action, &tag, r) {
            ("tag", None, Err(_)) => self.msg.no_require_tag.to_string(),
            (_, _, Ok(m)) | (_, _, Err(m)) => m,
        })
    }

    /// Rewrites line `lineno` of `rel` if its fingerprint is still `hash`,
    /// records it for undo, and returns Ok(message) on success or Err(message)
    /// when nothing was written.
    fn rewrite_listed(
        &self,
        rel: &str,
        lineno: usize,
        hash: &str,
        rewrite: impl FnOnce(&str) -> Result<String, task::Refusal>,
    ) -> Result<Result<String, String>> {
        let path = self.home.join(rel);
        if let Some(why) = self.blocked(&path) {
            return Ok(Err(why));
        }
        let mut lines = fileio::read_lines(&path)?;
        let Some(line) = lines.get(lineno.wrapping_sub(1)).cloned() else {
            return Ok(Err(fill(self.msg.line_changed, &[&rel])));
        };
        if tasks::line_hash(&line) != hash {
            return Ok(Err(fill(self.msg.line_changed, &[&rel])));
        }
        let out = match rewrite(&line) {
            Ok(out) => out,
            Err(why) => return Ok(Err(self.refusal(why, ""))),
        };
        lines[lineno - 1] = out.clone();
        fileio::write_lines(&path, &lines).with_context(|| fill(self.msg.cannot_write, &[&path.display()]))?;
        self.remember(rel, lineno, Some(line), &out);
        Ok(Ok(fill(self.msg.written, &[&rel, &lineno, &trim(&out)])))
    }

    fn refusal(&self, why: task::Refusal, detail: &str) -> String {
        use task::Refusal::*;
        match why {
            NoCheckbox => self.msg.refused_no_checkbox.to_string(),
            Cancelled => self.msg.refused_cancelled.to_string(),
            Done => self.msg.refused_done.to_string(),
            NotOpen => self.msg.refused_not_open.to_string(),
            BadDate => fill(self.msg.refused_bad_date, &[&detail]),
            NothingToTag => self.msg.refused_nothing_to_tag.to_string(),
        }
    }

    /// `task-due REL LINENO HASH [SPEC]`, run by fzf's `execute`: asks for a due
    /// date and sets it (TaskDue). The outcome goes to the status file, which
    /// the list's refresh shows.
    fn task_due(&self, args: &[String]) -> Result<()> {
        let arg = |i: usize| args.get(i).map_or("", String::as_str);
        let Ok(lineno) = arg(1).parse::<usize>() else {
            self.set_status(self.msg.no_selection);
            return Ok(());
        };
        // A fourth argument gives the date without asking (for scripts).
        let spec = match args.get(3) {
            Some(spec) => spec.clone(),
            None => match self.input(self.msg.prompt_due)? {
                Some(spec) => spec,
                None => return Ok(()),
            },
        };
        let message = match self.set_due(arg(0), lineno, arg(2), &spec)? {
            Ok(m) | Err(m) => m,
        };
        self.set_status(&message);
        Ok(())
    }

    /// TaskDue on a listed line: `spec` is resolved (blank clears).
    fn set_due(&self, rel: &str, lineno: usize, hash: &str, spec: &str) -> Result<Result<String, String>> {
        let spec = trim(spec);
        let date = if spec.is_empty() {
            String::new()
        } else {
            match task::resolve_due(spec, now().date()) {
                Some(d) => d,
                None => return Ok(Err(self.refusal(task::Refusal::BadDate, spec))),
            }
        };
        let unchanged = if spec.is_empty() {
            self.msg.no_due_to_clear
        } else {
            self.msg.due_unchanged
        };
        let mut same = false;
        let r = self.rewrite_listed(rel, lineno, hash, |line| {
            let out = task::due_line(line, &date)?;
            same = out == line;
            // An unchanged line is not written; report it as a refusal.
            if same {
                Err(task::Refusal::NothingToTag)
            } else {
                Ok(out)
            }
        })?;
        Ok(match r {
            Err(_) if same => Err(unchanged.to_string()),
            other => other,
        })
    }

    /// The guided capture (`capture-guided`, alt-a in the list): the text,
    /// then tags picked from the collection, then a due date.
    fn capture_guided(&self) -> Result<Option<String>> {
        let Some(rel) = self.cfg.tasks.capture_target() else {
            bail!(self.msg.no_capture_note);
        };
        let Some(text) = self.input(self.msg.prompt_task)? else {
            return Ok(None);
        };
        if trim(&text).is_empty() {
            return Ok(None);
        }
        let tags = self.collect_tags()?;
        let picked = if tags.is_empty() {
            Vec::new()
        } else {
            let list: String = tags.iter().map(|t| format!("{t}\n")).collect();
            match self.fzf(&list, &["--multi", &format!("--prompt={}", self.msg.prompt_tags)])? {
                Some(p) => p,
                // Esc on the tags cancels the capture, as in the plugin.
                None => return Ok(None),
            }
        };
        let body = task::with_tags(trim(&text), &picked);
        let spec = self.input(self.msg.prompt_due)?.unwrap_or_default();
        let mut notes = Vec::new();
        let due = if trim(&spec).is_empty() {
            None
        } else {
            let d = task::resolve_due(&spec, now().date());
            if d.is_none() {
                notes.push(fill(self.msg.due_unrecognised, &[&trim(&spec)]));
            }
            d
        };
        let captured = self.capture_text(rel, &body, due.as_deref())?;
        notes.extend(captured);
        Ok(Some(notes.join("  ")))
    }

    /// Appends a task to the capture note and records it for undo.
    fn capture_text(&self, rel: &str, text: &str, due: Option<&str>) -> Result<Option<String>> {
        let path = self.home.join(rel);
        self.guard(&path)?;
        match task::capture(&self.cfg, &path, text, due)
            .with_context(|| fill(self.msg.cannot_write, &[&path.display()]))?
        {
            Some((line, lineno)) => {
                self.remember(rel, lineno, None, &line);
                Ok(Some(fill(self.msg.captured, &[&rel, &line])))
            }
            None => Ok(Some(self.msg.nothing_captured.to_string())),
        }
    }

    /// Every `#tag` in the collection, sorted, with the required tag first
    /// (`collect_tags`).
    fn collect_tags(&self) -> Result<Vec<String>> {
        let glob = format!("*.{}", self.cfg.extension);
        let out = Command::new("rg")
            .args([
                "--no-filename",
                "--no-messages",
                "--no-line-number",
                "-o",
                "--glob",
                &glob,
            ])
            .args(["-e", "#[A-Za-z0-9_-]+"])
            .current_dir(&self.home)
            .output()
            .with_context(|| fill(self.msg.cannot_start, &[&"rg"]))?;
        let text = String::from_utf8_lossy(&out.stdout);
        let front = self.cfg.tasks.require_tag.as_ref().map(|t| format!("#{t}"));
        let mut set: Vec<String> = text
            .lines()
            .filter(|t| Some(*t) != front.as_deref())
            .map(str::to_string)
            .collect();
        set.sort();
        set.dedup();
        if let Some(f) = front {
            set.insert(0, f);
        }
        Ok(set)
    }

    /// TaskTag over picked lines (tab marks several): each scannable line is
    /// made a task carrying `require_tag`, with a new id when `on_tag`.
    fn tag_lines(&self) -> Result<()> {
        let Some(tag) = self.cfg.tasks.require_tag.clone() else {
            bail!(self.msg.no_require_tag);
        };
        let picked = self.pick_lines(self.msg.prompt_line, true)?;
        if picked.is_empty() {
            return Ok(());
        }
        let scanner =
            Scanner::new(&self.cfg).map_err(|e| anyhow!(fill(self.msg.bad_pattern, &[&"tasks.headings", &e])))?;
        let mut by_note: Vec<(String, Vec<(usize, String)>)> = Vec::new();
        for (rel, idx, listed) in picked {
            match by_note.iter_mut().find(|(r, _)| *r == rel) {
                Some((_, v)) => v.push((idx, listed)),
                None => by_note.push((rel, vec![(idx, listed)])),
            }
        }
        let b = &self.cfg.block_id;
        let mut tagged = 0;
        for (rel, picks) in by_note {
            let path = self.home.join(&rel);
            let mut lines = fileio::read_lines(&path)?;
            let scannable = scanner.scannable_lines(&lines);
            let mut taken = blockid::ids_in(&lines);
            let mut changed = Vec::new();
            for (idx, listed) in picks {
                if lines.get(idx) != Some(&listed) {
                    eprintln!("ttykasten: {}", fill(self.msg.line_changed, &[&rel]));
                    continue;
                }
                if !scannable[idx] {
                    continue;
                }
                let Ok(mut out) = task::tag_line(&listed, &tag, &self.cfg.tasks.new_checkbox) else {
                    continue;
                };
                if b.on_tag && blockid::read(&out).is_none() {
                    let id = blockid::mint(&taken, b.length, &b.alphabet);
                    taken.insert(id.clone());
                    out = blockid::with_id(&out, &id);
                }
                lines[idx] = out.clone();
                changed.push((idx, listed, out));
            }
            if changed.is_empty() {
                continue;
            }
            self.guard(&path)?;
            fileio::write_lines(&path, &lines).with_context(|| fill(self.msg.cannot_write, &[&path.display()]))?;
            for (idx, before, after) in changed {
                self.remember(&rel, idx + 1, Some(before), &after);
                println!("{}", fill(self.msg.written, &[&rel, &(idx + 1), &trim(&after)]));
                tagged += 1;
            }
        }
        if tagged == 0 {
            eprintln!("ttykasten: {}", self.msg.refused_nothing_to_tag);
        }
        Ok(())
    }

    /// Mint ids (`:FzfKastenBlockIds`): every scannable checkbox in a picked
    /// note that has none and carries `require_tag` gets one.
    fn mint_ids(&self) -> Result<()> {
        let files = self.note_files("--no-ignore-vcs")?;
        let list: String = files.iter().map(|f| format!("{f}\n")).collect();
        let mut args = vec![
            "--scheme=path".to_string(),
            format!("--prompt={}", self.msg.prompt_note),
        ];
        args.extend(self.preview_arg("{}")?);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let Some(out) = self.fzf(&list, &args)? else {
            return Ok(());
        };
        let Some(rel) = out.first().filter(|r| !r.is_empty()).cloned() else {
            return Ok(());
        };
        let scanner =
            Scanner::new(&self.cfg).map_err(|e| anyhow!(fill(self.msg.bad_pattern, &[&"tasks.headings", &e])))?;
        let path = self.home.join(&rel);
        let mut lines = fileio::read_lines(&path)?;
        let scannable = scanner.scannable_lines(&lines);
        let b = &self.cfg.block_id;
        let tag = self.cfg.tasks.require_tag.as_deref();
        let mut taken = blockid::ids_in(&lines);
        let mut changed = Vec::new();
        for (i, line) in lines.iter_mut().enumerate() {
            let eligible = scannable[i]
                && task::checkbox(line).is_some()
                && blockid::read(line).is_none()
                && tag.is_none_or(|t| task::has_tag(line, t));
            if eligible {
                let id = blockid::mint(&taken, b.length, &b.alphabet);
                taken.insert(id.clone());
                let out = blockid::with_id(line, &id);
                changed.push((i, line.clone(), out.clone()));
                *line = out;
            }
        }
        if !changed.is_empty() {
            self.guard(&path)?;
            fileio::write_lines(&path, &lines).with_context(|| fill(self.msg.cannot_write, &[&path.display()]))?;
            for (i, before, after) in &changed {
                self.remember(&rel, i + 1, Some(before.clone()), after);
            }
        }
        println!("{}", fill(self.msg.minted_n, &[&changed.len()]));
        Ok(())
    }

    /// `{{agenda}}` and `{{agenda_week}}` for a template filled at `time`.
    fn agenda(&self, key: &str, time: NaiveDateTime) -> Option<String> {
        let cal = &self.cfg.calendar;
        match key {
            "agenda" => Some(calendar::day_lines(cal, time.date()).join("\n")),
            "agenda_week" => {
                let r = week::range("", time.date()).ok()?;
                Some(calendar::range_lines(cal, &r.from, &r.to).0.join("\n"))
            }
            _ => None,
        }
    }

    fn week_range(&self, spec: &str) -> Result<week::Range> {
        week::range(spec, now().date()).map_err(|e| anyhow!(e))
    }

    /// The notes dated within a week (§11): the finder's notes, less ignored
    /// dirs and patterns and the week's own weekly note, by date then path.
    fn week_notes(&self, range: &week::Range) -> Result<Vec<week::WeekNote>> {
        let w = &self.cfg.week;
        let patterns = w
            .ignore_patterns
            .iter()
            .map(|p| regex::Regex::new(p))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow!(fill(self.msg.bad_pattern, &[&"week.ignore_patterns", &e])))?;
        let scanner =
            Scanner::new(&self.cfg).map_err(|e| anyhow!(fill(self.msg.bad_pattern, &[&"tasks.date_patterns", &e])))?;
        let monday = range.monday.and_hms_opt(12, 0, 0).context("monday")?;
        let weekly = Periodic::Weekly.rel(&self.cfg, monday);
        let mut notes = Vec::new();
        for rel in self.note_files("--no-ignore-vcs")? {
            let in_dir = w
                .ignore_dirs
                .iter()
                .any(|d| rel == *d || rel.strip_prefix(d.as_str()).is_some_and(|r| r.starts_with('/')));
            if in_dir || patterns.iter().any(|p| p.is_match(&rel)) || rel == weekly {
                continue;
            }
            let Ok(lines) = fileio::read_lines(&self.home.join(&rel)) else {
                continue;
            };
            if let Some(date) = scanner.date_of(&rel, &lines)
                && date >= range.from
                && date <= range.to
            {
                let name = link::note_name(Path::new(&rel));
                notes.push(week::WeekNote { rel, date, name, lines });
            }
        }
        notes.sort_by(|a, b| a.date.cmp(&b.date).then(a.rel.cmp(&b.rel)));
        Ok(notes)
    }

    /// The notes of a week in fzf; enter opens one, ctrl-d shows the digest.
    fn week(&self, spec: &str) -> Result<()> {
        let range = self.week_range(spec)?;
        let notes = self.week_notes(&range)?;
        if notes.is_empty() {
            eprintln!(
                "ttykasten: {}",
                fill(self.msg.no_notes_in_week, &[&range.label, &range.from, &range.to])
            );
            return Ok(());
        }
        let ext = format!(".{}", self.cfg.extension);
        let items: String = notes
            .iter()
            .map(|n| {
                format!(
                    "{}\t{}  {}\n",
                    n.rel,
                    week::day_label(&n.date),
                    n.rel.strip_suffix(&ext).unwrap_or(&n.rel)
                )
            })
            .collect();
        let mut args = vec![
            "--delimiter=\t".to_string(),
            "--with-nth=2..".into(),
            "--no-sort".into(),
            format!("--prompt={}> ", range.label),
            format!(
                "--header={}{}",
                fill(self.msg.week_header, &[&range.from, &range.to]),
                key_hint(self.msg.week_digest_hint, &self.cfg.week.digest_key)
            ),
        ];
        args.extend(expect(&self.cfg.week.digest_key));
        args.extend(self.preview_arg("{1}")?);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let Some(out) = self.fzf(&items, &args)? else {
            return Ok(());
        };
        if pressed(&out, &self.cfg.week.digest_key) {
            return self.digest(&range.from);
        }
        match out.get(1).and_then(|l| l.split('\t').next()) {
            Some(rel) if !rel.is_empty() => self.edit(&self.home.join(rel)),
            _ => Ok(()),
        }
    }

    /// The calendar's events of a week or a span of weeks (`agenda`), one row
    /// each, with a row for today. Enter opens the daily note of the row's
    /// day; `calendar.keys.copy` copies the picked events' lines and
    /// `calendar.keys.refresh` runs the command again.
    fn agenda_list(&self, spec: &str) -> Result<()> {
        let today = now().date();
        let range = calendar::span(spec, today).map_err(|e| anyhow!(e))?;
        let cal = &self.cfg.calendar;
        let k = &cal.keys;
        let mut refresh = false;
        loop {
            let until = (NaiveDate::parse_from_str(&range.to, "%Y-%m-%d")? + chrono::Duration::days(1))
                .format("%Y-%m-%d")
                .to_string();
            let f = calendar::fetch_again(cal, &range.from, &until, refresh)
                .map_err(|e| anyhow!("{}: {e}", cal.labels.unavailable))?;
            let stale = calendar::staleness(&f);
            if f.events.is_empty() {
                eprintln!(
                    "ttykasten: {}",
                    fill(self.msg.agenda_empty, &[&range.label, &range.from, &range.to, &stale])
                );
                return Ok(());
            }
            let today_s = today.format("%Y-%m-%d").to_string();
            let rows = calendar::rows(cal, &f.events, &today_s, self.msg.agenda_today);
            let lines: String = rows
                .iter()
                .enumerate()
                .map(|(i, (_, text, past))| {
                    if *past {
                        format!("{i}\t\x1b[2m{text}\x1b[0m\n")
                    } else {
                        format!("{i}\t{text}\n")
                    }
                })
                .collect();
            let header = format!(
                "{}{}{}{}",
                fill(self.msg.agenda_header, &[&range.from, &range.to, &stale]),
                self.msg.agenda_open_hint,
                key_hint(self.msg.agenda_copy_hint, &k.copy),
                key_hint(self.msg.agenda_refresh_hint, &k.refresh)
            );
            let mut args = vec![
                "--delimiter=\t".to_string(),
                "--with-nth=2..".into(),
                "--ansi".into(),
                "--no-sort".into(),
                "--multi".into(),
                format!("--prompt={} agenda> ", range.label),
                format!("--header={header}"),
            ];
            let keys: Vec<&str> = [k.copy.as_str(), k.refresh.as_str()]
                .into_iter()
                .filter(|k| !k.is_empty())
                .collect();
            if !keys.is_empty() {
                args.push(format!("--expect={}", keys.join(",")));
            }
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let Some(out) = self.fzf(&lines, &args)? else {
                return Ok(());
            };
            let (key, picked) = if keys.is_empty() {
                ("", &out[..])
            } else {
                (out[0].as_str(), &out[1..])
            };
            let picked: Vec<&calendar::Row> = picked
                .iter()
                .filter_map(|l| l.split('\t').next()?.parse::<usize>().ok())
                .filter_map(|i| rows.get(i).map(|(r, _, _)| r))
                .collect();
            if pressed(&out, &k.refresh) {
                refresh = true;
                continue;
            }
            if !key.is_empty() && key == k.copy {
                let text: Vec<String> = picked
                    .iter()
                    .filter_map(|r| match r {
                        calendar::Row::Event(i) => {
                            Some(format!("{}{}", cal.bullet, calendar::format(cal, &f.events[*i])))
                        }
                        calendar::Row::Today => None,
                    })
                    .collect();
                if !text.is_empty() {
                    self.copy(&(text.join("\n") + "\n"));
                    println!("{}", fill(self.msg.copied_events, &[&text.len()]));
                }
                return Ok(());
            }
            let date = match picked.first() {
                Some(calendar::Row::Event(i)) => NaiveDate::parse_from_str(&f.events[*i].date, "%Y-%m-%d")?,
                Some(calendar::Row::Today) => today,
                None => return Ok(()),
            };
            return self.open_periodic(Periodic::Daily, date.and_time(now().time()));
        }
    }

    /// The week digest (§11), coloured in the pager on a terminal, plain
    /// Markdown otherwise.
    fn digest(&self, spec: &str) -> Result<()> {
        let lines = self.digest_lines(spec)?;
        let pager = self.cfg.pager.trim();
        if pager.is_empty() || !std::io::stdout().is_terminal() {
            let mut out = std::io::stdout().lock();
            for l in &lines {
                let _ = writeln!(out, "{l}");
            }
            return Ok(());
        }
        let coloured = preview::render(&lines, &self.cfg.preview.colors, None);
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(pager)
            .stdin(Stdio::piped())
            .spawn()
            .with_context(|| fill(self.msg.cannot_start, &[&pager]))?;
        if let Some(mut w) = child.stdin.take() {
            let _ = w.write_all(coloured.as_bytes());
        }
        let _ = child.wait();
        Ok(())
    }

    fn digest_lines(&self, spec: &str) -> Result<Vec<String>> {
        let range = self.week_range(spec)?;
        let notes = self.week_notes(&range)?;
        let d = &self.cfg.week.digest;
        let tasks = if d.tasks {
            let scanner = Scanner::new(&self.cfg).map_err(|e| {
                anyhow!(fill(
                    self.msg.bad_pattern,
                    &[&"tasks.date_patterns / tasks.headings", &e]
                ))
            })?;
            let mut all = Vec::new();
            for rel in self.checkbox_notes()? {
                if let Ok(lines) = fileio::read_lines(&self.home.join(&rel)) {
                    scanner.scan_note(&rel, &lines, None, &mut all);
                }
            }
            tasks::select(
                all,
                Want {
                    inbox: false,
                    done: true,
                    cancelled: false,
                },
                &tasks::Filter::default(),
                Sort::Added,
                false,
            )
        } else {
            Vec::new()
        };
        let ahead = (d.ahead > 0).then(|| week::ahead_of(&range, d.ahead));
        let sources = d
            .sources
            .iter()
            .map(|s| {
                let cmd = s
                    .command
                    .replace("{from}", &range.from)
                    .replace("{to}", &range.to)
                    .replace("{ahead_from}", ahead.as_ref().map_or("", |a| a.from.as_str()))
                    .replace("{ahead_to}", ahead.as_ref().map_or("", |a| a.to.as_str()));
                (s.label.clone(), self.source_lines(&cmd))
            })
            .collect();
        let cal = &self.cfg.calendar;
        let extras = week::Extras {
            calendar: cal.enabled.then(|| {
                Box::new(move |from: &str, to: &str| calendar::range_lines(cal, from, to)) as week::CalendarFn
            }),
            sources,
        };
        let monday = range.monday.and_hms_opt(12, 0, 0).context("monday")?;
        let weekly_name = Periodic::Weekly.name(&self.cfg, monday);
        Ok(week::digest_lines(d, &range, &weekly_name, &notes, &tasks, &extras))
    }

    /// A digest source's output, trailing blank lines dropped, or an
    /// `(unavailable: …)` line when the command fails.
    fn source_lines(&self, command: &str) -> Vec<String> {
        let unavailable = &self.cfg.week.digest.labels.unavailable;
        let out = Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(&self.home)
            .stdin(Stdio::null())
            .output();
        match out {
            Ok(o) if o.status.success() => {
                let mut lines: Vec<String> = String::from_utf8_lossy(&o.stdout).lines().map(str::to_string).collect();
                while lines.last().is_some_and(|l| trim(l).is_empty()) {
                    lines.pop();
                }
                lines
            }
            Ok(o) => {
                let err = String::from_utf8_lossy(&o.stderr);
                let first = err.lines().next().unwrap_or("").to_string();
                let code = o.status.code().unwrap_or(-1);
                vec![format!(
                    "({unavailable}: exited {code}{})",
                    if first.is_empty() {
                        String::new()
                    } else {
                        format!(": {first}")
                    }
                )]
            }
            Err(e) => vec![format!("({unavailable}: {e})")],
        }
    }

    /// Why `path` must not be written now: it is open in a running Neovim
    /// with unsaved changes. Writing would lose one side's edits on `:w`.
    fn blocked(&self, path: &Path) -> Option<String> {
        let dir = self.cfg.swap_dir()?;
        let pid = nvimswap::unsaved_pid(&dir, path)?;
        let rel = path.strip_prefix(&self.home).unwrap_or(path);
        Some(fill(self.msg.unsaved_in_nvim, &[&rel.display(), &pid]))
    }

    /// Fails with the reason when `path` must not be written now.
    fn guard(&self, path: &Path) -> Result<()> {
        match self.blocked(path) {
            Some(why) => bail!(why),
            None => Ok(()),
        }
    }

    fn journal(&self) -> Option<Journal> {
        Journal::default_path().map(Journal::new)
    }

    /// Records a write for undo. A journal that can't be written costs only
    /// the undo, so it is reported and the edit stands.
    fn remember(&self, rel: &str, lineno: usize, before: Option<String>, after: &str) {
        let Some(j) = self.journal() else {
            return;
        };
        let entry = Entry {
            home: self.home.to_string_lossy().into(),
            rel: rel.to_string(),
            lineno,
            before,
            after: after.to_string(),
        };
        if let Err(e) = j.record(entry) {
            eprintln!("ttykasten: undo journal: {e}");
        }
    }

    fn undo_message(&self) -> Result<String> {
        let Some(j) = self.journal() else {
            return Ok(self.msg.undo_nothing.to_string());
        };
        Ok(match j.undo(&self.home, |p| self.blocked(p).is_some())? {
            Undo::Nothing => self.msg.undo_nothing.to_string(),
            Undo::Blocked(e) => self.blocked(&self.home.join(&e.rel)).unwrap_or_default(),
            Undo::Changed(e) => fill(self.msg.undo_changed, &[&e.rel, &e.lineno]),
            Undo::Undone(e) => fill(self.msg.undone, &[&e.rel, &e.lineno]),
        })
    }

    /// A message left for the task list by a command fzf ran with `execute`,
    /// which has no other way back to the list's header.
    fn status_path(&self) -> Option<PathBuf> {
        Journal::default_path().map(|p| p.with_file_name("status"))
    }

    fn set_status(&self, message: &str) {
        if let Some(p) = self.status_path() {
            if let Some(dir) = p.parent() {
                let _ = fs::create_dir_all(dir);
            }
            let _ = fs::write(p, message);
        }
    }

    fn take_status(&self) -> String {
        let Some(p) = self.status_path() else {
            return String::new();
        };
        let message = fs::read_to_string(&p).unwrap_or_default();
        let _ = fs::remove_file(p);
        message.replace(['\n', '\r'], " ")
    }

    /// The character that starts a romaji query, None when it is "". One
    /// character, since fzf binds it as a key and puts it back.
    fn romaji_prefix(&self) -> Result<Option<&str>> {
        let p = self.cfg.romaji.prefix.as_str();
        let mut chars = p.chars();
        match (chars.next(), chars.next()) {
            (None, _) => Ok(None),
            (Some(c), None) if !c.is_whitespace() && !",:+()'\"\\".contains(c) => Ok(Some(p)),
            _ => bail!(fill(self.msg.bad_romaji_prefix, &[&p])),
        }
    }

    /// This executable, quoted for a shell command line.
    fn self_command(&self) -> Result<String> {
        let exe = env::current_exe().context("current_exe")?;
        Ok(format!("'{}'", exe.to_string_lossy().replace('\'', r"'\''")))
    }

    /// Every note file, relative to home. `ignore` is rg's flag for which ignore
    /// files to disregard: `--no-ignore-vcs` for the finder's view of the
    /// collection, `--no-ignore` for the plugin's glob (§2.2).
    fn note_files(&self, ignore: &str) -> Result<Vec<String>> {
        let glob = format!("*.{}", self.cfg.extension);
        let out = Command::new("rg")
            .args(["--files", "--no-messages", ignore, "--glob", &glob])
            .current_dir(&self.home)
            .output()
            .with_context(|| fill(self.msg.cannot_start, &[&"rg"]))?;
        let text = String::from_utf8_lossy(&out.stdout);
        Ok(text.lines().map(str::to_string).collect())
    }

    /// Lists every non-blank line of every note in fzf and returns the one
    /// picked, as (path relative to home, 0-based index, the line as listed).
    fn pick_line(&self, prompt: &str) -> Result<Option<(String, usize, String)>> {
        Ok(self.pick_lines(prompt, false)?.into_iter().next())
    }

    /// Like `pick_line`; with `multi`, tab marks several lines.
    fn pick_lines(&self, prompt: &str, multi: bool) -> Result<Vec<(String, usize, String)>> {
        let mut picked: Vec<(String, usize, String)> = Vec::new();
        let mut items = String::new();
        for rel in self.note_files("--no-ignore-vcs")? {
            let Ok(lines) = fileio::read_lines(&self.home.join(&rel)) else {
                continue;
            };
            for (i, line) in lines.into_iter().enumerate() {
                if trim(&line).is_empty() {
                    continue;
                }
                let shown = line.replace('\t', " ");
                items.push_str(&format!(
                    "{}\t{rel}\t{}\t{rel}:{}: {shown}\n",
                    picked.len(),
                    i + 1,
                    i + 1
                ));
                picked.push((rel.clone(), i, line));
            }
        }
        let mut args = vec![
            "--delimiter=\t".to_string(),
            "--with-nth=4..".into(),
            format!("--prompt={prompt}"),
            "--preview-window=+{3}-/2".into(),
        ];
        if multi {
            args.push("--multi".into());
        }
        args.extend(self.preview_arg("{2} {3}")?);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let Some(out) = self.fzf(&items, &args)? else {
            return Ok(Vec::new());
        };
        let chosen: Vec<usize> = out
            .iter()
            .filter_map(|l| l.split('\t').next())
            .filter_map(|n| n.parse::<usize>().ok())
            .collect();
        Ok(chosen.into_iter().filter_map(|n| picked.get(n).cloned()).collect())
    }

    /// Picks a line and makes a link to it (YankLink, §9.3): the line's id is
    /// reused, or a new one is written into the note. The link is copied.
    fn link_line(&self) -> Result<Option<String>> {
        let Some((rel, idx, listed)) = self.pick_line(self.msg.prompt_line)? else {
            return Ok(None);
        };
        let path = self.home.join(&rel);
        let mut lines = fileio::read_lines(&path)?;
        // The note may have been edited while the list was open.
        if lines.get(idx) != Some(&listed) {
            bail!(fill(self.msg.line_changed, &[&rel]));
        }
        let b = &self.cfg.block_id;
        let alias = b.alias.then_some((b.alias_max > 0).then_some(b.alias_max));
        let name = link::note_name(&path);
        let Some(made) = link::block_link(&lines, idx, &name, b.length, &b.alphabet, alias) else {
            bail!(self.msg.blank_line);
        };
        if let Some(new_line) = made.new_line {
            self.guard(&path)?;
            lines[idx] = new_line;
            fileio::write_lines(&path, &lines).with_context(|| fill(self.msg.cannot_write, &[&path.display()]))?;
        }
        self.copy(&made.link);
        Ok(Some(made.link))
    }

    fn link(&self) -> Result<()> {
        if let Some(link) = self.link_line()? {
            println!("{link}");
        }
        Ok(())
    }

    /// LinkToDaily (§9.4): a link to a line, appended to today's daily note,
    /// which is made from its template first if it does not exist yet.
    fn link_daily(&self) -> Result<()> {
        let Some(link) = self.link_line()? else {
            return Ok(());
        };
        let cfg = &self.cfg;
        let (kind, now) = (Periodic::Daily, now());
        let rel = kind.rel(cfg, now);
        let path = self.home.join(&rel);
        let mut lines = if path.exists() {
            fileio::read_lines(&path)?
        } else {
            fs::create_dir_all(self.home.join(kind.dir(cfg)))?;
            template::load(
                &self.home,
                &cfg.hdate_format,
                kind.template(cfg),
                &kind.name(cfg, now),
                now,
                &|k| self.agenda(k, now),
            )
            .split('\n')
            .map(str::to_string)
            .collect()
        };
        lines.push(format!("{}{link}", cfg.block_id.daily_bullet));
        self.guard(&path)?;
        fileio::write_lines(&path, &lines).with_context(|| fill(self.msg.cannot_write, &[&path.display()]))?;
        println!("{}", fill(self.msg.linked_into, &[&link, &rel, &lines.len()]));
        Ok(())
    }

    /// Picks a line and lists the lines that link to it: to the line itself
    /// when it has an id (`[[note#^id]]`, and `[[#^id]]` inside the note), or
    /// else to its note (§10, where the note's own lines are left out).
    fn backlinks(&self) -> Result<()> {
        let Some((rel, _, listed)) = self.pick_line(self.msg.prompt_backlinks)? else {
            return Ok(());
        };
        let name = link::note_name(Path::new(&rel));
        let id = blockid::read(&listed).map(str::to_string);
        let target = match &id {
            Some(id) => format!("[[{name}#^{id}]]"),
            None => format!("[[{name}]]"),
        };
        let glob = format!("*.{}", self.cfg.extension);
        let out = Command::new("rg")
            .args([
                "--no-ignore",
                "--no-messages",
                "--null",
                "--line-number",
                "--no-heading",
            ])
            .args(["--color=never", "--fixed-strings", "--glob", &glob, "[["])
            .current_dir(&self.home)
            .output()
            .with_context(|| fill(self.msg.cannot_start, &[&"rg"]))?;
        let text = String::from_utf8_lossy(&out.stdout);
        let ext = &self.cfg.extension;
        let mut items = String::new();
        for record in text.lines() {
            let Some((file, rest)) = record.split_once('\0') else {
                continue;
            };
            let Some((lineno, line)) = rest.split_once(':') else {
                continue;
            };
            let same = file == rel;
            if (id.is_none() && same) || !link::links_to(line, ext, &name, id.as_deref(), same) {
                continue;
            }
            let shown = trim(line).replace('\t', " ");
            items.push_str(&format!("{file}\t{lineno}\t{file}:{lineno}: {shown}\n"));
        }
        if items.is_empty() {
            eprintln!("ttykasten: {}", fill(self.msg.no_backlinks, &[&target]));
            return Ok(());
        }
        let mut args = vec![
            "--delimiter=\t".to_string(),
            "--with-nth=3..".into(),
            format!("--prompt={}", self.msg.prompt_backlinks),
            format!("--header={target}"),
            "--preview-window=+{2}-/2".into(),
        ];
        args.extend(self.preview_arg("{1} {2}")?);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let Some(out) = self.fzf(&items, &args)? else {
            return Ok(());
        };
        let mut fields = out.first().map(|l| l.split('\t')).into_iter().flatten();
        match (fields.next(), fields.next().and_then(|n| n.parse::<usize>().ok())) {
            (Some(file), Some(lineno)) => self.edit_at(&self.home.join(file), lineno),
            _ => Ok(()),
        }
    }

    /// Prints a note in colour. Prints nothing if it can't be read (a daily note not yet made).
    /// `line` (1-based) is shown in the mark colour.
    fn preview(&self, rel: &str, line: Option<usize>) -> Result<()> {
        if let Ok(lines) = fileio::read_lines(&self.home.join(rel)) {
            let mut out = std::io::stdout().lock();
            let mark = line.and_then(|l| l.checked_sub(1));
            // Writing fails when fzf stops reading; that is fine.
            let _ = out.write_all(preview::render(&lines, &self.cfg.preview.colors, mark).as_bytes());
        }
        Ok(())
    }

    /// The `--preview` argument that calls this executable. None when
    /// `fzf.preview = false`.
    fn preview_arg(&self, field: &str) -> Result<Option<String>> {
        if !self.cfg.fzf.preview {
            return Ok(None);
        }
        let exe = env::current_exe().context("current_exe")?;
        let quoted = exe.to_string_lossy().replace('\'', r"'\''");
        Ok(Some(format!("--preview='{quoted}' preview {field}")))
    }

    // ---- External commands ----

    /// The argument if given, else what is typed into the input line.
    fn arg_or_input(&self, arg: Option<String>, prompt: &str) -> Result<Option<String>> {
        match arg {
            Some(a) => Ok(Some(a)),
            None => self.input(prompt),
        }
    }

    /// A one-line input: fzf with no candidates, returning what was typed.
    /// None if cancelled with Esc.
    fn input(&self, prompt: &str) -> Result<Option<String>> {
        let out = self.run_fzf(
            Stdio::null(),
            None,
            &[
                &format!("--prompt={prompt}"),
                "--print-query",
                "--disabled",
                "--info=hidden",
                "--no-separator",
                "--height=1",
                "--layout=reverse",
            ],
        )?;
        Ok(out.and_then(|o| o.into_iter().next()))
    }

    /// Runs fzf over the given candidates.
    fn fzf(&self, items: &str, args: &[&str]) -> Result<Option<Vec<String>>> {
        self.run_fzf(Stdio::piped(), Some(items), args)
    }

    /// fzf's output as lines, or None if cancelled (Esc etc.). `fzf.opts` from the
    /// config come after ttykasten's arguments, so they win.
    fn run_fzf(&self, stdin: Stdio, items: Option<&str>, args: &[&str]) -> Result<Option<Vec<String>>> {
        let fzf = &self.cfg.fzf.command;
        let mut child = Command::new(fzf)
            .args(args)
            .args(&self.cfg.fzf.opts)
            .current_dir(&self.home)
            .stdin(stdin)
            .stdout(Stdio::piped())
            .spawn()
            .with_context(|| fill(self.msg.cannot_start, &[fzf]))?;
        if let Some(items) = items {
            let mut w = child.stdin.take().context("fzf stdin")?;
            // fzf may exit before reading everything, so write errors are ignored.
            let _ = w.write_all(items.as_bytes());
        }
        let out = child.wait_with_output()?;
        // 0: selected. 1: no match (--print-query still prints the query). 130: cancelled.
        match out.status.code() {
            Some(0) | Some(1) => {}
            Some(130) => return Ok(None),
            _ => bail!(fill(self.msg.failed, &[fzf, &out.status])),
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let lines: Vec<String> = text.lines().map(str::to_string).collect();
        Ok((!lines.is_empty()).then_some(lines))
    }

    /// Copies text as `clipboard` says: "osc52" asks the terminal (which also
    /// works over ssh and mosh), "none" does nothing, and anything else is a
    /// command that reads the text on stdin. A failure only costs the copy.
    fn copy(&self, text: &str) {
        match self.cfg.clipboard.as_str() {
            "none" | "" => {}
            "osc52" => {
                let seq = format!("\x1b]52;c;{}\x07", base64(text.as_bytes()));
                if let Ok(mut tty) = fs::OpenOptions::new().write(true).open("/dev/tty") {
                    let _ = tty.write_all(seq.as_bytes());
                }
            }
            command => {
                let child = Command::new("sh").arg("-c").arg(command).stdin(Stdio::piped()).spawn();
                if let Ok(mut child) = child {
                    if let Some(mut w) = child.stdin.take() {
                        let _ = w.write_all(text.as_bytes());
                    }
                    let _ = child.wait();
                }
            }
        }
    }

    /// Opens a file in the editor at a line (1-based), using `editor_line`.
    fn edit_at(&self, path: &Path, line: usize) -> Result<()> {
        if self.cfg.editor_line.is_empty() {
            return self.edit(path);
        }
        let at = self.cfg.editor_line.replace("{line}", &line.to_string());
        self.shell(&format!("{} {at} \"$@\"", self.cfg.editor()), &[path])
    }

    /// Opens a file in the editor.
    fn edit(&self, path: &Path) -> Result<()> {
        let editor = self.cfg.editor();
        self.shell(&format!("{editor} \"$@\""), &[path])
    }

    /// Runs a command with `sh -c` in home. `$EDITOR` may include arguments.
    fn shell(&self, command: &str, args: &[&Path]) -> Result<()> {
        let status = Command::new("sh")
            .arg("-c")
            .arg(command)
            .arg("sh")
            .args(args)
            .current_dir(&self.home)
            .status()
            .with_context(|| fill(self.msg.cannot_start, &[&command]))?;
        if !status.success() {
            bail!(fill(self.msg.failed, &[&command, &status]));
        }
        Ok(())
    }
}

/// `ttykasten 0.2.0 (39ef7e6)`: the version, and the commit when built from git.
fn version() -> String {
    let v = env!("CARGO_PKG_VERSION");
    match env!("TTYKASTEN_COMMIT") {
        "" => format!("ttykasten {v}"),
        c => format!("ttykasten {v} ({c})"),
    }
}

/// Standard base64 with padding, for OSC 52.
fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn now() -> NaiveDateTime {
    Local::now().naive_local()
}

#[cfg(test)]
mod tests {
    use super::{DueFilter, Group, Sort, State, View, base64};

    #[test]
    fn base64_encodes() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64("[[n#^a1]]".as_bytes()), "W1tuI15hMV1d");
    }

    #[test]
    fn view_round_trip() {
        let v = View {
            inbox: false,
            sort: Sort::Due,
            reverse: true,
            state: State::Closed,
            tag: Some("budget".into()),
            due: Some(DueFilter::Week),
            group: Some(Group::Tag),
        };
        assert_eq!(v.encode(), "due, reversed, #budget, due this week, closed only, by tag");
        let back = View::from_prompt(false, &format!("tasks [{}]> ", v.encode()));
        assert_eq!(back.encode(), v.encode());
        assert_eq!(View::from_prompt(true, "inbox [priority]> ").encode(), "priority");
    }
}

/// Tasks from a scan, and the notes' lines by path.
type Scanned = (Vec<tasks::Task>, HashMap<String, Vec<String>>);

/// The word for `v` in a table of view settings.
fn word<T: PartialEq>(table: &[(T, &str)], v: T) -> Option<String> {
    table.iter().find(|(x, _)| *x == v).map(|(_, w)| w.to_string())
}

/// The setting a word stands for.
fn find<T: Copy>(table: &[(T, &str)], w: &str) -> Option<T> {
    table.iter().find(|(_, x)| *x == w).map(|(v, _)| *v)
}

/// What the task list shows: tasks or the inbox, the order, and the
/// filters and grouping.
#[derive(Clone, Default)]
struct View {
    inbox: bool,
    sort: Sort,
    reverse: bool,
    state: State,
    tag: Option<String>,
    due: Option<DueFilter>,
    group: Option<Group>,
}

const DUE_FILTERS: [(DueFilter, &str); 4] = [
    (DueFilter::Overdue, "overdue"),
    (DueFilter::Today, "due by today"),
    (DueFilter::Week, "due this week"),
    (DueFilter::None, "no due date"),
];
const STATES: [(State, &str); 2] = [(State::All, "closed too"), (State::Closed, "closed only")];
const GROUPS: [(Group, &str); 3] = [(Group::Note, "by note"), (Group::Tag, "by tag"), (Group::Due, "by due")];

impl View {
    fn kind(&self) -> &'static str {
        if self.inbox { "inbox" } else { "tasks" }
    }

    fn filter(&self) -> tasks::Filter {
        let today = now().date();
        tasks::Filter {
            state: self.state,
            tag: self.tag.clone(),
            due: self.due,
            today: today.format("%Y-%m-%d").to_string(),
            week_end: (today + chrono::Duration::days(7)).format("%Y-%m-%d").to_string(),
        }
    }

    /// The view in words, as the prompt shows it: the order first, then
    /// whatever narrows or divides the list.
    fn encode(&self) -> String {
        let mut words = vec![
            match self.sort {
                Sort::Priority => "priority",
                Sort::Due => "due",
                Sort::Added => "added",
            }
            .to_string(),
        ];
        if self.reverse {
            words.push("reversed".into());
        }
        if let Some(tag) = &self.tag {
            words.push(format!("#{tag}"));
        }
        words.extend(self.due.and_then(|d| word(&DUE_FILTERS, d)));
        words.extend(word(&STATES, self.state));
        words.extend(self.group.and_then(|g| word(&GROUPS, g)));
        words.join(", ")
    }

    fn decode(inbox: bool, s: &str) -> View {
        let mut view = View {
            inbox,
            ..View::default()
        };
        let mut words = s.split(", ");
        view.sort = Sort::parse(words.next().unwrap_or(""));
        for w in words {
            if w == "reversed" {
                view.reverse = true;
            } else if let Some(tag) = w.strip_prefix('#') {
                view.tag = Some(tag.to_string());
            } else if let Some(d) = find(&DUE_FILTERS, w) {
                view.due = Some(d);
            } else if let Some(st) = find(&STATES, w) {
                view.state = st;
            } else if let Some(g) = find(&GROUPS, w) {
                view.group = Some(g);
            }
        }
        view
    }

    /// Reads the view back from a prompt made by `view_prompt`.
    fn from_prompt(inbox: bool, prompt: &str) -> View {
        let inside = prompt
            .rsplit_once('[')
            .and_then(|(_, r)| r.split_once(']'))
            .map_or("", |(s, _)| s);
        View::decode(inbox, inside)
    }
}

/// A header's hint for a configured key; nothing when the key is "".
fn key_hint(hint: &str, key: &str) -> String {
    if key.is_empty() {
        String::new()
    } else {
        fill(hint, &[&key])
    }
}

/// `ttykasten doctor`: prints the report, coloured on a terminal, and fails
/// when it found an error.
fn doctor(colour: bool) -> Result<()> {
    use ttykasten::doctor::Level;
    let report = ttykasten::doctor::check();
    let paint = |code: &str, s: &str| {
        if colour {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    };
    for (title, findings) in &report.sections {
        println!("{}", paint("1", title));
        for f in findings {
            let mark = match f.level {
                Level::Ok => paint("32", "ok   "),
                Level::Info => paint("2", "-    "),
                Level::Warn => paint("33", "WARN "),
                Level::Error => paint("31", "ERROR"),
            };
            println!("  {mark} {}", f.text);
            for line in f.advice.iter().flat_map(|a| a.lines()) {
                println!("        {line}");
            }
        }
    }
    match report.errors() {
        0 => Ok(()),
        n => bail!("{n} error(s)"),
    }
}

/// fzf's `--expect` for a configured key; nothing when the key is "".
fn expect(key: &str) -> Option<String> {
    (!key.is_empty()).then(|| format!("--expect={key}"))
}

/// Whether fzf's output says `key` (given to `expect`) ended it.
fn pressed(out: &[String], key: &str) -> bool {
    !key.is_empty() && out.first().map(String::as_str) == Some(key)
}

/// Quotes a string for sh, for arguments fzf passes to a shell.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}
