//! Wikilinks and links to lines (§9.2, §9.3).

use std::path::Path;

use crate::{blockid, task, trim};

/// The inside of a `[[…]]`, split into its parts (`split_link`, §9.2).
#[derive(Debug, PartialEq, Eq)]
pub struct Link<'a> {
    /// The note's name; empty for a link into the same note (`[[#top]]`).
    pub name: &'a str,
    pub anchor: Option<&'a str>,
    pub alias: Option<&'a str>,
    pub dir: Option<&'a str>,
}

/// The insides of every `[[…]]` in a line, shortest match, left to right.
pub fn links_in(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(open) = rest.find("[[") {
        let Some(close) = rest[open + 2..].find("]]") else {
            break;
        };
        out.push(&rest[open + 2..open + 2 + close]);
        rest = &rest[open + 2 + close + 2..];
    }
    out
}

/// Splits a link's inside: alias at the first `|`, anchor at the first `#`,
/// directory before the last `/`, and a trailing `.<extension>` dropped.
pub fn split<'a>(inner: &'a str, extension: &str) -> Link<'a> {
    let (rest, alias) = match inner.split_once('|') {
        Some((r, a)) => (r, Some(a)),
        None => (inner, None),
    };
    let (rest, anchor) = match rest.split_once('#') {
        Some((r, a)) => (r, Some(a)),
        None => (rest, None),
    };
    let (dir, name) = match rest.rsplit_once('/') {
        Some((d, n)) => (Some(d), n),
        None => (None, rest),
    };
    let name = name
        .strip_suffix(extension)
        .and_then(|n| n.strip_suffix('.'))
        .unwrap_or(name);
    Link { name, anchor, alias, dir }
}

/// A note's name: its basename without the last extension. A leading dot is
/// not an extension, so `.bashrc` keeps its name.
pub fn note_name(path: &Path) -> String {
    let base = path
        .file_name()
        .map(|b| b.to_string_lossy().into_owned())
        .unwrap_or_default();
    match base.rfind('.') {
        Some(i) if i > 0 => base[..i].to_string(),
        _ => base,
    }
}

/// Whether `line` links to the note `name`, or to its line `^id` when `id` is
/// given. With an id, a same-note link (`[[#^id]]`) counts when the line is
/// in that note (`same_note`).
pub fn links_to(line: &str, extension: &str, name: &str, id: Option<&str>, same_note: bool) -> bool {
    links_in(line).into_iter().any(|inner| {
        let l = split(inner, extension);
        let right_note = l.name == name || (same_note && l.name.is_empty());
        match id {
            None => l.name == name,
            Some(id) => {
                right_note
                    && l.anchor
                        .map(trim)
                        .and_then(|a| a.strip_prefix('^'))
                        .is_some_and(|a| a == id)
            }
        }
    })
}

/// The alias for a link to `line` (`link_alias`, pickers.lua): the text without
/// ids, checkbox or bullet, tags and `[ ] |`, whitespace squeezed, cut to `max`
/// characters with `…`.
pub fn alias(line: &str, max: Option<usize>) -> String {
    let text = blockid::strip(line);
    let text = match task::checkbox(&text) {
        Some((at, _)) => text[at + 3..].to_string(),
        None => strip_bullet(&text).to_string(),
    };
    // Drop tags (`#[A-Za-z0-9_-]+`) and the characters `[`, `]` and `|`.
    let mut out = String::new();
    let mut skip_to = 0;
    for (i, c) in text.char_indices() {
        if i < skip_to {
            continue;
        }
        if c == '#' {
            let len = text[i + 1..]
                .bytes()
                .take_while(|&c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
                .count();
            if len > 0 {
                skip_to = i + 1 + len;
                continue;
            }
        }
        if !matches!(c, '[' | ']' | '|') {
            out.push(c);
        }
    }
    let squeezed = out
        .split(|c: char| c.is_ascii() && crate::is_ws(c as u8))
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    match max {
        Some(max) if max > 0 && squeezed.chars().count() > max => {
            let cut: String = squeezed.chars().take(max).collect();
            format!("{}…", trim(&cut))
        }
        _ => squeezed,
    }
}

/// Drops a leading `-`, `*` or `+` bullet (`^%s*[-*+]%s+`).
fn strip_bullet(text: &str) -> &str {
    let t = text.trim_start_matches(|c: char| c.is_ascii() && crate::is_ws(c as u8));
    match t.as_bytes().first() {
        Some(b'-' | b'*' | b'+') => {
            let rest = &t[1..];
            let after = rest.trim_start_matches(|c: char| c.is_ascii() && crate::is_ws(c as u8));
            if after.len() < rest.len() { after } else { text }
        }
        _ => text,
    }
}

/// The result of making a link to one line of a note.
#[derive(Debug, PartialEq, Eq)]
pub struct BlockLink {
    pub link: String,
    /// The line rewritten with a new id, when it had none.
    pub new_line: Option<String>,
}

/// YankLink's steps (§9.3) for `lines[idx]` of the note `name`: reuse the
/// line's id, or mint one that avoids the note's ids. None for a blank line.
pub fn block_link(
    lines: &[String],
    idx: usize,
    name: &str,
    id_length: usize,
    id_alphabet: &str,
    alias_max: Option<Option<usize>>,
) -> Option<BlockLink> {
    let line = lines.get(idx)?;
    if trim(line).is_empty() {
        return None;
    }
    let (id, new_line) = match blockid::read(line) {
        Some(id) => (id.to_string(), None),
        None => {
            let id = blockid::mint(&blockid::ids_in(lines), id_length, id_alphabet);
            let new_line = blockid::with_id(line, &id);
            (id, Some(new_line))
        }
    };
    let mut target = format!("{name}#^{id}");
    if let Some(max) = alias_max {
        let a = alias(line, max);
        if !a.is_empty() {
            target.push('|');
            target.push_str(&a);
        }
    }
    Some(BlockLink {
        link: format!("[[{target}]]"),
        new_line,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALPHA: &str = "abcdefghijklmnopqrstuvwxyz0123456789";

    #[test]
    fn finding_links() {
        assert_eq!(links_in("a [[x]] b [[y|z]]"), vec!["x", "y|z"]);
        assert_eq!(links_in("[[a]]b]]"), vec!["a"]);
        assert_eq!(links_in("[[[a]]]"), vec!["[a"]);
        assert!(links_in("[[open").is_empty());
    }

    #[test]
    fn splitting() {
        let l = split("sub/deep/note.md#見出し|呼び名", "md");
        assert_eq!(
            l,
            Link { name: "note", anchor: Some("見出し"), alias: Some("呼び名"), dir: Some("sub/deep") }
        );
        assert_eq!(split("#top", "md").name, "");
        assert_eq!(split("note#Q#A", "md").anchor, Some("Q#A"));
        assert_eq!(split("note.v2", "md").name, "note.v2");
        assert_eq!(split("note.v2.md", "md").name, "note.v2");
        assert_eq!(split("a|b#c", "md").alias, Some("b#c"));
    }

    #[test]
    fn names() {
        assert_eq!(note_name(Path::new("/x/notes/foo.md")), "foo");
        assert_eq!(note_name(Path::new("a.b.md")), "a.b");
        assert_eq!(note_name(Path::new(".bashrc")), ".bashrc");
    }

    #[test]
    fn linking_to() {
        assert!(links_to("see [[meeting]]", "md", "meeting", None, false));
        assert!(links_to("see [[dir/meeting.md#h|x]]", "md", "meeting", None, false));
        assert!(!links_to("see [[meetings]]", "md", "meeting", None, false));
        assert!(links_to("- [[meeting#^a1b2c3]]", "md", "meeting", Some("a1b2c3"), false));
        assert!(links_to("- [[meeting# ^a1b2c3 |x]]", "md", "meeting", Some("a1b2c3"), false));
        assert!(!links_to("- [[meeting#^zzz999]]", "md", "meeting", Some("a1b2c3"), false));
        assert!(!links_to("- [[meeting]]", "md", "meeting", Some("a1b2c3"), false));
        assert!(links_to("- [[#^a1b2c3]]", "md", "meeting", Some("a1b2c3"), true));
        assert!(!links_to("- [[#^a1b2c3]]", "md", "meeting", Some("a1b2c3"), false));
    }

    #[test]
    fn aliases() {
        assert_eq!(
            alias("- [ ] (A) draft the proposal #todo #budget ^a1b2c3", None),
            "(A) draft the proposal"
        );
        assert_eq!(alias("  * a [link] | b", None), "a link b");
        assert_eq!(alias("日本語の長い行を途中で切る", Some(5)), "日本語の長…");
        assert_eq!(alias("abc   ", Some(3)), "abc");
    }

    fn lines(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn block_links() {
        let l = lines(&["# T", "- [ ] draft ^t3k9aa", "", "plain line  "]);
        assert_eq!(
            block_link(&l, 1, "meeting", 6, ALPHA, None),
            Some(BlockLink { link: "[[meeting#^t3k9aa]]".into(), new_line: None })
        );
        assert_eq!(block_link(&l, 2, "meeting", 6, ALPHA, None), None);
        let made = block_link(&l, 3, "meeting", 6, ALPHA, None).unwrap();
        let new_line = made.new_line.unwrap();
        let id = blockid::read(&new_line).unwrap();
        assert_eq!(new_line, format!("plain line ^{id}"));
        assert_eq!(made.link, format!("[[meeting#^{id}]]"));
        assert_ne!(id, "t3k9aa");
        let with_alias = block_link(&l, 1, "meeting", 6, ALPHA, Some(Some(40))).unwrap();
        assert_eq!(with_alias.link, "[[meeting#^t3k9aa|draft]]");
    }
}
