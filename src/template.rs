//! Loading templates and substituting `{{KEY}}` (§3).

use std::path::Path;

use chrono::NaiveDateTime;

use crate::fileio;

/// `load_template(rel_path, title, time)`. An unreadable template gives
/// `"# " .. title`. The result has no trailing newline.
pub fn load(home: &Path, hdate: &str, rel: &str, title: &str, time: NaiveDateTime) -> String {
    let rest = rel.strip_prefix("templates/").unwrap_or(rel);
    let path = home.join("templates").join(rest);
    match fileio::read_lines(&path) {
        Ok(lines) => substitute(&lines.join("\n"), hdate, title, time),
        Err(_) => format!("# {title}"),
    }
}

fn value(key: &str, hdate: &str, title: &str, time: NaiveDateTime) -> Option<String> {
    let f = |fmt: &str| time.format(fmt).to_string();
    Some(match key {
        "title" => title.to_string(),
        "date" => f("%Y-%m-%d"),
        "hdate" => f(hdate),
        "year" => f("%Y"),
        "isoyear" => f("%G"),
        "month" => f("%m"),
        "day" => f("%d"),
        "week" => f("%V"),
        "time" => f("%H:%M"),
        // agenda / agenda_week need a calendar (gcalcli). Not supported yet, so
        // they are left intact like unknown keys.
        _ => return None,
    })
}

/// Substitutes the way Lua's `gsub("{{(.-)}}", …)` does. Keys match lazily
/// and are not trimmed. Unknown keys are left intact, braces and all.
pub fn substitute(text: &str, hdate: &str, title: &str, time: NaiveDateTime) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find("{{") {
        let Some(close) = rest[open + 2..].find("}}") else {
            break;
        };
        let key = &rest[open + 2..open + 2 + close];
        let end = open + 2 + close + 2;
        out.push_str(&rest[..open]);
        match value(key, hdate, title, time) {
            Some(v) => out.push_str(&v),
            None => out.push_str(&rest[open..end]),
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    const HDATE: &str = "%A, %B %d, %Y";

    fn t() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 9, 29)
            .unwrap()
            .and_hms_opt(8, 10, 0)
            .unwrap()
    }

    fn sub(s: &str) -> String {
        substitute(s, HDATE, "T", t())
    }

    #[test]
    fn keys() {
        assert_eq!(sub("{{hdate}}"), "Tuesday, September 29, 2026");
        assert_eq!(sub("{{date}} {{time}}"), "2026-09-29 08:10");
        assert_eq!(sub("{{year}}-W{{week}}"), "2026-W40");
        assert_eq!(sub("{{title}}"), "T");
    }

    #[test]
    fn isoyear() {
        let d = NaiveDate::from_ymd_opt(2025, 12, 29)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        assert_eq!(
            substitute("{{isoyear}}-W{{week}} {{year}}", HDATE, "", d),
            "2026-W01 2025"
        );
    }

    #[test]
    fn day_is_zero_padded() {
        let d = NaiveDate::from_ymd_opt(2026, 9, 1)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        assert_eq!(
            substitute("{{hdate}}", HDATE, "", d),
            "Tuesday, September 01, 2026"
        );
    }

    #[test]
    fn lazy_quirks() {
        assert_eq!(sub("{{ title }}"), "{{ title }}");
        assert_eq!(sub("{{Title}}"), "{{Title}}");
        assert_eq!(sub("{{title}}}"), "T}");
        assert_eq!(sub("{{{{title}}}}"), "{{{{title}}}}");
        assert_eq!(sub("{{title"), "{{title");
        // A % or {{ inside a value is inserted as is.
        assert_eq!(substitute("{{title}}", HDATE, "%1 {{date}}", t()), "%1 {{date}}");
    }
}
