//! Romaji search through `ttyskk migemo`: `kaigi` becomes a regex matching
//! 会議, かいぎ, カイギ and so on. The `rg` flavour is the same regex dialect
//! as Rust's regex crate, so its output is used directly.

use std::time::Duration;

use crate::config::Romaji;

/// The command for one query.
fn argv(cfg: &Romaji, query: &str) -> Vec<String> {
    let mut v = vec![cfg.command.clone(), "migemo".into(), "--flavour".into(), "rg".into()];
    if let Some(limit) = cfg.limit {
        v.push("--limit".into());
        v.push(limit.to_string());
    }
    v.push(query.into());
    v
}

/// The regex for a romaji query, or None when the backend can't answer.
pub fn regex(cfg: &Romaji, query: &str) -> Option<String> {
    if !cfg.enabled || query.is_empty() {
        return None;
    }
    let out = crate::command::run(&argv(cfg, query), Duration::from_millis(cfg.timeout)).ok()?;
    let re = out.trim();
    (!re.is_empty()).then(|| re.to_string())
}

/// Whether the backend answers at all.
pub fn available(cfg: &Romaji) -> bool {
    regex(cfg, "a").is_some()
}
