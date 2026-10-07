//! The undo journal: the last writes made by task actions, kept on disk so
//! that undo works across runs (format.md Q7).
//!
//! The plugin keeps this history in memory inside one Neovim process. A
//! terminal command lives for one action, so ttykasten keeps it in a file
//! outside the collection, under $XDG_STATE_HOME (or ~/.local/state). Like
//! the plugin, undo refuses when the line is no longer what was written.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::fileio;

/// How many writes are remembered, over all collections.
const MAX: usize = 50;

/// One write: line `lineno` (1-based) of `rel` went from `before` to `after`,
/// or `after` was added there when `before` is None.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The collection's home, so one journal can serve several.
    pub home: String,
    pub rel: String,
    pub lineno: usize,
    pub before: Option<String>,
    pub after: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Undo {
    Nothing,
    /// The line (or the note) has changed since; the entry is dropped.
    Changed(Entry),
    Undone(Entry),
}

pub struct Journal {
    path: PathBuf,
}

impl Journal {
    pub fn new(path: PathBuf) -> Self {
        Journal { path }
    }

    /// `$XDG_STATE_HOME/ttykasten/undo.jsonl`, else under ~/.local/state.
    pub fn default_path() -> Option<PathBuf> {
        let base = env::var_os("XDG_STATE_HOME")
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
        Some(base.join("ttykasten").join("undo.jsonl"))
    }

    fn load(&self) -> Vec<Entry> {
        let Ok(text) = fs::read_to_string(&self.path) else {
            return Vec::new();
        };
        // A damaged line is skipped rather than costing the whole journal.
        text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
    }

    fn save(&self, entries: &[Entry]) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut out = String::new();
        for e in entries {
            out.push_str(&serde_json::to_string(e).map_err(io::Error::other)?);
            out.push('\n');
        }
        fs::write(&self.path, out)
    }

    pub fn record(&self, entry: Entry) -> io::Result<()> {
        let mut entries = self.load();
        entries.push(entry);
        let skip = entries.len().saturating_sub(MAX);
        self.save(&entries[skip..])
    }

    /// Takes back the newest write made in `home`.
    pub fn undo(&self, home: &Path) -> io::Result<Undo> {
        let home_s = home.to_string_lossy();
        let mut entries = self.load();
        let Some(i) = entries.iter().rposition(|e| e.home == home_s) else {
            return Ok(Undo::Nothing);
        };
        let entry = entries.remove(i);
        let path = home.join(&entry.rel);
        let mut lines = match fileio::read_lines(&path) {
            Ok(l) => l,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                self.save(&entries)?;
                return Ok(Undo::Changed(entry));
            }
            Err(e) => return Err(e),
        };
        let idx = entry.lineno.wrapping_sub(1);
        if lines.get(idx) != Some(&entry.after) {
            self.save(&entries)?;
            return Ok(Undo::Changed(entry));
        }
        match &entry.before {
            Some(before) => lines[idx] = before.clone(),
            None => {
                lines.remove(idx);
            }
        }
        fileio::write_lines(&path, &lines)?;
        self.save(&entries)?;
        Ok(Undo::Undone(entry))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_undoes() {
        let dir = env::temp_dir().join(format!("ttykasten-journal-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let home = dir.join("home");
        fs::create_dir_all(&home).unwrap();
        let note = home.join("n.md");
        let j = Journal::new(dir.join("state/undo.jsonl"));
        let entry = |lineno, before: Option<&str>, after: &str| Entry {
            home: home.to_string_lossy().into(),
            rel: "n.md".into(),
            lineno,
            before: before.map(String::from),
            after: after.into(),
        };

        fs::write(&note, "- [x] a\n- [ ] b\nadded\n").unwrap();
        j.record(entry(1, Some("- [ ] a"), "- [x] a")).unwrap();
        j.record(entry(3, None, "added")).unwrap();
        // Another collection's entry is not touched.
        j.record(Entry { home: "/elsewhere".into(), ..entry(1, Some("x"), "y") }).unwrap();

        assert!(matches!(j.undo(&home).unwrap(), Undo::Undone(_)));
        assert_eq!(fs::read_to_string(&note).unwrap(), "- [x] a\n- [ ] b\n");
        // The line was edited since: refused, and the entry dropped.
        fs::write(&note, "- [x] a edited\n- [ ] b\n").unwrap();
        assert!(matches!(j.undo(&home).unwrap(), Undo::Changed(_)));
        assert_eq!(j.undo(&home).unwrap(), Undo::Nothing);
        assert_eq!(j.load().len(), 1);

        for i in 0..60 {
            j.record(entry(1, Some("a"), &i.to_string())).unwrap();
        }
        assert_eq!(j.load().len(), MAX);
        fs::remove_dir_all(&dir).unwrap();
    }
}
