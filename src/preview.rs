//! The preview shown next to fzf: a note, coloured by its format.
//!
//! Colours come from `[preview.colors]` (SGR parameters). The defaults are
//! 16-colour numbers, so the terminal's palette decides the actual shades.
//! What counts as what follows format.md: headings are `^#+\s` (§8.3), tags
//! `#[A-Za-z0-9_-]+` (§10), links `[[…]]` (§9.2), ids §9.1.

use crate::{blockid, task};
use crate::config::Colors;

const RESET: &str = "\x1b[0m";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Style {
    Dim,
    Heading,
    Tag,
    Link,
    Open,
    Done,
    Priority,
    Due,
    Cancelled,
    Mark,
}
use Style::*;

/// The colour settings as terminal escape sequences.
struct Palette {
    seqs: [String; 10],
}

impl Palette {
    fn new(c: &Colors) -> Self {
        let sgr = |p: &str| if p.is_empty() { String::new() } else { format!("\x1b[{p}m") };
        Palette {
            seqs: [
                sgr(&c.dim),
                sgr(&c.heading),
                sgr(&c.tag),
                sgr(&c.link),
                sgr(&c.open),
                sgr(&c.done),
                sgr(&c.priority),
                sgr(&c.due),
                sgr(&c.cancelled),
                sgr(&c.mark),
            ],
        }
    }

    fn get(&self, s: Style) -> &str {
        &self.seqs[s as usize]
    }
}

/// Renders a file's lines as coloured text. The line with index `mark`
/// (0-based), if any, is shown in the mark colour instead.
pub fn render<S: AsRef<str>>(lines: &[S], colors: &Colors, mark: Option<usize>) -> String {
    let pal = Palette::new(colors);
    let mut out = String::new();
    let fm_end = frontmatter_end(lines);
    let mut fence: Option<(u8, usize)> = None;
    for (i, line) in lines.iter().enumerate() {
        let line = line.as_ref();
        let start = out.len();
        if fm_end.is_some_and(|end| i <= end) {
            paint(&mut out, pal.get(Dim), line);
        } else if let Some((ch, len)) = fence {
            if fence_delim(line).is_some_and(|(c, l)| c == ch && l >= len) {
                fence = None;
            }
            paint(&mut out, pal.get(Dim), line);
        } else if let Some(open) = fence_delim(line) {
            fence = Some(open);
            paint(&mut out, pal.get(Dim), line);
        } else if is_heading(line) {
            paint(&mut out, pal.get(Heading), line);
        } else {
            inline(&mut out, &pal, line);
        }
        if mark == Some(i) {
            out.truncate(start);
            paint(&mut out, pal.get(Mark), line);
        }
        out.push('\n');
    }
    out
}

fn paint(out: &mut String, style: &str, text: &str) {
    if style.is_empty() {
        out.push_str(text);
        return;
    }
    out.push_str(style);
    out.push_str(text);
    out.push_str(RESET);
}

/// If line 1 is `---` and a later line is `---`, that line's index (0-based).
fn frontmatter_end<S: AsRef<str>>(lines: &[S]) -> Option<usize> {
    if lines.first()?.as_ref() != "---" {
        return None;
    }
    lines
        .iter()
        .skip(1)
        .position(|l| l.as_ref() == "---")
        .map(|p| p + 1)
}

/// For a line starting with ```` ``` ```` or `~~~`, the character and the run length.
fn fence_delim(line: &str) -> Option<(u8, usize)> {
    let t = line.trim_start_matches([' ', '\t']).as_bytes();
    let ch = *t.first().filter(|&&c| c == b'`' || c == b'~')?;
    let len = t.iter().take_while(|&&c| c == ch).count();
    (len >= 3).then_some((ch, len))
}

fn is_heading(line: &str) -> bool {
    let rest = line.trim_start_matches('#');
    rest.len() < line.len() && rest.bytes().next().is_some_and(crate::is_ws)
}

/// The coloured spans of a line as (start, end, style). On overlap the one found first wins.
fn spans(line: &str) -> Vec<(usize, usize, Style)> {
    let b = line.as_bytes();
    let mut spans = Vec::new();

    // Links, shortest match, left to right (§9.2).
    let mut from = 0;
    while let Some(open) = line[from..].find("[[").map(|p| p + from) {
        let Some(close) = line[open + 2..].find("]]").map(|p| p + open + 2) else {
            break;
        };
        spans.push((open, close + 2, Link));
        from = close + 2;
    }

    // The checkbox mark, and a priority `(A)` after it.
    if let Some((at, mark)) = task::checkbox(line) {
        let style = match mark {
            b' ' => Open,
            b'-' => Dim,
            _ => Done,
        };
        spans.push((at, at + 3, style));
        let rest = &b[at + 3..];
        let ws = rest.iter().take_while(|&&c| crate::is_ws(c)).count();
        let p = at + 3 + ws;
        if ws > 0
            && b.get(p) == Some(&b'(')
            && b.get(p + 1).is_some_and(u8::is_ascii_uppercase)
            && b.get(p + 2) == Some(&b')')
        {
            spans.push((p, p + 3, Priority));
        }
    }

    // Due dates, and the done and cancelled stamps.
    for (key, style) in [("due:", Due), ("done:", Dim), ("cancelled:", Dim)] {
        for (i, _) in line.match_indices(key) {
            let end = i + key.len();
            let len = b[end..]
                .iter()
                .take_while(|&&c| c.is_ascii_digit() || matches!(c, b'-' | b':' | b'T'))
                .count();
            // A done: stamp has a space before its time (`done:2026-09-29 14:05`).
            let mut stop = end + len;
            if key == "done:"
                && b.get(stop) == Some(&b' ')
                && b.get(stop + 1..stop + 6).is_some_and(|t| {
                    t.len() == 5 && t[2] == b':' && t.iter().enumerate().all(|(k, c)| k == 2 || c.is_ascii_digit())
                })
            {
                stop += 6;
            }
            if len >= 10 {
                spans.push((i, stop, style));
            }
        }
    }

    // Tags. No left boundary (§10).
    for (i, _) in line.match_indices('#') {
        let len = b[i + 1..]
            .iter()
            .take_while(|&&c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
            .count();
        if len > 0 {
            spans.push((i, i + 1 + len, Tag));
        }
    }

    // Block ids.
    if let Some(id) = blockid::read(line) {
        for (i, _) in line.match_indices(&format!("^{id}")) {
            spans.push((i, i + 1 + id.len(), Dim));
        }
    }

    // Keep earlier spans and drop any that overlap them.
    let mut kept: Vec<(usize, usize, Style)> = Vec::new();
    for s in spans {
        if kept.iter().all(|k| s.1 <= k.0 || k.1 <= s.0) {
            kept.push(s);
        }
    }
    kept.sort_by_key(|s| s.0);
    kept
}

fn inline(out: &mut String, pal: &Palette, line: &str) {
    // A cancelled task is struck through as a whole.
    let base = match task::checkbox(line) {
        Some((_, b'-')) => pal.get(Cancelled),
        _ => "",
    };
    out.push_str(base);
    let mut at = 0;
    for (start, end, style) in spans(line) {
        let seq = pal.get(style);
        if seq.is_empty() {
            continue;
        }
        out.push_str(&line[at..start]);
        out.push_str(seq);
        out.push_str(&line[start..end]);
        out.push_str(RESET);
        out.push_str(base);
        at = end;
    }
    out.push_str(&line[at..]);
    if !base.is_empty() {
        out.push_str(RESET);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(s: &str) -> String {
        let mut out = String::new();
        let mut in_esc = false;
        for c in s.chars() {
            match (in_esc, c) {
                (false, '\x1b') => in_esc = true,
                (true, 'm') => in_esc = false,
                (true, _) => {}
                (false, c) => out.push(c),
            }
        }
        out
    }

    #[test]
    fn text_is_kept() {
        let lines = [
            "---",
            "title: x",
            "---",
            "# Log",
            "- [ ] (A) #projectx check [[note#^abc123|a]] #todo due:2026-10-02 ^t3k9aa",
            "- [x] foo done:2026-09-29 14:05 ^t3k9aa",
            "- [-] ~~bar~~ cancelled:2026-09-16",
            "```",
            "#todo in code",
            "```",
            "日本語 #会議 a#b",
        ];
        let out = render(&lines, &Colors::default(), None);
        assert_eq!(plain(&out), lines.join("\n") + "\n");
        // Empty colours produce no escape sequences.
        let none = Colors { heading: String::new(), tag: String::new(), link: String::new(), open: String::new(), done: String::new(), priority: String::new(), due: String::new(), dim: String::new(), cancelled: String::new(), mark: String::new() };
        let out = render(&lines, &none, None);
        assert!(!out.contains('\x1b'));
        assert_eq!(plain(&out), lines.join("\n") + "\n");
    }

    #[test]
    fn coloring() {
        let s = spans("- [ ] (A) #projectx x [[n#h]] due:2026-10-02 ^abc123");
        let styled: Vec<(&str, Style)> = s
            .iter()
            .map(|&(a, b, st)| (&"- [ ] (A) #projectx x [[n#h]] due:2026-10-02 ^abc123"[a..b], st))
            .collect();
        assert_eq!(
            styled,
            vec![
                ("[ ]", Open),
                ("(A)", Priority),
                ("#projectx", Tag),
                ("[[n#h]]", Link),
                ("due:2026-10-02", Due),
                ("^abc123", Dim),
            ]
        );
        // Headings versus tags.
        assert!(is_heading("# Log"));
        assert!(!is_heading("#todo foo"));
        assert!(spans("#会議").is_empty());
        // An indented fence is still a fence.
        assert_eq!(fence_delim("  ````"), Some((b'`', 4)));
    }
}
