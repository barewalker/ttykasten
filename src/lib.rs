//! ttykasten's format logic. Reads and writes exactly the bytes fzfkasten.nvim does.
//!
//! The specification is docs/format.md; section numbers (§0.4 and so on) refer
//! to it. Running fzf and the editor is left to main.rs.

pub mod blockid;
pub mod config;
pub mod fileio;
pub mod i18n;
pub mod link;
pub mod note;
pub mod preview;
pub mod task;
pub mod template;

/// ASCII whitespace, Lua's `%s`. The ideographic space U+3000 is not included (§0.2).
pub fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// Strips leading and trailing ASCII whitespace (Lua's `^%s*(.-)%s*$`).
pub fn trim(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_ascii() && is_ws(c as u8))
}

/// Strips trailing ASCII whitespace only.
pub fn rtrim(s: &str) -> &str {
    s.trim_end_matches(|c: char| c.is_ascii() && is_ws(c as u8))
}
