//! Note names and locations (§2, §4, §5).

use std::path::{Path, PathBuf};

use chrono::{Duration, NaiveDateTime, NaiveTime};

use crate::config::Config;
use crate::is_ws;

/// Daily and weekly notes. Their directory, name format and template come from
/// the `notes.daily` / `notes.weekly` settings (§1, §2.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Periodic {
    Daily,
    Weekly,
}

impl Periodic {
    pub fn dir(self, cfg: &Config) -> &str {
        match self {
            Periodic::Daily => &cfg.notes.daily.dir,
            Periodic::Weekly => &cfg.notes.weekly.dir,
        }
    }

    /// Weeks default to the ISO year (`%G`). The calendar year `%Y` would give
    /// the week around New Year two names (§14 Q1).
    pub fn format(self, cfg: &Config) -> &str {
        match self {
            Periodic::Daily => &cfg.notes.daily.format,
            Periodic::Weekly => &cfg.notes.weekly.format,
        }
    }

    pub fn template(self, cfg: &Config) -> &str {
        match self {
            Periodic::Daily => &cfg.notes.daily.template,
            Periodic::Weekly => &cfg.notes.weekly.template,
        }
    }

    pub fn name(self, cfg: &Config, t: NaiveDateTime) -> String {
        t.format(self.format(cfg)).to_string()
    }

    /// The path relative to home.
    pub fn rel(self, cfg: &Config, t: NaiveDateTime) -> String {
        format!("{}/{}.{}", self.dir(cfg), self.name(cfg, t), cfg.extension)
    }
}

/// Moves t by n calendar days and sets the time to 12:00 (§0.3).
pub fn days_from(t: NaiveDateTime, n: i64) -> NaiveDateTime {
    (t.date() + Duration::days(n)).and_time(NaiveTime::from_hms_opt(12, 0, 0).unwrap())
}

/// A title of whitespace only creates no note (§4 step 1).
pub fn title_is_blank(title: &str) -> bool {
    title.bytes().all(is_ws)
}

/// `sanitize_filename` (§4 step 3). None when nothing is left.
pub fn sanitize_filename(title: &str) -> Option<String> {
    let kept: String = title
        .chars()
        .filter(|&c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .filter(|&c| !(c < ' ' || c == '\x7f'))
        .collect();
    // Control characters are gone, so the only ASCII whitespace left is ' '.
    let mut collapsed = String::with_capacity(kept.len());
    for word in kept.split(' ').filter(|w| !w.is_empty()) {
        if !collapsed.is_empty() {
            collapsed.push(' ');
        }
        collapsed.push_str(word);
    }
    let name = collapsed.trim_start_matches('.').trim_end_matches('.');
    (!name.is_empty()).then(|| name.to_string())
}

/// New notes always go directly under home (§4 step 4).
pub fn new_note_path(home: &Path, sanitized: &str, extension: &str) -> PathBuf {
    home.join(format!("{sanitized}.{extension}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(y: i32, m: u32, d: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(8, 0, 0)
            .unwrap()
    }

    #[test]
    fn sanitize() {
        let s = |t| sanitize_filename(t);
        assert_eq!(s("Meeting: Acme / Q3").as_deref(), Some("Meeting Acme Q3"));
        assert_eq!(s("  a\tb  ").as_deref(), Some("ab"));
        assert_eq!(s(". foo").as_deref(), Some(" foo"));
        assert_eq!(s("..a..").as_deref(), Some("a"));
        assert_eq!(s("日本語　メモ").as_deref(), Some("日本語　メモ"));
        assert_eq!(s("///"), None);
        assert_eq!(s("..."), None);
    }

    #[test]
    fn blank_title() {
        assert!(title_is_blank(" \t"));
        // The ideographic space is not whitespace.
        assert!(!title_is_blank("　"));
    }

    #[test]
    fn periodic_names() {
        let mut cfg = Config::default();
        cfg.notes.daily.dir = "lognote".into();
        cfg.notes.weekly.dir = "lognote".into();
        let (d, w) = (Periodic::Daily, Periodic::Weekly);
        assert_eq!(d.rel(&cfg, at(2026, 9, 29)), "lognote/2026-09-29.md");
        assert_eq!(w.rel(&cfg, at(2026, 9, 29)), "lognote/2026-W40.md");
        // Q1: the year is the ISO year.
        assert_eq!(w.name(&cfg, at(2025, 12, 29)), "2026-W01");
        assert_eq!(w.name(&cfg, at(2026, 1, 1)), "2026-W01");
        assert_eq!(w.name(&cfg, at(2027, 1, 1)), "2026-W53");
        // The defaults are the plugin's.
        assert_eq!(d.rel(&Config::default(), at(2026, 9, 29)), "daily/2026-09-29.md");
    }

    #[test]
    fn calendar_days() {
        let t = days_from(at(2026, 9, 30), 1);
        assert_eq!(t.to_string(), "2026-10-01 12:00:00");
        assert_eq!(days_from(at(2026, 1, 1), -1).to_string(), "2025-12-31 12:00:00");
    }
}
