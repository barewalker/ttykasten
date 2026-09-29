//! Task lines (§8). Only capture (TaskAdd) so far.

use std::fs;
use std::io;
use std::path::Path;

use crate::config::Config;
use crate::{blockid, fileio, trim};

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

/// Captures a task: appends it as the file's last line and returns that line.
/// A missing file is created, directory included, holding just the line.
pub fn capture(cfg: &Config, path: &Path, text: &str, due: Option<&str>) -> io::Result<Option<String>> {
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
    Ok(Some(line))
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let first = capture(&cfg, &path, "one", None).unwrap().unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), format!("{first}\n"));

        // CRLF, a BOM, a missing final newline and trailing blank lines are normalised per §0.4.
        fs::write(&path, "\u{feff}# T\r\n- a\r\n\r\nlast").unwrap();
        let second = capture(&cfg, &path, "two", None).unwrap().unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            format!("# T\n- a\n\nlast\n{second}\n")
        );
        assert!(capture(&cfg, &path, "  ", None).unwrap().is_none());
        fs::remove_dir_all(&dir).unwrap();
    }
}
