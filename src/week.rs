//! Week views and the week digest (format.md §11).

use chrono::{Datelike, Duration, NaiveDate};

use crate::config::Digest;
use crate::tasks::Task;

/// An ISO week: its label (`%G-W%V`), Monday and Sunday.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Range {
    pub label: String,
    pub from: String,
    pub to: String,
    pub monday: NaiveDate,
}

impl Range {
    fn of(monday: NaiveDate) -> Range {
        Range {
            label: monday.format("%G-W%V").to_string(),
            from: monday.format("%Y-%m-%d").to_string(),
            to: (monday + Duration::days(6)).format("%Y-%m-%d").to_string(),
            monday,
        }
    }
}

fn monday_of(d: NaiveDate) -> NaiveDate {
    d - Duration::days(i64::from(d.weekday().num_days_from_monday()))
}

/// The week a spec names: empty for this week, `-1` / `+2` relative to it,
/// `2026-W37` (also `2026w37`, `2026W37`), or a date in the week.
pub fn range(spec: &str, today: NaiveDate) -> Result<Range, String> {
    let spec = crate::trim(spec);
    if spec.is_empty() {
        return Ok(Range::of(monday_of(today)));
    }
    let digits = spec.strip_prefix(['-', '+']).unwrap_or(spec);
    if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
        let n: i64 = spec.parse().map_err(|_| format!("Not a week: {spec:?}."))?;
        return Ok(Range::of(monday_of(today + Duration::days(n * 7))));
    }
    let b = spec.as_bytes();
    if b.len() >= 6 && b[..4].iter().all(u8::is_ascii_digit) {
        let rest = spec[4..].strip_prefix('-').unwrap_or(&spec[4..]);
        if let Some(w) = rest.strip_prefix(['W', 'w'])
            && (1..=2).contains(&w.len())
            && w.bytes().all(|c| c.is_ascii_digit())
        {
            let year: i32 = spec[..4].parse().unwrap_or(0);
            let week: i64 = w.parse().unwrap_or(0);
            if !(1..=53).contains(&week) {
                return Err(format!("There is no week {week}."));
            }
            let jan4 = NaiveDate::from_ymd_opt(year, 1, 4).ok_or("bad year")?;
            return Ok(Range::of(monday_of(jan4) + Duration::days((week - 1) * 7)));
        }
    }
    if crate::task::valid_due(spec)
        && spec.len() == 10
        && let Ok(d) = NaiveDate::parse_from_str(spec, "%Y-%m-%d")
    {
        return Ok(Range::of(monday_of(d)));
    }
    Err(format!(
        "Expected a week like 2026-W37, a date like 2026-09-10, or -1 for last week; got {spec:?}."
    ))
}

/// The weeks after `range`, for the digest's look ahead.
pub fn ahead_of(range: &Range, weeks: u32) -> Range {
    let from = range.monday + Duration::days(7);
    let to = range.monday + Duration::days(7 * i64::from(weeks) + 6);
    let label = if weeks == 1 {
        from.format("%G-W%V").to_string()
    } else {
        format!("{}..{}", from.format("%G-W%V"), to.format("%G-W%V"))
    };
    Range {
        label,
        from: from.format("%Y-%m-%d").to_string(),
        to: to.format("%Y-%m-%d").to_string(),
        monday: from,
    }
}

/// `MM-DD Ddd` for a YYYY-MM-DD date.
pub fn day_label(date: &str) -> String {
    NaiveDate::parse_from_str(date, "%Y-%m-%d").map_or_else(|_| date.to_string(), |d| d.format("%m-%d %a").to_string())
}

/// A note's summary for the digest (`summarise`): a title (a leading H1 that
/// is the note's only H1), an outline of the other headings, up to `quote`
/// body lines, and whether there were more.
pub fn summarise<S: AsRef<str>>(lines: &[S], quote: usize) -> (Option<String>, Vec<String>, Vec<String>, bool) {
    let start = body_start(lines);
    let mut headings: Vec<(usize, String)> = Vec::new();
    let mut excerpt = Vec::new();
    let mut more = false;
    let mut in_fence: Option<String> = None;
    let (mut first_content, mut opens_with_h1, mut h1s) = (true, false, 0);
    for line in lines.iter().skip(start).map(AsRef::as_ref) {
        if let Some(fence) = fence_delim(line) {
            match &in_fence {
                None => in_fence = Some(fence),
                Some(open) => {
                    let t = line.trim_start_matches(|c: char| c.is_ascii() && crate::is_ws(c as u8));
                    if t.starts_with(open.as_str()) {
                        in_fence = None;
                    }
                }
            }
        }
        if let Some((level, text)) = heading(line).filter(|(_, t)| !t.is_empty())
            && in_fence.is_none()
        {
            if level == 1 {
                h1s += 1;
                opens_with_h1 = opens_with_h1 || first_content;
            }
            headings.push((level, text.to_string()));
            first_content = false;
        } else if line.bytes().any(|b| !crate::is_ws(b)) {
            first_content = false;
            if excerpt.len() < quote {
                excerpt.push(line.to_string());
            } else {
                more = true;
            }
        }
    }
    let title = (opens_with_h1 && h1s == 1).then(|| headings.remove(0).1);
    let top = headings.iter().map(|(l, _)| *l).min().unwrap_or(1);
    let outline = headings
        .iter()
        .map(|(l, t)| format!("{}- {t}", "  ".repeat(l - top)))
        .collect();
    (title, outline, excerpt, more)
}

fn body_start<S: AsRef<str>>(lines: &[S]) -> usize {
    if lines.first().map(AsRef::as_ref) != Some("---") {
        return 0;
    }
    lines
        .iter()
        .skip(1)
        .position(|l| l.as_ref() == "---")
        .map_or(0, |p| p + 2)
}

/// The opening run of a fence (```` ``` ```` or `~~~`, any length).
fn fence_delim(line: &str) -> Option<String> {
    let t = line.trim_start_matches(|c: char| c.is_ascii() && crate::is_ws(c as u8));
    let ch = t.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let run: String = t.chars().take_while(|c| *c == ch).collect();
    (run.len() >= 3).then_some(run)
}

/// `^(#+)%s+(.-)%s*$`: the level and the text.
fn heading(line: &str) -> Option<(usize, &str)> {
    let level = line.bytes().take_while(|b| *b == b'#').count();
    let rest = &line[level..];
    if level == 0 || !rest.bytes().next().is_some_and(crate::is_ws) {
        return None;
    }
    Some((level, crate::trim(rest)))
}

/// A note dated within the week.
pub struct WeekNote {
    pub rel: String,
    pub date: String,
    pub name: String,
    pub lines: Vec<String>,
}

/// Calendar lines and the event count for a span (from, to inclusive).
pub type CalendarFn<'a> = Box<dyn Fn(&str, &str) -> (Vec<String>, usize) + 'a>;

/// What the digest needs from outside: the calendar and the sources.
pub struct Extras<'a> {
    /// Calendar lines and the event count for a span, when the calendar is
    /// enabled.
    pub calendar: Option<CalendarFn<'a>>,
    /// Each source's label and output lines.
    pub sources: Vec<(String, Vec<String>)>,
}

/// The digest's lines (`digest_lines`). `tasks` are the tagged tasks over all
/// notes, done ones included and cancelled ones not, in "added" order.
pub fn digest_lines(
    d: &Digest,
    range: &Range,
    weekly_name: &str,
    notes: &[WeekNote],
    tasks: &[Task],
    extras: &Extras<'_>,
) -> Vec<String> {
    let l = &d.labels;
    let ahead = (d.ahead > 0).then(|| ahead_of(range, d.ahead));
    let calendar = extras.calendar.as_ref().filter(|_| d.calendar);
    let mut out: Vec<String> = Vec::new();
    let section = |out: &mut Vec<String>, title: String, rows: Vec<String>| {
        out.push(String::new());
        out.push(format!("## {title}"));
        out.push(String::new());
        out.extend(rows);
    };
    out.push(format!("# {}  {} to {}", range.label, range.from, range.to));
    out.push(String::new());
    out.push(format!(
        "{}, [[{weekly_name}]]",
        l.notes.replace("{}", &notes.len().to_string())
    ));
    if let Some(cal) = calendar {
        let (rows, count) = cal(&range.from, &range.to);
        out.push(String::new());
        out.push(format!("## {} ({count})", l.calendar));
        out.push(String::new());
        out.extend(rows);
    }
    for n in notes {
        let (title, outline, excerpt, more) = summarise(&n.lines, d.lines);
        out.push(String::new());
        let mut h = format!("## {}  [[{}]]", day_label(&n.date), n.name);
        if let Some(t) = title.filter(|t| *t != n.name) {
            h.push_str(" -- ");
            h.push_str(&t);
        }
        out.push(h);
        if !outline.is_empty() {
            out.push(String::new());
            out.extend(outline);
        }
        if !excerpt.is_empty() {
            out.push(String::new());
            out.extend(excerpt.iter().map(|e| format!("> {e}")));
            if more {
                out.push("> …".into());
            }
        }
    }
    let task_line = |t: &Task, mark: &str| {
        let text = match t.priority {
            Some(p) => format!("({p}) {}", t.text),
            None => t.text.clone(),
        };
        let mut target = crate::link::note_name(std::path::Path::new(&t.rel));
        if let Some(id) = &t.id {
            target.push_str("#^");
            target.push_str(id);
        }
        format!("- {mark} {text}  ([[{target}]])")
    };
    let mut due_ahead: Vec<&Task> = Vec::new();
    if d.tasks {
        let in_week: Vec<&str> = notes.iter().map(|n| n.rel.as_str()).collect();
        let mut finished: Vec<&Task> = Vec::new();
        let mut open: Vec<&Task> = Vec::new();
        for t in tasks {
            let done_day = t.done_at.as_deref().map(|s| &s[..s.len().min(10)]);
            if t.done && done_day.is_some_and(|d| d >= range.from.as_str() && d <= range.to.as_str()) {
                finished.push(t);
            } else if !t.done && !t.cancelled {
                if in_week.contains(&t.rel.as_str()) {
                    open.push(t);
                }
                let due = t.due.as_deref().map(|s| &s[..s.len().min(10)]);
                if let (Some(a), Some(due)) = (&ahead, due)
                    && due >= a.from.as_str()
                    && due <= a.to.as_str()
                {
                    due_ahead.push(t);
                }
            }
        }
        // Equal keys fall back to path, then line, as in the plugin.
        finished.sort_by(|a, b| {
            a.done_at
                .cmp(&b.done_at)
                .then(a.rel.cmp(&b.rel))
                .then(a.lineno.cmp(&b.lineno))
        });
        due_ahead.sort_by(|a, b| a.due.cmp(&b.due).then(a.rel.cmp(&b.rel)).then(a.lineno.cmp(&b.lineno)));
        if !finished.is_empty() {
            let rows = finished.iter().map(|t| task_line(t, &d.marks.done)).collect();
            section(&mut out, format!("{} ({})", l.finished, finished.len()), rows);
        }
        if !open.is_empty() {
            let rows = open.iter().map(|t| task_line(t, &d.marks.open)).collect();
            section(&mut out, format!("{} ({})", l.open, open.len()), rows);
        }
    }
    for (label, lines) in &extras.sources {
        section(&mut out, label.clone(), lines.clone());
    }
    if let Some(a) = &ahead {
        if let Some(cal) = calendar {
            let (rows, count) = cal(&a.from, &a.to);
            section(&mut out, format!("{} ({count})  {} to {}", l.ahead, a.from, a.to), rows);
        }
        if !due_ahead.is_empty() {
            let rows = due_ahead.iter().map(|t| task_line(t, &d.marks.open)).collect();
            section(
                &mut out,
                format!("{} {} to {} ({})", l.due, a.from, a.to, due_ahead.len()),
                rows,
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn ranges() {
        let wed = d(2026, 10, 7);
        let r = range("", wed).unwrap();
        assert_eq!(
            (r.label.as_str(), r.from.as_str(), r.to.as_str()),
            ("2026-W41", "2026-10-05", "2026-10-11")
        );
        assert_eq!(range("-1", wed).unwrap().label, "2026-W40");
        assert_eq!(range("+2", wed).unwrap().label, "2026-W43");
        assert_eq!(range("2026-W37", wed).unwrap().from, "2026-09-07");
        assert_eq!(range("2026w37", wed).unwrap().from, "2026-09-07");
        assert_eq!(range("2026W1", wed).unwrap().from, "2025-12-29");
        assert_eq!(range("2026-09-10", wed).unwrap().label, "2026-W37");
        assert_eq!(range("2025-12-31", wed).unwrap().label, "2026-W01");
        assert!(range("2026-W54", wed).is_err());
        assert!(range("next", wed).is_err());
        let a = ahead_of(&range("", wed).unwrap(), 2);
        assert_eq!(
            (a.label.as_str(), a.from.as_str(), a.to.as_str()),
            ("2026-W42..2026-W43", "2026-10-12", "2026-10-25")
        );
    }

    #[test]
    fn summaries() {
        let lines = [
            "---",
            "title: x",
            "---",
            "# Meeting with Acme",
            "",
            "intro line",
            "## Agenda",
            "- a",
            "### Detail",
            "```",
            "# not a heading",
            "```",
            "more",
            "and more",
        ];
        let (title, outline, excerpt, more) = summarise(&lines, 3);
        assert_eq!(title.as_deref(), Some("Meeting with Acme"));
        assert_eq!(outline, vec!["- Agenda", "  - Detail"]);
        assert_eq!(excerpt, vec!["intro line", "- a", "```"]);
        assert!(more);
        // Two H1s: neither is the title.
        let (title, outline, _, _) = summarise(&["# A", "# B"], 8);
        assert_eq!(title, None);
        assert_eq!(outline, vec!["- A", "- B"]);
    }
}
