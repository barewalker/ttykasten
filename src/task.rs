//! Task lines (§8): capturing a task and rewriting one line (§8.8).

use std::fs;
use std::io;
use std::path::Path;

use std::sync::LazyLock;

use chrono::{Datelike, NaiveDate, NaiveDateTime};
use regex::Regex;

use crate::config::Config;
use crate::{blockid, fileio, is_ws, rtrim, trim};

/// `has_tag(text, tag)`: `#tag%f[^%w_-]`. On the right, `_` and `-` continue the
/// tag, as in `patterns.tag`. There is no left boundary (§8.5).
pub fn has_tag(text: &str, tag: &str) -> bool {
    let needle = format!("#{tag}");
    let b = text.as_bytes();
    text.match_indices(&needle).any(|(i, _)| {
        b.get(i + needle.len())
            .is_none_or(|&c| !(c.is_ascii_alphanumeric() || c == b'_' || c == b'-'))
    })
}

/// For a checkbox line: where `[m]` starts after the indent and bullet, and the mark.
pub fn checkbox(line: &str) -> Option<(usize, u8)> {
    let b = line.as_bytes();
    let mut i = b.iter().take_while(|&&c| crate::is_ws(c)).count();
    if !matches!(b.get(i), Some(b'-' | b'*')) {
        return None;
    }
    i += 1;
    let ws = b[i..].iter().take_while(|&&c| crate::is_ws(c)).count();
    if ws == 0 || b.get(i + ws) != Some(&b'[') || b.get(i + ws + 2) != Some(&b']') {
        return None;
    }
    let mark = b[i + ws + 1];
    matches!(mark, b' ' | b'x' | b'X' | b'-').then_some((i + ws, mark))
}

/// Strips one `~~` wrapped around the whole text, the cancel strike (§8.2 step 6).
pub fn unstrike(text: &str) -> &str {
    match text.strip_prefix("~~").and_then(|t| t.strip_suffix("~~")) {
        Some(inner) if text.len() >= 4 => inner,
        _ => text,
    }
}

static DONE_STAMP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[ \t\n\x0B\x0C\r]*done:[0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}").unwrap()
});
static CANCEL_STAMP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[ \t\n\x0B\x0C\r]*cancelled:[0-9]{4}-[0-9]{2}-[0-9]{2}").unwrap());
static DUE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[ \t\n\x0B\x0C\r]*due:[0-9]{4}-[0-9]{2}-[0-9]{2}[T0-9:]*").unwrap());

/// Why a line was not rewritten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    NoCheckbox,
    /// Toggling a cancelled task; reopen it first.
    Cancelled,
    /// Cancelling a done task.
    Done,
    /// Setting the due date of a task that is not open.
    NotOpen,
    BadDate,
    /// Tagging a line that already carries the tag, or has nothing to tag.
    NothingToTag,
}

/// `keeping_block_id`: the line's id is taken off, the rewrite applied, and
/// the id put back last.
fn keeping_block_id(line: &str, rewrite: impl FnOnce(&str) -> Result<String, Refusal>) -> Result<String, Refusal> {
    match blockid::read(line) {
        None => rewrite(line),
        Some(id) => {
            let id = id.to_string();
            rewrite(&blockid::strip(line)).map(|out| blockid::with_id(&out, &id))
        }
    }
}

/// The parts of a checkbox line: up to and including `[`, the mark, and
/// what follows `]`.
fn split_checkbox(line: &str) -> Option<(&str, u8, &str)> {
    let (at, mark) = checkbox(line)?;
    Some((&line[..at + 1], mark, &line[at + 3..]))
}

/// TaskToggle (§8.8): open becomes done with a `done:` stamp; done becomes
/// open and loses it. A cancelled task is refused.
pub fn toggle_line(line: &str, now: NaiveDateTime) -> Result<String, Refusal> {
    keeping_block_id(line, |line| {
        let (before, mark, rest) = split_checkbox(line).ok_or(Refusal::NoCheckbox)?;
        if mark == b'-' {
            return Err(Refusal::Cancelled);
        }
        let now_done = mark == b' ';
        let toggled = format!("{before}{}]{rest}", if now_done { 'x' } else { ' ' });
        let mut out = rtrim(&DONE_STAMP.replace_all(&toggled, "")).to_string();
        if now_done {
            out.push_str(&now.format(" done:%Y-%m-%d %H:%M").to_string());
        }
        Ok(out)
    })
}

/// TaskCancel (§8.8): wraps the text in `~~` and stamps it, keeping the
/// priority outside; a cancelled task is reopened. A done task is refused.
pub fn cancel_line(line: &str, today: NaiveDate) -> Result<String, Refusal> {
    keeping_block_id(line, |line| {
        let (before, mark, rest) = split_checkbox(line).ok_or(Refusal::NoCheckbox)?;
        if mark.eq_ignore_ascii_case(&b'x') {
            return Err(Refusal::Done);
        }
        let lead_len = rest.bytes().take_while(|&b| is_ws(b)).count();
        let lead = &rest[..lead_len];
        let mut text = rtrim(&rest[lead_len..]);
        if text.is_empty() {
            return Err(Refusal::NoCheckbox);
        }
        let mut kept = "";
        let b = text.as_bytes();
        if b.len() >= 4 && b[0] == b'(' && b[1].is_ascii_uppercase() && b[2] == b')' && is_ws(b[3]) {
            let ws = b[3..].iter().take_while(|&&c| is_ws(c)).count();
            kept = &text[..3 + ws];
            text = &text[3 + ws..];
        }
        let reopening = mark == b'-';
        let stripped = CANCEL_STAMP.replace_all(text, "");
        let mut text = unstrike(trim(&stripped)).to_string();
        if !reopening {
            text = format!("~~{text}~~{}", today.format(" cancelled:%Y-%m-%d"));
        }
        let mark = if reopening { ' ' } else { '-' };
        Ok(format!("{before}{mark}]{lead}{kept}{text}"))
    })
}

/// `tag_line` (§8.8 TaskTag): a line without a checkbox becomes one
/// (`new_checkbox`, dropping a `-` or `*` bullet), then ` #tag` is added
/// before the id. Refused when the tag is already there or nothing is left.
pub fn tag_line(line: &str, tag: &str, new_checkbox: &str) -> Result<String, Refusal> {
    keeping_block_id(line, |line| {
        let mut line = line.to_string();
        if checkbox(&line).is_none() {
            let indent_len = line.bytes().take_while(|&b| is_ws(b)).count();
            let indent = &line[..indent_len];
            let mut text = rtrim(&line[indent_len..]);
            if let Some(rest) = text.strip_prefix(['-', '*']) {
                let ws = rest.bytes().take_while(|&b| is_ws(b)).count();
                if ws > 0 {
                    text = &rest[ws..];
                }
            }
            if text.is_empty() {
                return Err(Refusal::NothingToTag);
            }
            line = format!("{indent}{new_checkbox}{text}");
        }
        if has_tag(&line, tag) {
            return Err(Refusal::NothingToTag);
        }
        Ok(format!("{} #{tag}", rtrim(&line)))
    })
}

/// TaskDue (§8.8): replaces every `due:` with ` due:<date>`, or removes them
/// when `date` is empty. Only an open task takes a due date.
pub fn due_line(line: &str, date: &str) -> Result<String, Refusal> {
    keeping_block_id(line, |line| {
        let (_, mark, _) = split_checkbox(line).ok_or(Refusal::NoCheckbox)?;
        if mark != b' ' {
            return Err(Refusal::NotOpen);
        }
        let date = trim(date);
        let stripped = rtrim(&DUE.replace_all(line, "")).to_string();
        if date.is_empty() {
            return Ok(stripped);
        }
        if !valid_due(date) {
            return Err(Refusal::BadDate);
        }
        Ok(format!("{stripped} due:{date}"))
    })
}

/// An absolute due date: `YYYY-MM-DD` or `YYYY-MM-DDTHH:MM`. The calendar is
/// not checked.
pub fn valid_due(s: &str) -> bool {
    let shape = |s: &str, pat: &str| {
        s.len() == pat.len()
            && s.bytes().zip(pat.bytes()).all(|(c, p)| if p == b'9' { c.is_ascii_digit() } else { c == p })
    };
    shape(s, "9999-99-99") || shape(s, "9999-99-99T99:99")
}

/// `resolve_due` (§8.8): an absolute date as it is, or a relative one
/// (today, tomorrow, +3d, 2w, fri, 明日, 金 …) as YYYY-MM-DD from `today`.
/// None for anything else. An empty spec is the caller's "clear".
pub fn resolve_due(spec: &str, today: NaiveDate) -> Option<String> {
    let spec = trim(spec);
    if spec.is_empty() {
        return None;
    }
    if valid_due(spec) {
        return Some(spec.to_string());
    }
    let key = spec.to_ascii_lowercase();
    let at = |days: i64| Some((today + chrono::Duration::days(days)).format("%Y-%m-%d").to_string());
    let offset = match key.as_str() {
        "today" => Some(0),
        "tomorrow" | "tom" | "tmr" => Some(1),
        _ => match spec {
            "今日" => Some(0),
            "明日" | "あした" => Some(1),
            "明後日" | "あさって" => Some(2),
            _ => None,
        },
    };
    if let Some(d) = offset {
        return at(d);
    }
    let digits = key.strip_prefix('+').unwrap_or(&key);
    if let Some(unit) = digits.chars().last().filter(|c| *c == 'd' || *c == 'w') {
        let n = &digits[..digits.len() - 1];
        if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) {
            let n: i64 = n.parse().ok()?;
            return at(if unit == 'w' { n * 7 } else { n });
        }
    }
    let weekday = match key.as_str() {
        "sun" | "sunday" => Some(0),
        "mon" | "monday" => Some(1),
        "tue" | "tuesday" => Some(2),
        "wed" | "wednesday" => Some(3),
        "thu" | "thursday" => Some(4),
        "fri" | "friday" => Some(5),
        "sat" | "saturday" => Some(6),
        _ => ["日", "月", "火", "水", "木", "金", "土"].iter().position(|d| *d == spec).map(|p| p as i64),
    };
    let wd = weekday?;
    let today_wd = i64::from(today.weekday().num_days_from_sunday());
    at((wd - today_wd).rem_euclid(7))
}

/// Tags picked in the guided capture, each added as ` #name` unless the text
/// already has it (`with_tags`).
pub fn with_tags(text: &str, picked: &[String]) -> String {
    let mut out = text.to_string();
    for tag in picked {
        let name = tag.trim_start_matches('#');
        if !name.is_empty() && !has_tag(&out, name) {
            out.push_str(" #");
            out.push_str(name);
        }
    }
    out
}

/// `new_task_line` (§8.8 TaskAdd). None for blank text. `tag` is `require_tag`,
/// and `id` is the new id added with `on_capture`.
pub fn new_task_line(
    checkbox: &str,
    text: &str,
    tag: Option<&str>,
    due: Option<&str>,
    id: Option<&str>,
) -> Option<String> {
    let text = trim(text);
    if text.is_empty() {
        return None;
    }
    let mut line = format!("{checkbox}{text}");
    if let Some(tag) = tag
        && !has_tag(&line, tag)
    {
        line.push_str(" #");
        line.push_str(tag);
    }
    if let Some(due) = due.filter(|d| !d.is_empty()) {
        line.push_str(" due:");
        line.push_str(due);
    }
    Some(match id {
        Some(id) => blockid::with_id(&line, id),
        None => line,
    })
}

/// Captures a task: appends it as the file's last line and returns that line
/// with its 1-based number. A missing file is created, directory included,
/// holding just the line.
pub fn capture(cfg: &Config, path: &Path, text: &str, due: Option<&str>) -> io::Result<Option<(String, usize)>> {
    let mut lines = match fileio::read_lines(path) {
        Ok(l) => l,
        Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(e),
    };
    let id = cfg
        .block_id
        .on_capture
        .then(|| blockid::mint(&blockid::ids_in(&lines), cfg.block_id.length, &cfg.block_id.alphabet));
    let checkbox = &cfg.tasks.new_checkbox;
    let Some(line) = new_task_line(checkbox, text, cfg.tasks.require_tag.as_deref(), due, id.as_deref())
    else {
        return Ok(None);
    };
    lines.push(line.clone());
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fileio::write_lines(path, &lines)?;
    Ok(Some((line, lines.len())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(y: i32, m: u32, d: u32, h: u32, mi: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(h, mi, 0).unwrap()
    }

    #[test]
    fn toggling() {
        let now = at(2026, 9, 29, 14, 5);
        assert_eq!(
            toggle_line("- [ ] draft #todo ^t3k9aa", now).unwrap(),
            "- [x] draft #todo done:2026-09-29 14:05 ^t3k9aa"
        );
        assert_eq!(toggle_line("- [x] foo done:2026-07-17 10:00", now).unwrap(), "- [ ] foo");
        assert_eq!(toggle_line("  * [X] foo", now).unwrap(), "  * [ ] foo");
        assert_eq!(toggle_line("- [-] ~~foo~~", now), Err(Refusal::Cancelled));
        assert_eq!(toggle_line("plain", now), Err(Refusal::NoCheckbox));
    }

    #[test]
    fn cancelling() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 16).unwrap();
        assert_eq!(
            cancel_line("- [ ] (A) #projectx check #todo due:2026-09-14 ^a1b2c3", today).unwrap(),
            "- [-] (A) ~~#projectx check #todo due:2026-09-14~~ cancelled:2026-09-16 ^a1b2c3"
        );
        assert_eq!(cancel_line("- [-] (A) ~~foo~~ cancelled:2026-07-17", today).unwrap(), "- [ ] (A) foo");
        assert_eq!(cancel_line("- [x] foo", today), Err(Refusal::Done));
        assert_eq!(cancel_line("- [X] foo", today), Err(Refusal::Done));
        assert_eq!(cancel_line("- [ ]   ", today), Err(Refusal::NoCheckbox));
    }

    #[test]
    fn tagging() {
        let t = |l| tag_line(l, "todo", "- [ ] ");
        assert_eq!(t("buy milk").unwrap(), "- [ ] buy milk #todo");
        assert_eq!(t("  - buy milk").unwrap(), "  - [ ] buy milk #todo");
        assert_eq!(t("1. buy milk").unwrap(), "- [ ] 1. buy milk #todo");
        assert_eq!(t("- [ ] foo ^abc123").unwrap(), "- [ ] foo #todo ^abc123");
        assert_eq!(t("- [-] ~~foo~~ cancelled:2026-07-17").unwrap(), "- [-] ~~foo~~ cancelled:2026-07-17 #todo");
        assert_eq!(t("- [ ] foo #todo"), Err(Refusal::NothingToTag));
        assert_eq!(t("   "), Err(Refusal::NothingToTag));
    }

    #[test]
    fn due_dates() {
        assert_eq!(
            due_line("- [ ] foo due:2026-07-20 #todo ^t3k9aa", "2026-07-24").unwrap(),
            "- [ ] foo #todo due:2026-07-24 ^t3k9aa"
        );
        assert_eq!(due_line("- [ ] foo due:2026-07-20", "").unwrap(), "- [ ] foo");
        assert_eq!(due_line("- [ ] foo", "").unwrap(), "- [ ] foo");
        assert_eq!(due_line("- [x] foo", "2026-07-24"), Err(Refusal::NotOpen));
        assert_eq!(due_line("- [X] foo", "2026-07-24"), Err(Refusal::NotOpen));
        assert_eq!(due_line("- [ ] foo", "2026-07-24 10:00"), Err(Refusal::BadDate));
        assert_eq!(due_line("- [ ] foo", "2026-07-24T10:00").unwrap(), "- [ ] foo due:2026-07-24T10:00");
    }

    #[test]
    fn resolving_due() {
        // Wednesday 2026-07-22.
        let wed = NaiveDate::from_ymd_opt(2026, 7, 22).unwrap();
        let r = |s| resolve_due(s, wed);
        assert_eq!(r("fri").as_deref(), Some("2026-07-24"));
        assert_eq!(r("FRIDAY").as_deref(), Some("2026-07-24"));
        assert_eq!(r("wed").as_deref(), Some("2026-07-22"));
        assert_eq!(r("火").as_deref(), Some("2026-07-28"));
        assert_eq!(r("today").as_deref(), Some("2026-07-22"));
        assert_eq!(r("Tom").as_deref(), Some("2026-07-23"));
        assert_eq!(r("明後日").as_deref(), Some("2026-07-24"));
        assert_eq!(r("+3d").as_deref(), Some("2026-07-25"));
        assert_eq!(r("2W").as_deref(), Some("2026-08-05"));
        assert_eq!(r("0d").as_deref(), Some("2026-07-22"));
        assert_eq!(r("2026-02-31").as_deref(), Some("2026-02-31"));
        assert_eq!(r("-1d"), None);
        assert_eq!(r("2026-07-24 10:00"), None);
        assert_eq!(r("someday"), None);
        assert_eq!(r("  "), None);
    }

    #[test]
    fn adding_tags() {
        let picked = vec!["#todo".to_string(), "#projectx".to_string(), "v2".to_string()];
        assert_eq!(with_tags("review #todo", &picked), "review #todo #projectx #v2");
    }

    #[test]
    fn tag_detection() {
        assert!(!has_tag("#todos", "todo"));
        assert!(!has_tag("#TODO", "todo"));
        assert!(has_tag("foo#todo", "todo"));
        assert!(!has_tag("#todo-x", "todo"));
        assert!(!has_tag("#todo_list", "todo"));
        assert!(has_tag("#todo,", "todo"));
        assert!(has_tag("#todo ^abc123", "todo"));
        assert!(has_tag("a #todo", "todo"));
        assert!(has_tag("#todos #todo", "todo"));
        assert!(has_tag("#todo日本", "todo"));
    }

    fn line(text: &str, due: Option<&str>, id: Option<&str>) -> Option<String> {
        new_task_line("- [ ] ", text, Some("todo"), due, id)
    }

    #[test]
    fn capture_line() {
        assert_eq!(
            line("  call Acme back ", Some("2026-10-02"), Some("a1b2c3")).as_deref(),
            Some("- [ ] call Acme back #todo due:2026-10-02 ^a1b2c3")
        );
        assert_eq!(line("#todo foo", None, Some("abc123")).as_deref(), Some("- [ ] #todo foo ^abc123"));
        // with_block_id removes an id typed into the text.
        assert_eq!(line("foo ^zzz999", None, Some("abc123")).as_deref(), Some("- [ ] foo #todo ^abc123"));
        // `#todo-x` is another tag, so #todo is added (Q8).
        assert_eq!(
            line("#todo-x foo", None, Some("abc123")).as_deref(),
            Some("- [ ] #todo-x foo #todo ^abc123")
        );
        assert_eq!(line(" \t", None, Some("abc123")), None);
        // The plugin's defaults: no tag and no id.
        assert_eq!(new_task_line("- [ ] ", " foo ", None, None, None).as_deref(), Some("- [ ] foo"));
    }

    fn user_config() -> Config {
        let mut cfg = Config::default();
        cfg.tasks.require_tag = Some("todo".into());
        cfg.block_id.on_capture = true;
        cfg
    }

    #[test]
    fn capture_appends_last() {
        let cfg = user_config();
        let dir = std::env::temp_dir().join(format!("ttykasten-test-{}", std::process::id()));
        let path = dir.join("tasks/active.md");
        let _ = fs::remove_dir_all(&dir);

        let (first, n) = capture(&cfg, &path, "one", None).unwrap().unwrap();
        assert_eq!(n, 1);
        assert_eq!(fs::read_to_string(&path).unwrap(), format!("{first}\n"));

        // CRLF, a BOM, a missing final newline and trailing blank lines are normalised per §0.4.
        fs::write(&path, "\u{feff}# T\r\n- a\r\n\r\nlast").unwrap();
        let (second, n) = capture(&cfg, &path, "two", None).unwrap().unwrap();
        assert_eq!(n, 5);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            format!("# T\n- a\n\nlast\n{second}\n")
        );
        assert!(capture(&cfg, &path, "  ", None).unwrap().is_none());
        fs::remove_dir_all(&dir).unwrap();
    }
}
