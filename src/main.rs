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
use ttykasten::{fileio, preview, task, template};

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
            // The preview next to fzf. Called by fzf, so it is not in the menu.
            "preview" => self.preview(arg.as_deref().unwrap_or("")),
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

    /// Prints a note in colour. Prints nothing if it can't be read (a daily note not yet made).
    fn preview(&self, rel: &str) -> Result<()> {
        if let Ok(lines) = fileio::read_lines(&self.home.join(rel)) {
            let mut out = std::io::stdout().lock();
            // Writing fails when fzf stops reading; that is fine.
            let _ = out.write_all(preview::render(&lines, &self.cfg.preview.colors).as_bytes());
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

fn now() -> NaiveDateTime {
    Local::now().naive_local()
}
