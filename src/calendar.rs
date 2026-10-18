//! Calendar events from gcalcli, for `{{agenda}}`, `{{agenda_week}}` and the
//! week digest (format.md §12).
//!
//! The command prints a TSV with a header row; its columns are found by name.
//! Results are cached on disk for `cache.ttl` seconds, and when the command
//! fails, an older cached result is used and marked as stale.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Local, NaiveDate};

use crate::config::Calendar;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// Start date, YYYY-MM-DD.
    pub date: String,
    pub start: Option<String>,
    pub end_date: Option<String>,
    pub stop: Option<String>,
    pub title: String,
    pub location: Option<String>,
    pub id: Option<String>,
    pub all_day: bool,
    /// The last day the event covers (all-day end dates are exclusive).
    pub last: String,
}

/// Events, and whether they come from a cache the command failed to renew.
#[derive(Debug, Clone)]
pub struct Fetched {
    pub events: Vec<Event>,
    pub at: SystemTime,
    /// Set when the command failed and an older cache stands in: the error.
    pub stale: Option<String>,
}

fn day(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

fn shift(date: &str, days: i64) -> String {
    day(date).map_or_else(
        || date.to_string(),
        |d| (d + chrono::Duration::days(days)).format("%Y-%m-%d").to_string(),
    )
}

/// Reads gcalcli's `agenda --tsv` output (`parse_tsv`).
pub fn parse_tsv(text: &str) -> Result<Vec<Event>, String> {
    let mut lines = text.split('\n');
    let header = match lines.next() {
        Some(h) if !h.is_empty() => h,
        _ => return Ok(Vec::new()),
    };
    let cols: Vec<&str> = header.split('\t').collect();
    let at = |name: &str| cols.iter().rposition(|c| *c == name);
    if at("start_date").is_none() || at("title").is_none() {
        return Err("not a gcalcli TSV (no start_date/title columns)".into());
    }
    let mut events = Vec::new();
    for line in lines.filter(|l| !l.is_empty()) {
        let fields: Vec<&str> = line.split('\t').collect();
        let col = |name: &str| {
            at(name)
                .and_then(|i| fields.get(i))
                .filter(|v| !v.is_empty())
                .map(|v| v.to_string())
        };
        let Some(date) = col("start_date").filter(|d| crate::task::valid_due(d) && d.len() == 10) else {
            continue;
        };
        let start = col("start_time");
        let end_date = col("end_date");
        let all_day = start.is_none();
        let mut last = match &end_date {
            Some(e) if all_day && *e > date => shift(e, -1),
            Some(e) => e.clone(),
            None => date.clone(),
        };
        if last < date {
            last = date.clone();
        }
        events.push(Event {
            title: col("title").unwrap_or_default(),
            stop: col("end_time"),
            location: col("location"),
            id: col("id"),
            date,
            start,
            end_date,
            all_day,
            last,
        });
    }
    sort(&mut events);
    Ok(events)
}

/// By date; all-day events first; then by start time; then by title.
fn sort(events: &mut [Event]) {
    events.sort_by(|a, b| {
        a.date
            .cmp(&b.date)
            .then(b.all_day.cmp(&a.all_day))
            .then(a.start.as_deref().unwrap_or("").cmp(b.start.as_deref().unwrap_or("")))
            .then(a.title.cmp(&b.title))
    });
}

/// The command for events from `from` up to (not including) `to`: the
/// configured `command` with `{from}` and `{to}`, or gcalcli's agenda.
fn command(cal: &Calendar, from: &str, to: &str) -> Vec<String> {
    if let Some(cmd) = cal.command.as_deref().filter(|c| !c.is_empty()) {
        return vec![
            "sh".into(),
            "-c".into(),
            cmd.replace("{from}", from).replace("{to}", to),
        ];
    }
    let mut argv: Vec<String> = [
        cal.gcalcli.as_str(),
        "--nocolor",
        "agenda",
        "--tsv",
        "--details",
        "location",
        "--details",
        "id",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for name in cal.name.names() {
        argv.push("--calendar".into());
        argv.push(name.to_string());
    }
    argv.push(from.into());
    argv.push(to.into());
    argv
}

fn cache_dir(cal: &Calendar) -> Option<PathBuf> {
    if let Some(dir) = cal.cache.dir.as_deref().filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    std::env::var_os("XDG_CACHE_HOME")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .map(|b| b.join("ttykasten").join("calendar"))
}

fn cache_key(argv: &[String]) -> String {
    crate::tasks::line_hash(&argv.join("\u{1f}"))
}

/// Events from `from` up to `to` (exclusive): from a fresh cache, else from
/// the command, else from an old cache marked stale.
pub fn fetch(cal: &Calendar, from: &str, to: &str) -> Result<Fetched, String> {
    fetch_again(cal, from, to, false)
}

/// `fetch`, or with `refresh` the command first, whatever the cache's age.
pub fn fetch_again(cal: &Calendar, from: &str, to: &str, refresh: bool) -> Result<Fetched, String> {
    if !cal.enabled {
        return Err("calendar.enabled is false".into());
    }
    let argv = command(cal, from, to);
    let file = cache_dir(cal).map(|d| {
        d.join(format!(
            "{}.tsv",
            cache_key(&[from.into(), to.into(), argv.join("\u{1f}")])
        ))
    });
    let written = file
        .as_ref()
        .and_then(|f| fs::metadata(f).and_then(|m| m.modified()).ok());
    let fresh = written.is_some_and(|w| w.elapsed().is_ok_and(|age| age.as_secs() < cal.cache.ttl));
    let cached = |at: SystemTime, stale: Option<String>| {
        let text = fs::read_to_string(file.as_ref()?).ok()?;
        parse_tsv(&text).ok().map(|events| Fetched { events, at, stale })
    };
    if fresh
        && !refresh
        && let Some(f) = cached(written.unwrap(), None)
    {
        return Ok(f);
    }
    let why = match crate::command::run(&argv, Duration::from_millis(cal.timeout)) {
        Ok(text) => match parse_tsv(&text) {
            Ok(events) => {
                if let Some(f) = &file {
                    if let Some(dir) = f.parent() {
                        let _ = fs::create_dir_all(dir);
                    }
                    let _ = fs::write(f, &text);
                }
                return Ok(Fetched {
                    events,
                    at: SystemTime::now(),
                    stale: None,
                });
            }
            Err(e) => e,
        },
        Err(e) => e,
    };
    match written.and_then(|w| cached(w, Some(why.clone()))) {
        Some(f) => Ok(f),
        None => Err(why),
    }
}

/// One row of the agenda: an event, or the line that marks today.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Today,
    Event(usize),
}

/// The agenda's rows (`rows`): each event as `MM-DD Ddd  ` and its line, in
/// order, with a line for today before the first event from today on (or
/// last, when every event is past). Each row says whether it is past.
pub fn rows(cal: &Calendar, events: &[Event], today: &str, today_word: &str) -> Vec<(Row, String, bool)> {
    let label = |d: &str| day(d).map_or_else(|| d.to_string(), |d| d.format("%m-%d %a").to_string());
    let rule = (
        Row::Today,
        format!("{} {today_word} {} {}", "─".repeat(8), label(today), "─".repeat(24)),
        false,
    );
    let mut out = Vec::new();
    let mut drawn = false;
    for (i, ev) in events.iter().enumerate() {
        if !drawn && ev.date.as_str() >= today {
            out.push(rule.clone());
            drawn = true;
        }
        out.push((
            Row::Event(i),
            format!("{}  {}", label(&ev.date), format(cal, ev)),
            ev.date.as_str() < today,
        ));
    }
    if !drawn {
        out.push(rule);
    }
    out
}

/// The weeks `spec` names, as one range: a week as `week::range` reads it,
/// or two joined by `..` (`0..1` this week and next, `-1..0`, `..1`).
pub fn span(spec: &str, today: NaiveDate) -> Result<crate::week::Range, String> {
    let spec = crate::trim(spec);
    let Some((first, second)) = spec.split_once("..") else {
        return crate::week::range(spec, today);
    };
    let mut a = crate::week::range(first, today)?;
    let mut b = crate::week::range(second, today)?;
    if b.monday < a.monday {
        std::mem::swap(&mut a, &mut b);
    }
    if a.label == b.label {
        return Ok(a);
    }
    Ok(crate::week::Range {
        label: format!("{}..{}", a.label, b.label),
        to: b.to,
        ..a
    })
}

/// The events that cover `day`.
pub fn on_day<'a>(events: &'a [Event], day: &str) -> Vec<&'a Event> {
    events
        .iter()
        .filter(|e| e.date.as_str() <= day && day <= e.last.as_str())
        .collect()
}

/// One event as a line: `HH:MM-HH:MM  title  @location`, `all day  title`, or
/// `all day (MM-DD..MM-DD)  title`.
pub fn format(cal: &Calendar, ev: &Event) -> String {
    let when = if ev.all_day {
        let mut w = cal.labels.all_day.clone();
        if ev.last > ev.date {
            w = format!("{w} ({}..{})", &ev.date[5..], &ev.last[5..]);
        }
        w
    } else {
        let start = ev.start.clone().unwrap_or_default();
        match &ev.stop {
            Some(stop) => format!("{start}-{stop}"),
            None => start,
        }
    };
    let mut line = format!("{when}  {}", ev.title);
    if let Some(loc) = &ev.location {
        line.push_str("  @");
        line.push_str(loc);
    }
    line
}

/// " (error; as of MM-DD HH:MM)" for a stale result, "" for a fresh one.
pub fn staleness(f: &Fetched) -> String {
    match &f.stale {
        None => String::new(),
        Some(e) => {
            let at: DateTime<Local> = f.at.into();
            format!(" ({e}; as of {})", at.format("%m-%d %H:%M"))
        }
    }
}

/// `{{agenda}}`: the events of `date`, one `- ` line each.
pub fn day_lines(cal: &Calendar, date: NaiveDate) -> Vec<String> {
    let d = date.format("%Y-%m-%d").to_string();
    let b = &cal.bullet;
    let unavailable = &cal.labels.unavailable;
    let f = match fetch(cal, &d, &shift(&d, 1)) {
        Ok(f) => f,
        Err(e) => return vec![format!("{b}({unavailable}: {e})")],
    };
    let mut out: Vec<String> = on_day(&f.events, &d)
        .iter()
        .map(|e| format!("{b}{}", format(cal, e)))
        .collect();
    if f.stale.is_some() {
        out.push(format!("{b}({unavailable}{})", staleness(&f)));
    }
    out
}

/// The events from `from` to `to` (inclusive), grouped under `MM-DD Ddd`
/// lines, with the number of distinct events. Days without events are left
/// out.
pub fn range_lines(cal: &Calendar, from: &str, to: &str) -> (Vec<String>, usize) {
    let b = &cal.bullet;
    let unavailable = &cal.labels.unavailable;
    let f = match fetch(cal, from, &shift(to, 1)) {
        Ok(f) => f,
        Err(e) => return (vec![format!("{b}({unavailable}: {e})")], 0),
    };
    let (Some(mut d), Some(stop)) = (day(from), day(to)) else {
        return (Vec::new(), 0);
    };
    let mut out = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    while d <= stop {
        let ds = d.format("%Y-%m-%d").to_string();
        let on = on_day(&f.events, &ds);
        if !on.is_empty() {
            out.push(d.format("%m-%d %a").to_string());
            for e in on {
                out.push(format!("{b}{}", format(cal, e)));
                let id = e.id.clone().unwrap_or_else(|| format!("{}{}", e.date, e.title));
                if !seen.contains(&id) {
                    seen.push(id);
                }
            }
        }
        d += chrono::Duration::days(1);
    }
    if f.stale.is_some() {
        out.push(format!("({unavailable}{})", staleness(&f)));
    }
    (out, seen.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TSV: &str = "start_date\tstart_time\tend_date\tend_time\ttitle\tlocation\tid\n\
2026-10-05\t10:00\t2026-10-05\t11:00\tReview\tRoom 3\te1\n\
2026-10-05\t\t2026-10-08\t\tTrip\t\te2\n\
2026-10-05\t09:00\t2026-10-05\t09:30\tStandup\t\te3\n\
2026-10-07\t\t2026-10-08\t\tHoliday\t\te4\n\
bad\t\t\t\tskipped\t\t\n";

    #[test]
    fn parsing_and_order() {
        let ev = parse_tsv(TSV).unwrap();
        let titles: Vec<&str> = ev.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(titles, vec!["Trip", "Standup", "Review", "Holiday"]);
        assert_eq!(ev[0].last, "2026-10-07");
        assert_eq!(ev[3].last, "2026-10-07");
        assert!(parse_tsv("a\tb\n1\t2").is_err());
        assert!(parse_tsv("").unwrap().is_empty());
    }

    #[test]
    fn formatting() {
        let cal = Calendar::default();
        let ev = parse_tsv(TSV).unwrap();
        let f: Vec<String> = ev.iter().map(|e| format(&cal, e)).collect();
        assert_eq!(
            f,
            vec![
                "all day (10-05..10-07)  Trip",
                "09:00-09:30  Standup",
                "10:00-11:00  Review  @Room 3",
                "all day  Holiday"
            ]
        );
        assert_eq!(on_day(&ev, "2026-10-06").len(), 1);
    }

    #[test]
    fn disabled_calendar() {
        let cal = Calendar::default();
        let d = NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
        assert_eq!(
            day_lines(&cal, d),
            vec!["- (calendar unavailable: calendar.enabled is false)"]
        );
    }

    #[test]
    fn agenda_rows_and_spans() {
        let cal = Calendar::default();
        let ev = parse_tsv(TSV).unwrap();
        let rows = rows(&cal, &ev, "2026-10-06", "today");
        let text: Vec<&str> = rows.iter().map(|(_, t, _)| t.as_str()).collect();
        assert_eq!(text[3], "──────── today 10-06 Tue ────────────────────────");
        assert_eq!(text[4], "10-07 Wed  all day  Holiday");
        assert!(rows[0].2 && !rows[4].2);
        assert_eq!(rows[3].0, Row::Today);
        let all_past = super::rows(&cal, &ev, "2026-12-01", "today");
        assert_eq!(all_past.last().unwrap().0, Row::Today);

        let today = NaiveDate::from_ymd_opt(2026, 10, 7).unwrap();
        let s = span("0..1", today).unwrap();
        assert_eq!(
            (s.label.as_str(), s.from.as_str(), s.to.as_str()),
            ("2026-W41..2026-W42", "2026-10-05", "2026-10-18")
        );
        assert_eq!(span("1..0", today).unwrap().label, "2026-W41..2026-W42");
        assert_eq!(span("0..0", today).unwrap().label, "2026-W41");
        assert!(span("x..1", today).is_err());
    }
}
