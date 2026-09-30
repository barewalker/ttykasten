//! ttykasten: fzfkasten.nvim's notes and tasks, from the terminal outside Neovim.
//!
//! Run with no arguments, it lists the actions in fzf. Subcommands are an
//! interface for key bindings and scripts, not something people are expected
//! to learn (docs/design.md). Locations, formats, the menu, the language and
//! the colours all come from the config file (config.rs).

use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use anyhow::{Context, Result, anyhow, bail};
use chrono::{Local, NaiveDate, NaiveDateTime};

use ttykasten::config::{self, Config, MenuItem};
use ttykasten::i18n::{Msgs, fill};
use ttykasten::note::{self, Periodic};
use ttykasten::{blockid, fileio, link, preview, task, template, trim};

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
            let content = match &self.cfg.new_note_template {
                Some(t) => template::load(&self.home, &self.cfg.hdate_format, t, &title, now()),
                None => format!("# {title}"),
            };
            fileio::write_text(&path, &content)
                .with_context(|| fill(self.msg.cannot_write, &[&path.display()]))?;
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
        let path = self.home.join(rel);
        match task::capture(&self.cfg, &path, &text, None)
            .with_context(|| fill(self.msg.cannot_write, &[&path.display()]))?
        {
            Some(line) => println!("{}", fill(self.msg.captured, &[&rel, &line])),
            None => eprintln!("ttykasten: {}", self.msg.nothing_captured),
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
            );
            fileio::write_text(&path, &content)
                .with_context(|| fill(self.msg.cannot_write, &[&path.display()]))?;
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
            "--expect=ctrl-x".into(),
            format!("--header={}", self.msg.log_header),
            format!("--prompt={}", self.msg.prompt_log),
        ];
        args.extend(preview);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let Some(out) = self.fzf(&lines, &args)? else {
            return Ok(());
        };
        if out.first().map(String::as_str) == Some("ctrl-x") {
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
            && s.bytes()
                .enumerate()
                .all(|(i, b)| if i == 4 || i == 7 { b == b'-' } else { b.is_ascii_digit() });
        let date = well_formed
            .then(|| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
            .flatten()
            .ok_or_else(|| anyhow!(fill(self.msg.bad_date, &[&s])))?;
        if date > now.date() {
            bail!(fill(self.msg.future_date, &[&s]));
        }
        self.open_periodic(Periodic::Daily, note::days_from(date.and_time(now.time()), 0))
    }

    /// Finds a note (§7). rg's listing is piped straight into fzf.
    fn find(&self) -> Result<()> {
        let glob = format!("*.{}", self.cfg.extension);
        let mut rg = Command::new("rg")
            .args(["--files", "--no-messages", "--no-ignore-vcs", "--glob", &glob])
            .current_dir(&self.home)
            .stdout(Stdio::piped())
            .spawn()
            .with_context(|| fill(self.msg.cannot_start, &[&"rg"]))?;
        let stdout = rg.stdout.take().context("rg")?;
        let mut args = vec![
            "--scheme=path".to_string(),
            format!("--prompt={}", self.msg.prompt_find),
        ];
        args.extend(self.preview_arg("{}")?);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = self.run_fzf(Stdio::from(stdout), None, &args);
        let _ = rg.kill();
        let _ = rg.wait();
        let Some(out) = out? else {
            return Ok(());
        };
        match out.first() {
            Some(rel) if !rel.is_empty() => self.edit(&self.home.join(rel)),
            _ => Ok(()),
        }
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
                items.push_str(&format!("{}\t{rel}\t{}\t{rel}:{}: {shown}\n", picked.len(), i + 1, i + 1));
                picked.push((rel.clone(), i, line));
            }
        }
        let mut args = vec![
            "--delimiter=\t".to_string(),
            "--with-nth=4..".into(),
            format!("--prompt={prompt}"),
            "--preview-window=+{3}-/2".into(),
        ];
        args.extend(self.preview_arg("{2} {3}")?);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let Some(out) = self.fzf(&items, &args)? else {
            return Ok(None);
        };
        Ok(out
            .first()
            .and_then(|l| l.split('\t').next())
            .and_then(|n| n.parse::<usize>().ok())
            .and_then(|n| picked.into_iter().nth(n)))
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
            lines[idx] = new_line;
            fileio::write_lines(&path, &lines)
                .with_context(|| fill(self.msg.cannot_write, &[&path.display()]))?;
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
            template::load(&self.home, &cfg.hdate_format, kind.template(cfg), &kind.name(cfg, now), now)
                .split('\n')
                .map(str::to_string)
                .collect()
        };
        lines.push(format!("{}{link}", cfg.block_id.daily_bullet));
        fileio::write_lines(&path, &lines)
            .with_context(|| fill(self.msg.cannot_write, &[&path.display()]))?;
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
            .args(["--no-ignore", "--no-messages", "--null", "--line-number", "--no-heading"])
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
    fn run_fzf(
        &self,
        stdin: Stdio,
        items: Option<&str>,
        args: &[&str],
    ) -> Result<Option<Vec<String>>> {
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
                let child = Command::new("sh")
                    .arg("-c")
                    .arg(command)
                    .stdin(Stdio::piped())
                    .spawn();
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
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
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
    use super::base64;

    #[test]
    fn base64_encodes() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64("[[n#^a1]]".as_bytes()), "W1tuI15hMV1d");
    }
}
