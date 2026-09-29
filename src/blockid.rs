//! Block ids, `^abc123` (§9.1).
//!
//! The grammar is Lua's `%s%^([%w][%w-]+)%f[%W]`: one whitespace byte, `^`, an
//! alphanumeric, then one or more alphanumerics or `-`, ending in an
//! alphanumeric.

use std::collections::HashSet;
use std::fs::File;
use std::io::Read;

use crate::{is_ws, rtrim};

/// With `line[at]` being `^`, the length in bytes of the id that follows.
fn id_len_at(line: &[u8], at: usize) -> Option<usize> {
    let body = &line[at + 1..];
    if !body.first()?.is_ascii_alphanumeric() {
        return None;
    }
    let run = body
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric() || **b == b'-')
        .count();
    // Giving back trailing `-`s lands where the frontier %f[%W] holds.
    let len = run - body[..run].iter().rev().take_while(|&&b| b == b'-').count();
    (len >= 2).then_some(len)
}

/// The ids in a line, in order, as (whitespace before, id start, length).
fn find_ids(line: &str) -> Vec<(usize, usize, usize)> {
    let b = line.as_bytes();
    let mut found = Vec::new();
    let mut i = 0;
    while i + 1 < b.len() {
        if is_ws(b[i])
            && b[i + 1] == b'^'
            && let Some(len) = id_len_at(b, i + 1)
        {
            found.push((i, i + 2, len));
            i += 2 + len;
            continue;
        }
        i += 1;
    }
    found
}

/// The first id in a line.
pub fn read(line: &str) -> Option<&str> {
    find_ids(line)
        .first()
        .map(|&(_, start, len)| &line[start..start + len])
}

/// Removes every id together with the one whitespace byte before it.
pub fn strip(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut from = 0;
    for (ws, start, len) in find_ids(line) {
        out.push_str(&line[from..ws]);
        from = start + len;
    }
    out.push_str(&line[from..]);
    out
}

/// `with_block_id`: removes existing ids, trims trailing whitespace and appends ` ^id`.
pub fn with_id(line: &str, id: &str) -> String {
    format!("{} ^{id}", rtrim(&strip(line)))
}

/// Collects every id found in the lines.
pub fn ids_in<S: AsRef<str>>(lines: &[S]) -> HashSet<String> {
    lines
        .iter()
        .filter_map(|l| read(l.as_ref()).map(str::to_string))
        .collect()
}

/// Mints a new id of `length` characters from `alphabet` (the `block_id`
/// settings). A draw found in `taken` is redrawn, up to 64 times.
pub fn mint(taken: &HashSet<String>, length: usize, alphabet: &str) -> String {
    let chars: Vec<char> = alphabet.chars().collect();
    let chars: &[char] = if chars.is_empty() || chars.len() > 256 {
        &DEFAULT_ALPHABET
    } else {
        &chars
    };
    let mut id = draw(length, chars);
    for _ in 0..64 {
        if !taken.contains(&id) {
            break;
        }
        id = draw(length, chars);
    }
    id
}

/// Used instead of an unusable alphabet, as the plugin does.
const DEFAULT_ALPHABET: [char; 36] = [
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r',
    's', 't', 'u', 'v', 'w', 'x', 'y', 'z', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9',
];

fn draw(length: usize, chars: &[char]) -> String {
    let n = chars.len();
    // Only bytes below a multiple of n are used, so every character is equally likely.
    let limit = 256 - 256 % n;
    let mut out = String::new();
    let mut count = 0;
    let mut buf = [0u8; 32];
    while count < length {
        fill_random(&mut buf);
        for &b in buf.iter().filter(|&&b| (b as usize) < limit) {
            if count == length {
                break;
            }
            out.push(chars[b as usize % n]);
            count += 1;
        }
    }
    out
}

fn fill_random(buf: &mut [u8]) {
    if File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(buf))
        .is_ok()
    {
        return;
    }
    // Fallback where /dev/urandom is missing: fill from the clock and a hash seed.
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut h = RandomState::new().build_hasher();
    for chunk in buf.chunks_mut(8) {
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        );
        let v = h.finish().to_le_bytes();
        chunk.copy_from_slice(&v[..chunk.len()]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading() {
        assert_eq!(read("x ^2"), None);
        assert_eq!(read("a^bcdef"), None);
        assert_eq!(read("^abc"), None);
        assert_eq!(read("x ^abc-"), Some("abc"));
        assert_eq!(read("x ^abc日本"), Some("abc"));
        assert_eq!(read("x ^a-b"), Some("a-b"));
        assert_eq!(read("x ^a--"), None);
        assert_eq!(read("- [x] plan ^t3k9aa done:2026-09-29 14:05"), Some("t3k9aa"));
        assert_eq!(read("a ^x b ^yy"), Some("yy"));
    }

    #[test]
    fn stripping_and_writing() {
        assert_eq!(strip("foo ^abc123 bar ^def456"), "foo bar");
        assert_eq!(strip("foo ^abc-"), "foo-");
        assert_eq!(with_id("- [ ] foo ^old1  ", "new123"), "- [ ] foo ^new123");
        assert_eq!(with_id("- [ ] foo  ", "k2m9qa"), "- [ ] foo ^k2m9qa");
    }

    #[test]
    fn minting() {
        let id = mint(&HashSet::new(), 6, "abcdefghijklmnopqrstuvwxyz0123456789");
        assert_eq!(id.len(), 6);
        assert!(id.chars().all(|c| DEFAULT_ALPHABET.contains(&c)));
        let id = mint(&HashSet::new(), 4, "ab");
        assert_eq!(id.len(), 4);
        assert!(id.chars().all(|c| c == 'a' || c == 'b'));
    }
}
