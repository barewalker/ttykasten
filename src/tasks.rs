//! Collecting tasks from the notes (§8.3–§8.7): the task list and the inbox.
//!
//! The notes are the only record of the tasks; there is no index. Each call
//! reads the notes that contain a checkbox and rebuilds the list.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use unicode_width::UnicodeWidthStr;

use crate::config::Config;
use crate::{blockid, is_ws, trim};

/// One checkbox found in a note.
#[derive(Debug, Clone)]
pub struct Task {
    /// The task's text, without priority, stamps, ids and an outer strike.
    pub text: String,
    pub id: Option<String>,
    pub done: bool,
    pub done_at: Option<String>,
    pub cancelled: bool,
    pub cancelled_at: Option<String>,
    pub priority: Option<char>,
    pub due: Option<String>,
    /// Path relative to home.
    pub rel: String,
    /// 1-based line number.
    pub lineno: usize,
    /// The note's date (§8.4).
    pub date: Option<String>,
    /// How many checkboxes this one sits inside.
    pub depth: usize,
    /// The task it is a subtask of, as an index into the scan.
    pub parent: Option<usize>,
    /// The parent's line number (in the same note).
    pub parent_line: Option<usize>,
    /// The text of the list item directly above it.
    pub context: Option<String>,
    pub children: usize,
    pub children_closed: usize,
    /// The tags it is filed under: its own, then its parents', nearest first,
    /// without `require_tag`, which every task carries (`tags_of`).
    pub tags: Vec<String>,
    /// Its due date, or that of the nearest task it is a step of.
    pub due_in_effect: Option<String>,
    root_priority: Option<char>,
    root_due: Option<String>,
    root_lineno: usize,
    tagged: bool,
    aged_out: bool,
    /// A subtask whose parent is not in the list being shown.
    pub orphaned: bool,
}

/// What to collect.
#[derive(Debug, Clone, Copy, Default)]
pub struct Want {
    /// The untagged checkboxes (the inbox) instead of the tagged ones.
    pub inbox: bool,
    pub done: bool,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    #[default]
    Priority,
    Due,
    Added,
}

impl Sort {
    pub fn parse(s: &str) -> Sort {
        match s {
            "due" => Sort::Due,
            "added" => Sort::Added,
            _ => Sort::Priority,
        }
    }
}

/// A tag in a task's text (the plugin's `patterns.tag`).
static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"#([A-Za-z0-9_-]+)").unwrap());

/// Which tasks to show by state: open ones, closed ones too, or only those.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    Open,
    All,
    Closed,
}

/// Which tasks a due filter keeps. Each of the first three includes the one
/// before it: what was due on Tuesday is still due by Friday.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DueFilter {
    Overdue,
    Today,
    Week,
    None,
}

/// Where a due date falls; disjoint, so each task is in one group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bucket {
    Overdue,
    Today,
    Week,
    Later,
    None,
}

impl Bucket {
    pub const ORDER: [Bucket; 5] = [
        Bucket::Overdue,
        Bucket::Today,
        Bucket::Week,
        Bucket::Later,
        Bucket::None,
    ];
}

/// What the list can be divided by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Note,
    Tag,
    Due,
}

/// The view's filters. `today` and `week_end` (today + 7) are YYYY-MM-DD.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub state: State,
    pub tag: Option<String>,
    pub due: Option<DueFilter>,
    pub today: String,
    pub week_end: String,
}

/// Where `due` falls, by its day (a time is ignored).
pub fn bucket(due: Option<&str>, today: &str, week_end: &str) -> Bucket {
    let Some(due) = due else {
        return Bucket::None;
    };
    let day = due.get(..10).unwrap_or(due);
    if day < today {
        Bucket::Overdue
    } else if day == today {
        Bucket::Today
    } else if day <= week_end {
        Bucket::Week
    } else {
        Bucket::Later
    }
}

impl DueFilter {
    fn keeps(self, b: Bucket) -> bool {
        match self {
            DueFilter::Overdue => b == Bucket::Overdue,
            DueFilter::Today => matches!(b, Bucket::Overdue | Bucket::Today),
            DueFilter::Week => matches!(b, Bucket::Overdue | Bucket::Today | Bucket::Week),
            DueFilter::None => b == Bucket::None,
        }
    }
}

/// The compiled patterns a scan needs.
pub struct Scanner<'a> {
    cfg: &'a Config,
    due: Regex,
    done_stamp: Regex,
    cancel_stamp: Regex,
    date: Regex,
    date_patterns: Vec<Regex>,
    headings: Vec<Regex>,
}

/// Lua's `%s`, for use inside regexes.
const WS: &str = r"[ \t\n\x0B\x0C\r]";

impl<'a> Scanner<'a> {
    pub fn new(cfg: &'a Config) -> Result<Self, regex::Error> {
        let compile = |v: &[String]| v.iter().map(|p| Regex::new(p)).collect::<Result<Vec<_>, _>>();
        Ok(Scanner {
            cfg,
            due: Regex::new(r"due:([0-9]{4}-[0-9]{2}-[0-9]{2}[T0-9:]*)")?,
            done_stamp: Regex::new(&format!(
                r"{WS}*done:([0-9]{{4}}-[0-9]{{2}}-[0-9]{{2}} [0-9]{{2}}:[0-9]{{2}})"
            ))?,
            cancel_stamp: Regex::new(&format!(r"{WS}*cancelled:([0-9]{{4}}-[0-9]{{2}}-[0-9]{{2}})"))?,
            date: Regex::new(r"[0-9]{4}-[0-9]{2}-[0-9]{2}")?,
            date_patterns: compile(&cfg.tasks.date_patterns)?,
            headings: compile(&cfg.tasks.headings)?,
        })
    }

    /// Scans one note's lines and appends what it finds to `all`. `cutoff` is
    /// the `since_days` limit as YYYY-MM-DD.
    pub fn scan_note(&self, rel: &str, lines: &[String], cutoff: Option<&str>, all: &mut Vec<Task>) {
        let t = &self.cfg.tasks;
        if in_dirs(rel, &t.ignore.dirs) {
            return;
        }
        let (fm, fm_end) = frontmatter(lines);
        let opted_out = t
            .ignore
            .frontmatter_key
            .as_ref()
            .and_then(|k| fm.get(k.as_str()))
            .is_some_and(|v| matches!(*v, "false" | "no" | "off"));
        let date = self.note_date(rel, lines, &fm);
        let aged_out =
            cutoff.is_some_and(|c| date.as_deref().is_some_and(|d| d < c)) && !t.always.iter().any(|a| a == rel);
        // With require_tag an old note can still hold tagged tasks, so only
        // its untagged checkboxes age out.
        if opted_out || (aged_out && t.require_tag.is_none()) {
            return;
        }
        let scannable = self.scannable(lines, fm_end);

        struct Open {
            indent: usize,
            text: String,
            tagged: bool,
            depth: usize,
            task: Option<usize>,
            root: Option<usize>,
        }
        let mut stack: Vec<Open> = Vec::new();
        for (i, line) in lines.iter().enumerate().skip(fm_end) {
            let at_margin = line.bytes().next().is_some_and(|b| !is_ws(b));
            if !scannable[i] {
                // A heading, or a fence at the margin, ends the list (Q9).
                if at_margin {
                    stack.clear();
                }
                continue;
            }
            let item = checkbox_item(line);
            let bullet = if item.is_none() { bullet_text(line) } else { None };
            if item.is_none() && bullet.is_none() {
                // A paragraph at the margin ends the list; blank lines and
                // indented prose sit inside a loose list item.
                if at_margin {
                    stack.clear();
                }
                continue;
            }
            let indent = indent_width(line);
            while stack.last().is_some_and(|o| o.indent >= indent) {
                stack.pop();
            }
            let parent = stack.last();
            let Some((kind, raw)) = item else {
                // A bullet is never a task and never introduces the tag, but
                // it passes down what the item above it decided.
                let entry = Open {
                    indent,
                    text: bullet.unwrap_or_default().to_string(),
                    tagged: parent.is_some_and(|p| p.tagged),
                    depth: parent.map_or(0, |p| p.depth),
                    task: parent.and_then(|p| p.task),
                    root: parent.and_then(|p| p.root),
                };
                stack.push(entry);
                continue;
            };
            let parsed = self.parse_text(raw);
            let idx = all.len();
            let parent_task = parent.and_then(|p| p.task);
            let root = parent.and_then(|p| p.root).unwrap_or(idx);
            let tagged = match &t.require_tag {
                None => true,
                Some(tag) => crate::task::has_tag(&parsed.text, tag) || parent.is_some_and(|p| p.tagged),
            };
            let (root_priority, root_due, root_lineno) = if root == idx {
                (parsed.priority, parsed.due.clone(), i + 1)
            } else {
                (all[root].priority, all[root].due.clone(), all[root].lineno)
            };
            let depth = parent.map_or(0, |p| p.depth);
            let context = parent.map(|p| p.text.clone());
            let mut tags: Vec<String> = Vec::new();
            for c in TAG.captures_iter(&parsed.text) {
                let name = &c[1];
                if Some(name) != t.require_tag.as_deref() && !tags.iter().any(|n| n == name) {
                    tags.push(name.to_string());
                }
            }
            if let Some(p) = parent_task {
                for name in &all[p].tags {
                    if !tags.contains(name) {
                        tags.push(name.clone());
                    }
                }
            }
            let due_in_effect = parsed
                .due
                .clone()
                .or_else(|| parent_task.and_then(|p| all[p].due_in_effect.clone()));
            if let Some(p) = parent_task {
                all[p].children += 1;
                if kind != Kind::Open {
                    all[p].children_closed += 1;
                }
            }
            stack.push(Open {
                indent,
                text: parsed.text.clone(),
                tagged,
                depth: depth + 1,
                task: Some(idx),
                root: Some(root),
            });
            all.push(Task {
                text: parsed.text,
                id: blockid::read(raw).map(str::to_string),
                done: kind == Kind::Done,
                done_at: parsed.done_at,
                cancelled: kind == Kind::Cancelled,
                cancelled_at: parsed.cancelled_at,
                priority: parsed.priority,
                due: parsed.due,
                rel: rel.to_string(),
                lineno: i + 1,
                date: date.clone(),
                depth,
                parent: parent_task,
                parent_line: parent_task.map(|p| all[p].lineno),
                context,
                children: 0,
                children_closed: 0,
                tags,
                due_in_effect,
                root_priority,
                root_due,
                root_lineno,
                tagged,
                aged_out,
                orphaned: false,
            });
        }
    }

    /// Which lines of a note can hold a task (§8.3), for TaskTag and minting
    /// ids, which must not touch a line the scan would skip.
    pub fn scannable_lines(&self, lines: &[String]) -> Vec<bool> {
        let (_, fm_end) = frontmatter(lines);
        self.scannable(lines, fm_end)
    }

    /// Which lines can hold a task (§8.3): below the frontmatter, outside
    /// fences, not headings, and in scope.
    fn scannable(&self, lines: &[String], fm_end: usize) -> Vec<bool> {
        let by_headings = self.cfg.tasks.scope == "headings";
        let mut out = vec![false; lines.len()];
        let mut in_scope = !by_headings;
        let mut fence: Option<(u8, usize)> = None;
        for (i, line) in lines.iter().enumerate().skip(fm_end) {
            let delim = fence_delim(line);
            if let Some((ch, len)) = fence {
                if delim.is_some_and(|(c, l)| c == ch && l >= len) {
                    fence = None;
                }
            } else if delim.is_some() {
                fence = delim;
            } else if let Some(heading) = heading_text(line) {
                if by_headings {
                    let lowered = trim(heading).to_ascii_lowercase();
                    in_scope = self.headings.iter().any(|r| r.is_match(&lowered));
                }
            } else if in_scope {
                out[i] = true;
            }
        }
        out
    }

    /// A note's date (§8.4): `date_patterns`, then the file name, then the
    /// frontmatter keys. Never the file's mtime.
    /// A note's date (§8.4), as the week views read it too.
    pub fn date_of(&self, rel: &str, lines: &[String]) -> Option<String> {
        let (fm, _) = frontmatter(lines);
        self.note_date(rel, lines, &fm)
    }

    fn note_date(&self, rel: &str, lines: &[String], fm: &HashMap<&str, &str>) -> Option<String> {
        let first_date = |s: &str| self.date.find(s).map(|m| m.as_str().to_string());
        for line in lines.iter().take(self.cfg.tasks.date_lines) {
            for re in &self.date_patterns {
                if let Some(c) = re.captures(line) {
                    let hit = c.get(1).or_else(|| c.get(0)).map_or("", |m| m.as_str());
                    if let Some(d) = first_date(hit) {
                        return Some(d);
                    }
                }
            }
        }
        let base = rel.rsplit('/').next().unwrap_or(rel);
        if let Some(d) = first_date(base) {
            return Some(d);
        }
        self.cfg
            .tasks
            .date_keys
            .iter()
            .find_map(|k| fm.get(k.as_str()).and_then(|v| first_date(v)))
    }

    /// `parse_task_text` (§8.2).
    fn parse_text(&self, raw: &str) -> Parsed {
        let mut text = raw.to_string();
        let mut priority = None;
        let b = text.as_bytes();
        if b.len() >= 4 && b[0] == b'(' && b[1].is_ascii_uppercase() && b[2] == b')' && is_ws(b[3]) {
            priority = Some(b[1] as char);
            let ws = b[3..].iter().take_while(|&&c| is_ws(c)).count();
            text = text[3 + ws..].to_string();
        }
        let due = self.due.captures(&text).map(|c| c[1].to_string());
        let done_at = self.done_stamp.captures(&text).map(|c| c[1].to_string());
        if done_at.is_some() {
            text = self.done_stamp.replace_all(&text, "").into_owned();
        }
        let cancelled_at = self.cancel_stamp.captures(&text).map(|c| c[1].to_string());
        if cancelled_at.is_some() {
            text = self.cancel_stamp.replace_all(&text, "").into_owned();
        }
        let text = blockid::strip(&text);
        let text = trim(&text);
        let text = crate::task::unstrike(text);
        Parsed {
            text: trim(text).to_string(),
            priority,
            due,
            done_at,
            cancelled_at,
        }
    }

    /// Display text (`display_text`): without `due:` and the required tag,
    /// whitespace squeezed.
    fn display_text(&self, text: &str) -> String {
        let text = self.due.replace_all(text, "");
        let text = match &self.cfg.tasks.require_tag {
            Some(tag) => remove_tag(&text, tag),
            None => text.into_owned(),
        };
        squeeze(&text)
    }

    /// How a task reads on one line (`entry_text`). A done or cancelled task
    /// is marked `✓` or `✗`.
    pub fn entry_text(&self, task: &Task) -> String {
        self.entry_text_as(task, task.orphaned)
    }

    /// `entry_text` with `orphaned` decided by the caller: a grouped view
    /// decides per group whether the parent is on a row above.
    pub fn entry_text_as(&self, task: &Task, orphaned: bool) -> String {
        let nested = task.depth > 0 && task.parent.is_some() && !orphaned;
        let mut lead = if nested {
            format!("{}↳ ", "  ".repeat(task.depth))
        } else {
            String::new()
        };
        if task.done {
            lead.push_str("✓ ");
        } else if task.cancelled {
            lead.push_str("✗ ");
        }
        let prefix = task.priority.map(|p| format!("({p}) ")).unwrap_or_default();
        let progress = if task.children > 0 {
            format!("  [{}/{}]", task.children_closed, task.children)
        } else {
            String::new()
        };
        let due = task.due.as_ref().map(|d| format!("  [due {d}]")).unwrap_or_default();
        let context = match (&task.context, nested) {
            (Some(c), false) => format!("  ← {}", truncate(&self.display_text(c), 40)),
            _ => String::new(),
        };
        format!(
            "{lead}{prefix}{}{progress}{due}{context}",
            self.display_text(&task.text)
        )
    }
}

struct Parsed {
    text: String,
    priority: Option<char>,
    due: Option<String>,
    done_at: Option<String>,
    cancelled_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Open,
    Done,
    Cancelled,
}

/// A short fingerprint of a line (64-bit FNV-1a, hex), so an action from a
/// list can check that the line is still the one the list showed.
pub fn line_hash(line: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in line.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Picks the tasks wanted from a scan, marks orphans and sorts them.
pub fn select(mut all: Vec<Task>, want: Want, filter: &Filter, sort: Sort, reverse: bool) -> Vec<Task> {
    let keep: Vec<bool> = all
        .iter()
        .map(|t| {
            let closed = t.done || t.cancelled;
            !(t.aged_out && !t.tagged)
                && t.tagged != want.inbox
                && match filter.state {
                    State::Open => (want.done || !t.done) && (want.cancelled || !t.cancelled),
                    State::All => true,
                    State::Closed => closed,
                }
                && filter.tag.as_ref().is_none_or(|tag| t.tags.contains(tag))
                && filter
                    .due
                    .is_none_or(|d| d.keeps(bucket(t.due_in_effect.as_deref(), &filter.today, &filter.week_end)))
        })
        .collect();
    for t in all.iter_mut() {
        t.orphaned = t.parent.is_some_and(|p| !keep[p]);
    }
    let mut out: Vec<Task> = all.into_iter().zip(keep).filter(|(_, k)| *k).map(|(t, _)| t).collect();
    // Reversing turns the items round, not the steps of a job, which keep
    // the order they are written in.
    out.sort_by(|a, b| {
        let by = sort_fields(a, sort).cmp(&sort_fields(b, sort));
        if reverse { by.reverse() } else { by }.then(a.lineno.cmp(&b.lineno))
    });
    out
}

/// Every tag the tasks are filed under, in the order they first come up.
pub fn tags_in(list: &[Task]) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for name in list.iter().flat_map(|t| &t.tags) {
        if !tags.contains(name) {
            tags.push(name.clone());
        }
    }
    tags
}

/// What a group is made of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupKey {
    Note(String),
    Tag(String),
    NoTag,
    Due(Bucket),
}

/// One group: its key, and its tasks as (index into the list, orphaned).
/// A step counts as orphaned when its parent is not in the same group.
pub type Grouped = Vec<(GroupKey, Vec<(usize, bool)>)>;

/// Divides an ordered list (`group`), keeping the order inside each group:
/// by note in the order notes first come up; by tag likewise, a task under
/// each of its tags and the untagged last; by due date in calendar order.
pub fn group(list: &[Task], by: Group, today: &str, week_end: &str) -> Grouped {
    let mut groups: Vec<(GroupKey, Vec<usize>)> = Vec::new();
    let mut add = |key: GroupKey, i: usize| match groups.iter_mut().find(|(k, _)| *k == key) {
        Some((_, v)) => v.push(i),
        None => groups.push((key, vec![i])),
    };
    for (i, t) in list.iter().enumerate() {
        match by {
            Group::Note => add(GroupKey::Note(t.rel.clone()), i),
            Group::Tag if t.tags.is_empty() => add(GroupKey::NoTag, i),
            Group::Tag => t.tags.iter().for_each(|name| add(GroupKey::Tag(name.clone()), i)),
            Group::Due => add(GroupKey::Due(bucket(t.due_in_effect.as_deref(), today, week_end)), i),
        }
    }
    match by {
        Group::Tag => groups.sort_by_key(|(k, _)| *k == GroupKey::NoTag),
        Group::Due => groups.sort_by_key(|(k, _)| match k {
            GroupKey::Due(b) => Bucket::ORDER.iter().position(|o| o == b),
            _ => None,
        }),
        Group::Note => {}
    }
    groups
        .into_iter()
        .map(|(key, members)| {
            let lines: Vec<(&str, usize)> = members
                .iter()
                .map(|&i| (list[i].rel.as_str(), list[i].lineno))
                .collect();
            let rows = members
                .iter()
                .map(|&i| {
                    let t = &list[i];
                    let parent_here = t.parent_line.is_some_and(|p| lines.contains(&(t.rel.as_str(), p)));
                    (i, t.parent.is_some() && !parent_here)
                })
                .collect();
            (key, rows)
        })
        .collect()
}

/// The keys a task sorts by, taken from its root task so a subtask stays
/// under the item it belongs to (§8.7). Missing values sort last.
fn sort_fields(t: &Task, sort: Sort) -> (String, String, String, usize) {
    let priority = t.root_priority.map(String::from).unwrap_or_else(|| "~".into());
    let due = t.root_due.clone().unwrap_or_else(|| "9999-99-99".into());
    match sort {
        Sort::Priority => (priority, due, t.rel.clone(), t.root_lineno),
        Sort::Due => (due, priority, t.rel.clone(), t.root_lineno),
        Sort::Added => (
            t.date.clone().unwrap_or_else(|| "9999-99-99".into()),
            t.rel.clone(),
            String::new(),
            t.root_lineno,
        ),
    }
}

/// The frontmatter's flat pairs and the number of lines it takes (0 if none).
fn frontmatter(lines: &[String]) -> (HashMap<&str, &str>, usize) {
    let mut fm = HashMap::new();
    if lines.first().map(String::as_str) != Some("---") {
        return (fm, 0);
    }
    for (i, line) in lines.iter().enumerate().skip(1) {
        if line == "---" {
            return (fm, i + 1);
        }
        let key_len = line
            .bytes()
            .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
            .count();
        if key_len > 0 && line.as_bytes().get(key_len) == Some(&b':') {
            // A later duplicate replaces the earlier value, as in the plugin.
            fm.insert(&line[..key_len], trim(&line[key_len + 1..]));
        }
    }
    (HashMap::new(), 0)
}

fn in_dirs(rel: &str, dirs: &[String]) -> bool {
    dirs.iter()
        .any(|d| rel == d || rel.strip_prefix(d.as_str()).is_some_and(|r| r.starts_with('/')))
}

fn fence_delim(line: &str) -> Option<(u8, usize)> {
    let t = line
        .trim_start_matches(|c: char| c.is_ascii() && is_ws(c as u8))
        .as_bytes();
    let ch = *t.first().filter(|&&c| c == b'`' || c == b'~')?;
    let len = t.iter().take_while(|&&c| c == ch).count();
    (len >= 3).then_some((ch, len))
}

/// The text of a heading line (`^#+%s+(.*)$`).
fn heading_text(line: &str) -> Option<&str> {
    let rest = line.trim_start_matches('#');
    if rest.len() == line.len() || !rest.bytes().next().is_some_and(is_ws) {
        return None;
    }
    Some(rest.trim_start_matches(|c: char| c.is_ascii() && is_ws(c as u8)))
}

/// Lua's greedy `%s+(.+)$` after position `at`: the whitespace run, leaving at
/// least one character for the capture. None when there is no whitespace or
/// nothing after it.
fn after_ws(line: &str, at: usize) -> Option<&str> {
    let rest = &line[at..];
    let ws = rest.bytes().take_while(|&b| is_ws(b)).count();
    if ws == 0 || rest.is_empty() {
        return None;
    }
    if ws < rest.len() {
        Some(&rest[ws..])
    } else if ws >= 2 {
        // Only whitespace: `%s+` gives back one byte for `.+`.
        Some(&rest[ws - 1..])
    } else {
        None
    }
}

/// A checkbox line (§8.1): its kind and raw text.
fn checkbox_item(line: &str) -> Option<(Kind, &str)> {
    let (at, mark) = crate::task::checkbox(line)?;
    let kind = match mark {
        b' ' => Kind::Open,
        b'x' | b'X' => Kind::Done,
        b'-' => Kind::Cancelled,
        _ => return None,
    };
    after_ws(line, at + 3).map(|raw| (kind, raw))
}

/// The text of a bullet that is not a checkbox (`^%s*[-*+]%s+(.+)$` or
/// `^%s*%d+[%.%)]%s+(.+)$`).
fn bullet_text(line: &str) -> Option<&str> {
    let lead = line.bytes().take_while(|&b| is_ws(b)).count();
    let b = &line.as_bytes()[lead..];
    match b.first()? {
        b'-' | b'*' | b'+' => after_ws(line, lead + 1),
        c if c.is_ascii_digit() => {
            let digits = b.iter().take_while(|c| c.is_ascii_digit()).count();
            match b.get(digits) {
                Some(b'.' | b')') => after_ws(line, lead + digits + 1),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Indent width, a tab counting as four.
fn indent_width(line: &str) -> usize {
    line.bytes()
        .take_while(|&b| b == b' ' || b == b'\t')
        .map(|b| if b == b'\t' { 4 } else { 1 })
        .sum()
}

/// Removes `#tag` wherever the next byte is not alphanumeric (`%f[%W]`).
fn remove_tag(text: &str, tag: &str) -> String {
    let needle = format!("#{tag}");
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find(&needle) {
        let end = i + needle.len();
        let boundary = rest.as_bytes().get(end).is_none_or(|c| !c.is_ascii_alphanumeric());
        out.push_str(&rest[..i]);
        if !boundary {
            out.push_str(&needle);
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

fn squeeze(text: &str) -> String {
    text.split(|c: char| c.is_ascii() && is_ws(c as u8))
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Cuts text to `width` display columns, ending in `…` when cut.
fn truncate(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_string();
    }
    let mut out = String::new();
    for c in text.chars() {
        let next = format!("{out}{c}");
        if next.width() > width - 1 {
            break;
        }
        out = next;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> Config {
        let mut c = Config::default();
        c.tasks.require_tag = Some("todo".into());
        c.tasks.since_days = Some(60);
        c.tasks.always = vec!["tasks/active.md".into()];
        c.tasks.date_patterns = vec![r"^\*\*作成日\*\*:\s*([0-9]{4}-[0-9]{2}-[0-9]{2})".into()];
        c
    }

    fn scan(cfg: &Config, rel: &str, text: &str, cutoff: Option<&str>) -> Vec<Task> {
        let lines: Vec<String> = text.lines().map(str::to_string).collect();
        let s = Scanner::new(cfg).unwrap();
        let mut all = Vec::new();
        s.scan_note(rel, &lines, cutoff, &mut all);
        all
    }

    #[test]
    fn worked_example() {
        let c = cfg();
        let all = scan(
            &c,
            "n.md",
            "- [-] (A) ~~#projectx review draft #todo due:2026-09-14~~ cancelled:2026-09-16 ^a1b2c3",
            None,
        );
        let t = &all[0];
        assert!(t.cancelled);
        assert_eq!(t.priority, Some('A'));
        assert_eq!(t.due.as_deref(), Some("2026-09-14"));
        assert_eq!(t.cancelled_at.as_deref(), Some("2026-09-16"));
        assert_eq!(t.id.as_deref(), Some("a1b2c3"));
        assert_eq!(t.text, "#projectx review draft #todo due:2026-09-14");
    }

    #[test]
    fn grammar_edges() {
        let c = cfg();
        let all = scan(
            &c,
            "n.md",
            "- [ ]\n- [ ]  \n+ [ ] plus\n1. [ ] num\n- [>] arrow\n* [x] star done:2026-07-17 10:00",
            None,
        );
        // `- [ ]  ` is a task with empty text; `- [ ]`, `+`, `1.` and `[>]` are not.
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].text, "");
        assert!(all[1].done);
        assert_eq!(all[1].done_at.as_deref(), Some("2026-07-17 10:00"));
        assert_eq!(all[1].text, "star");
    }

    #[test]
    fn nesting_and_tags() {
        let c = cfg();
        let text = "\
- [ ] (B) parent #todo due:2026-07-31
  - [x] (A) child done:2026-07-31 08:58
  - note
    - [ ] grandchild under a bullet
- [ ] untagged
Paragraph at the margin.
  - [ ] indented after the paragraph
# Heading
- [ ] #todo-x is another tag
```
- [ ] #todo in a fence
```";
        let all = scan(&c, "n.md", text, None);
        let texts: Vec<(&str, bool, usize)> = all.iter().map(|t| (t.text.as_str(), t.tagged, t.depth)).collect();
        assert_eq!(
            texts,
            vec![
                ("parent #todo due:2026-07-31", true, 0),
                ("child", true, 1),
                ("grandchild under a bullet", true, 1),
                ("untagged", false, 0),
                ("indented after the paragraph", false, 0),
                ("#todo-x is another tag", false, 0),
            ]
        );
        assert_eq!(all[0].children, 2);
        assert_eq!(all[0].children_closed, 1);
        assert_eq!(all[2].context.as_deref(), Some("note"));
        assert_eq!(all[1].parent, Some(0));
        assert_eq!(all[4].parent, None);
    }

    #[test]
    fn dates_and_aging() {
        let c = cfg();
        let s = Scanner::new(&c).unwrap();
        let lines = |t: &str| t.lines().map(str::to_string).collect::<Vec<_>>();
        let d = |rel: &str, t: &str| {
            let l = lines(t);
            let (fm, _) = frontmatter(&l);
            s.note_date(rel, &l, &fm)
        };
        assert_eq!(d("x.md", "# T\n**作成日**: 2026-03-12").as_deref(), Some("2026-03-12"));
        assert_eq!(d("x.md", "**作成日**：2026-03-12"), None);
        assert_eq!(d("a/2026-09-18 meeting.md", "").as_deref(), Some("2026-09-18"));
        assert_eq!(d("20260514-1030.md", ""), None);
        assert_eq!(
            d("x.md", "---\ncreated: 2026-01-02\n---").as_deref(),
            Some("2026-01-02")
        );
        assert_eq!(d("x.md", "---\ncreated: 2026-01-02"), None);

        // An old note keeps its tagged tasks and loses its inbox items.
        let old = "- [ ] tagged #todo\n- [ ] untagged";
        let all = scan(&c, "lognote/2025-01-01.md", old, Some("2026-08-01"));
        let tasks = select(all.clone(), Want::default(), &Filter::default(), Sort::Priority, false);
        let inbox = select(
            all,
            Want {
                inbox: true,
                ..Want::default()
            },
            &Filter::default(),
            Sort::Priority,
            false,
        );
        assert_eq!(tasks.len(), 1);
        assert!(inbox.is_empty());
        // tasks/active.md never ages out.
        let all = scan(
            &c,
            "tasks/active.md",
            "---\ndate: 2025-01-01\n---\n- [ ] untagged",
            Some("2026-08-01"),
        );
        assert_eq!(
            select(
                all,
                Want {
                    inbox: true,
                    ..Want::default()
                },
                &Filter::default(),
                Sort::Priority,
                false
            )
            .len(),
            1
        );
    }

    #[test]
    fn opting_out_and_ignored_dirs() {
        let c = cfg();
        assert!(scan(&c, "n.md", "---\ntasks: false\n---\n- [ ] x #todo", None).is_empty());
        assert!(scan(&c, "templates/t.md", "- [ ] x #todo", None).is_empty());
        assert_eq!(scan(&c, "templatesx/t.md", "- [ ] x #todo", None).len(), 1);
    }

    #[test]
    fn sorting_keeps_subtasks_under_their_root() {
        let c = cfg();
        let text = "\
- [ ] (B) second #todo
  - [ ] (A) step of second
- [ ] (A) first #todo due:2026-10-02
- [ ] none #todo";
        let all = scan(&c, "n.md", text, None);
        let sorted = select(all, Want::default(), &Filter::default(), Sort::Priority, false);
        let order: Vec<&str> = sorted.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(
            order,
            vec![
                "first #todo due:2026-10-02",
                "second #todo",
                "step of second",
                "none #todo"
            ]
        );
    }

    #[test]
    fn entries() {
        let c = cfg();
        let s = Scanner::new(&c).unwrap();
        let text = "\
- context bullet that is rather long and keeps going past the cut
  - [ ] (A) task #todo due:2026-10-02 ^a1b2c3
- [ ] parent #todo
  - [x] child done:2026-09-01 10:00
  - [ ] other child";
        let all = scan(&c, "n.md", text, None);
        let shown = select(all.clone(), Want::default(), &Filter::default(), Sort::Priority, false);
        let e: Vec<String> = shown.iter().map(|t| s.entry_text(t)).collect();
        assert_eq!(
            e,
            vec![
                "(A) task  [due 2026-10-02]  ← context bullet that is rather long and …",
                "parent  [1/2]",
                "  ↳ other child",
            ]
        );
        assert_eq!(truncate("日本語のテキスト", 7), "日本語…");
    }

    #[test]
    fn filters_reverse_and_groups() {
        let c = cfg();
        let text = "\
- [ ] (A) job #todo #budget due:2026-10-10
  - [ ] step one
  - [x] step two done:2026-10-01 09:00
- [ ] (B) later #todo due:2026-12-01
- [ ] (C) plain #todo
- [-] (A) dropped #todo #budget cancelled:2026-10-02";
        let all = scan(&c, "n.md", text, None);
        assert_eq!(all[1].tags, vec!["budget"]);
        assert_eq!(all[1].due_in_effect.as_deref(), Some("2026-10-10"));
        let f = |state, tag: Option<&str>, due| Filter {
            state,
            tag: tag.map(str::to_string),
            due,
            today: "2026-10-12".into(),
            week_end: "2026-10-19".into(),
        };
        let texts = |v: Vec<Task>| v.into_iter().map(|t| t.lineno).collect::<Vec<_>>();
        let pick =
            |filter: &Filter, reverse| texts(select(all.clone(), Want::default(), filter, Sort::Priority, reverse));
        assert_eq!(pick(&f(State::Open, None, None), false), vec![1, 2, 4, 5]);
        // Reversed: the items turn round, the job's steps stay in order.
        assert_eq!(pick(&f(State::Open, None, None), true), vec![5, 4, 1, 2]);
        assert_eq!(pick(&f(State::All, None, None), false), vec![1, 2, 3, 6, 4, 5]);
        assert_eq!(pick(&f(State::Closed, None, None), false), vec![3, 6]);
        assert_eq!(pick(&f(State::Open, Some("budget"), None), false), vec![1, 2]);
        assert_eq!(pick(&f(State::Open, None, Some(DueFilter::Overdue)), false), vec![1, 2]);
        assert_eq!(pick(&f(State::Open, None, Some(DueFilter::Week)), false), vec![1, 2]);
        assert_eq!(pick(&f(State::Open, None, Some(DueFilter::None)), false), vec![5]);
        // The closed-only view drops the open job, so its done step is orphaned.
        let closed = select(
            all.clone(),
            Want::default(),
            &f(State::Closed, None, None),
            Sort::Priority,
            false,
        );
        assert!(closed[0].orphaned);

        let shown = select(
            all.clone(),
            Want::default(),
            &f(State::Open, None, None),
            Sort::Priority,
            false,
        );
        let by_tag = group(&shown, Group::Tag, "2026-10-12", "2026-10-19");
        let keys: Vec<&GroupKey> = by_tag.iter().map(|(k, _)| k).collect();
        assert_eq!(keys, vec![&GroupKey::Tag("budget".into()), &GroupKey::NoTag]);
        assert_eq!(by_tag[0].1, vec![(0, false), (1, false)]);
        let by_due = group(&shown, Group::Due, "2026-10-12", "2026-10-19");
        let keys: Vec<&GroupKey> = by_due.iter().map(|(k, _)| k).collect();
        assert_eq!(
            keys,
            vec![
                &GroupKey::Due(Bucket::Overdue),
                &GroupKey::Due(Bucket::Later),
                &GroupKey::Due(Bucket::None)
            ]
        );
        assert_eq!(tags_in(&shown), vec!["budget"]);
        assert_eq!(
            bucket(Some("2026-10-12T15:00"), "2026-10-12", "2026-10-19"),
            Bucket::Today
        );
    }
}
