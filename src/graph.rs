//! The collection as a graph of links: a note's link tree, orphans, dead
//! links and hubs (the plugin's graph.lua).
//!
//! Unlike backlinks, the graph skips fenced blocks and inline code spans,
//! leaves out ignored dirs, and drops links from a note to itself.

use std::collections::{BTreeMap, HashMap, VecDeque};

use crate::link;

#[derive(Debug, Clone)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub rel: String,
    pub lineno: usize,
    /// The line the link is written on, trimmed.
    pub line: String,
}

#[derive(Debug, Default)]
pub struct Graph {
    /// Note names, sorted.
    pub names: Vec<String>,
    /// Each name's files, in the order found.
    pub paths: HashMap<String, Vec<String>>,
    out: HashMap<String, Vec<usize>>,
    back: HashMap<String, Vec<usize>>,
    pub edges: Vec<Edge>,
}

/// A fence line (```` ``` ```` or `~~~` after up to three spaces); fences
/// simply toggle here.
fn is_fence(line: &str) -> bool {
    let t = line.strip_prefix("   ").or_else(|| line.strip_prefix("  ")).or_else(|| line.strip_prefix(' ')).unwrap_or(line);
    let t = if line.starts_with("    ") { line } else { t };
    t.starts_with("```") || t.starts_with("~~~")
}

/// The line without inline code spans (`` `+[^`]*`+ ``).
fn without_code_spans(line: &str) -> String {
    let b = line.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    let mut from = 0;
    while i < b.len() {
        if b[i] == b'`' {
            let start = i;
            while i < b.len() && b[i] == b'`' {
                i += 1;
            }
            let body = i;
            while i < b.len() && b[i] != b'`' {
                i += 1;
            }
            if i < b.len() {
                while i < b.len() && b[i] == b'`' {
                    i += 1;
                }
                out.push_str(&line[from..start]);
                from = i;
                continue;
            }
            i = body;
            continue;
        }
        i += 1;
    }
    out.push_str(&line[from..]);
    out
}

/// Builds the graph from notes given as (path relative to home, lines),
/// which should be sorted by path.
pub fn build(notes: &[(String, Vec<String>)], extension: &str, ignore_dirs: &[String]) -> Graph {
    let mut g = Graph::default();
    for (rel, lines) in notes {
        if ignore_dirs.iter().any(|d| rel == d || rel.strip_prefix(d.as_str()).is_some_and(|r| r.starts_with('/'))) {
            continue;
        }
        let name = link::note_name(std::path::Path::new(rel));
        if !g.paths.contains_key(&name) {
            g.names.push(name.clone());
            g.out.insert(name.clone(), Vec::new());
            g.back.insert(name.clone(), Vec::new());
        }
        g.paths.entry(name.clone()).or_default().push(rel.clone());
        let mut linked: Vec<String> = Vec::new();
        let mut in_code = false;
        for (i, line) in lines.iter().enumerate() {
            if is_fence(line) {
                in_code = !in_code;
                continue;
            }
            if in_code {
                continue;
            }
            let plain = without_code_spans(line);
            for inner in link::links_in(&plain) {
                let target = link::split(inner, extension).name.to_string();
                if !target.is_empty() && target != name && !linked.contains(&target) {
                    linked.push(target.clone());
                    g.edges.push(Edge { from: name.clone(), to: target, rel: rel.clone(), lineno: i + 1, line: crate::trim(line).to_string() });
                }
            }
        }
    }
    for (i, e) in g.edges.iter().enumerate() {
        g.out.entry(e.from.clone()).or_default().push(i);
        g.back.entry(e.to.clone()).or_default().push(i);
    }
    g.names.sort();
    g
}

impl Graph {
    pub fn has_note(&self, name: &str) -> bool {
        self.paths.contains_key(name)
    }

    fn out_of(&self, name: &str) -> &[usize] {
        self.out.get(name).map_or(&[], Vec::as_slice)
    }

    fn back_of(&self, name: &str) -> &[usize] {
        self.back.get(name).map_or(&[], Vec::as_slice)
    }

    pub fn has_backlinks(&self, name: &str) -> bool {
        !self.back_of(name).is_empty()
    }
}

/// One row of a link tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub rel: String,
    pub lineno: usize,
    pub missing: bool,
    pub display: String,
}

struct Kid {
    name: String,
    arrow: &'static str,
    rel: String,
    lineno: usize,
    missing: bool,
}

/// A note's neighbours: links out (→), links in (←), both (↔).
fn neighbours(g: &Graph, name: &str) -> Vec<Kid> {
    let mut kids: Vec<Kid> = Vec::new();
    let mut out_at: HashMap<&str, usize> = HashMap::new();
    for &i in g.out_of(name) {
        let e = &g.edges[i];
        let paths = g.paths.get(&e.to);
        out_at.insert(&e.to, kids.len());
        kids.push(Kid {
            name: e.to.clone(),
            arrow: "→",
            rel: paths.map_or_else(|| e.rel.clone(), |p| p[0].clone()),
            lineno: if paths.is_some() { 1 } else { e.lineno },
            missing: paths.is_none(),
        });
    }
    for &i in g.back_of(name) {
        let e = &g.edges[i];
        match out_at.get(e.from.as_str()) {
            Some(&k) => kids[k].arrow = "↔",
            None => kids.push(Kid { name: e.from.clone(), arrow: "←", rel: e.rel.clone(), lineno: e.lineno, missing: false }),
        }
    }
    kids
}

/// The notes within `depth` links of `root`, breadth first, each once.
pub fn tree(g: &Graph, root: &str, depth: usize) -> Vec<Row> {
    let mut kids_of: BTreeMap<String, Vec<Kid>> = BTreeMap::new();
    let mut dist: HashMap<String, usize> = HashMap::new();
    kids_of.insert(root.to_string(), Vec::new());
    dist.insert(root.to_string(), 0);
    let mut queue = VecDeque::from([root.to_string()]);
    while let Some(name) = queue.pop_front() {
        if dist[&name] >= depth {
            continue;
        }
        for kid in neighbours(g, &name) {
            if dist.contains_key(&kid.name) {
                continue;
            }
            dist.insert(kid.name.clone(), dist[&name] + 1);
            kids_of.insert(kid.name.clone(), Vec::new());
            queue.push_back(kid.name.clone());
            kids_of.get_mut(&name).unwrap().push(kid);
        }
    }
    let mut rows = Vec::new();
    fn render(kids_of: &BTreeMap<String, Vec<Kid>>, name: &str, prefix: &str, rows: &mut Vec<Row>) {
        let kids = &kids_of[name];
        for (i, kid) in kids.iter().enumerate() {
            let last = i + 1 == kids.len();
            rows.push(Row {
                name: kid.name.clone(),
                rel: kid.rel.clone(),
                lineno: kid.lineno,
                missing: kid.missing,
                display: format!(
                    "{prefix}{}{} {}{}",
                    if last { "└─ " } else { "├─ " },
                    kid.arrow,
                    kid.name,
                    if kid.missing { "  (no note)" } else { "" }
                ),
            });
            render(kids_of, &kid.name, &format!("{prefix}{}", if last { "   " } else { "│  " }), rows);
        }
    }
    render(&kids_of, root, "", &mut rows);
    rows
}

/// Notes with no link in and none out, by path.
pub fn orphans(g: &Graph) -> Vec<String> {
    let mut rels: Vec<String> = g
        .names
        .iter()
        .filter(|n| g.out_of(n).is_empty() && g.back_of(n).is_empty())
        .flat_map(|n| g.paths[n].clone())
        .collect();
    rels.sort();
    rels
}

/// Links no note answers to, by target, path and line.
pub fn dead_links(g: &Graph) -> Vec<&Edge> {
    let mut dead: Vec<&Edge> = g.edges.iter().filter(|e| !g.has_note(&e.to)).collect();
    dead.sort_by(|a, b| a.to.cmp(&b.to).then(a.rel.cmp(&b.rel)).then(a.lineno.cmp(&b.lineno)));
    dead
}

/// Connected notes by how many links meet there: (name, first path, in, out).
pub fn hubs(g: &Graph) -> Vec<(String, String, usize, usize)> {
    let mut hubs: Vec<(String, String, usize, usize)> = g
        .names
        .iter()
        .map(|n| (n.clone(), g.paths[n][0].clone(), g.back_of(n).len(), g.out_of(n).len()))
        .filter(|(_, _, b, o)| b + o > 0)
        .collect();
    hubs.sort_by(|a, b| (b.2 + b.3).cmp(&(a.2 + a.3)).then(b.2.cmp(&a.2)).then(a.0.cmp(&b.0)));
    hubs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notes() -> Vec<(String, Vec<String>)> {
        let n = |rel: &str, text: &str| (rel.to_string(), text.lines().map(str::to_string).collect());
        vec![
            n("alpha.md", "see [[beta]] and [[gamma]] and [[alpha]]\n`[[in code]]`\n```\n[[fenced]]\n```\n[[beta]] again"),
            n("beta.md", "back to [[alpha|A]]\n[[missing note]]"),
            n("gamma.md", "nothing"),
            n("lonely.md", "no links"),
            n("templates/t.md", "[[alpha]]"),
        ]
    }

    #[test]
    fn building() {
        let g = build(&notes(), "md", &["templates".into()]);
        assert_eq!(g.names, vec!["alpha", "beta", "gamma", "lonely"]);
        let e: Vec<(&str, &str, usize)> = g.edges.iter().map(|e| (e.from.as_str(), e.to.as_str(), e.lineno)).collect();
        assert_eq!(e, vec![("alpha", "beta", 1), ("alpha", "gamma", 1), ("beta", "alpha", 1), ("beta", "missing note", 2)]);
        assert_eq!(orphans(&g), vec!["lonely.md"]);
        assert_eq!(dead_links(&g).len(), 1);
        let h = hubs(&g);
        assert_eq!(h[0], ("alpha".into(), "alpha.md".into(), 1, 2));
        assert_eq!(h[1], ("beta".into(), "beta.md".into(), 1, 2));
    }

    #[test]
    fn trees() {
        let g = build(&notes(), "md", &["templates".into()]);
        let rows: Vec<String> = tree(&g, "alpha", 2).into_iter().map(|r| r.display).collect();
        assert_eq!(rows, vec!["├─ ↔ beta", "│  └─ → missing note  (no note)", "└─ → gamma"]);
        assert_eq!(without_code_spans("a `x` b ``y`` c `open"), "a  b  c `open");
    }
}
