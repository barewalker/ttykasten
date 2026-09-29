//! Reading a note as lines and writing it back line by line (§0.4).
//!
//! fzfkasten uses `vim.fn.readfile` / `writefile`, so every write normalises
//! the whole file: CRLF becomes LF, a leading BOM is dropped, and a final
//! newline is guaranteed. Every other byte is kept.

use std::fs;
use std::io;
use std::path::Path;

const BOM: &[u8] = b"\xef\xbb\xbf";

/// Splits into lines the way `readfile` does.
pub fn split_lines(bytes: &[u8]) -> Vec<&[u8]> {
    let bytes = bytes.strip_prefix(BOM).unwrap_or(bytes);
    let mut lines = Vec::new();
    let mut rest = bytes;
    while let Some(i) = rest.iter().position(|&b| b == b'\n') {
        let line = &rest[..i];
        lines.push(line.strip_suffix(b"\r").unwrap_or(line));
        rest = &rest[i + 1..];
    }
    // A final line without a newline is still a line; a trailing newline adds no empty line.
    if !rest.is_empty() {
        lines.push(rest);
    }
    lines
}

/// Reads a file as lines. A file that is not UTF-8 could be damaged by writing
/// it back, so it is refused when read.
pub fn read_lines(path: &Path) -> io::Result<Vec<String>> {
    let bytes = fs::read(path)?;
    split_lines(&bytes)
        .into_iter()
        .map(|l| {
            String::from_utf8(l.to_vec()).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{}: not UTF-8", path.display()),
                )
            })
        })
        .collect()
}

/// Writes each line followed by `\n` (`writefile`).
pub fn write_lines<S: AsRef<str>>(path: &Path, lines: &[S]) -> io::Result<()> {
    let mut out = String::new();
    for l in lines {
        out.push_str(l.as_ref());
        out.push('\n');
    }
    fs::write(path, out)
}

/// Writes text made from a template. The result is what Neovim writes on `:w`
/// after splitting the text into buffer lines on `\n` (end of §3). A `\r`
/// stays part of its line.
pub fn write_text(path: &Path, text: &str) -> io::Result<()> {
    let lines: Vec<&str> = text.split('\n').collect();
    write_lines(path, &lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(s: &[u8]) -> Vec<&[u8]> {
        split_lines(s)
    }

    #[test]
    fn readfile_rules() {
        assert_eq!(split(b"a\nb\n"), vec![&b"a"[..], b"b"]);
        assert_eq!(split(b"a\nb"), vec![&b"a"[..], b"b"]);
        assert_eq!(split(b"a\r\nb\r\n"), vec![&b"a"[..], b"b"]);
        assert_eq!(split(b"\xef\xbb\xbfa\n"), vec![&b"a"[..]]);
        assert_eq!(split(b"a\n\n"), vec![&b"a"[..], b""]);
        assert!(split(b"").is_empty());
        // A CR inside a line is kept.
        assert_eq!(split(b"a\rb\n"), vec![&b"a\rb"[..]]);
    }
}
