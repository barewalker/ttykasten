//! Telling whether a note is open in Neovim with unsaved changes.
//!
//! The plugin refuses to write a file whose buffer has unsaved changes in
//! its own Neovim. ttykasten runs outside Neovim, so it reads the swap file
//! instead: Neovim keeps one per edited file, named after the file's full
//! path with every `/` turned into `%` (the default `directory` ends in `//`),
//! and its first block records the editing process and whether the buffer
//! has unsaved changes. The layout is Vim's `struct block0` (memline.c):
//!
//! - bytes 0..2: `b0`
//! - bytes 24..28: the process id, least significant byte first
//! - byte 1007 (the last byte of the 900-byte file name field): `U` when
//!   the buffer is modified. Neovim sets it on the first change and clears
//!   it on `:w`.

use std::fs;
use std::io::Read;
use std::path::Path;

const DIRTY_AT: usize = 1007;
const DIRTY: u8 = b'U';

/// The id of a running Neovim that has `file` open with unsaved changes,
/// according to the swap files in `swap_dir`. None when there is no such
/// swap file, its process is gone (a stale swap), or the buffer is saved.
pub fn unsaved_pid(swap_dir: &Path, file: &Path) -> Option<u32> {
    let mut names = vec![encode(file)];
    // The note may have been opened by another spelling of its path.
    if let Ok(real) = fs::canonicalize(file) {
        let real = encode(&real);
        if !names.contains(&real) {
            names.push(real);
        }
    }
    let entries = fs::read_dir(swap_dir).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // `<encoded>.swp`, or `.swo`, `.swn` ... when several are open.
        let is_swap = names
            .iter()
            .any(|n| name.len() == n.len() + 4 && name.starts_with(n.as_str()) && name[n.len()..].starts_with(".s"));
        if !is_swap {
            continue;
        }
        if let Some(pid) = dirty_pid(&entry.path()) {
            return Some(pid);
        }
    }
    None
}

fn encode(path: &Path) -> String {
    path.to_string_lossy().replace('/', "%")
}

/// The swap file's process id when it says the buffer is modified and that
/// process is still running.
fn dirty_pid(swap: &Path) -> Option<u32> {
    let mut block = [0u8; DIRTY_AT + 1];
    fs::File::open(swap).ok()?.read_exact(&mut block).ok()?;
    if &block[..2] != b"b0" || block[DIRTY_AT] != DIRTY {
        return None;
    }
    let pid = u32::from_le_bytes([block[24], block[25], block[26], block[27]]);
    alive(pid).then_some(pid)
}

fn alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    if Path::new("/proc").is_dir() {
        return Path::new(&format!("/proc/{pid}")).exists();
    }
    // No /proc (macOS): ask kill(1) whether the process exists.
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn swap(dir: &Path, file: &Path, ext: &str, pid: u32, dirty: bool) {
        let mut block = vec![0u8; 4096];
        block[..2].copy_from_slice(b"b0");
        block[24..28].copy_from_slice(&pid.to_le_bytes());
        if dirty {
            block[DIRTY_AT] = DIRTY;
        }
        fs::write(dir.join(format!("{}.{ext}", encode(file))), block).unwrap();
    }

    #[test]
    fn reads_swap_files() {
        let dir = std::env::temp_dir().join(format!("ttykasten-swap-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let note = Path::new("/notes/a b/plan.md");
        let me = std::process::id();

        assert_eq!(unsaved_pid(&dir, note), None);
        swap(&dir, note, "swp", me, false);
        assert_eq!(unsaved_pid(&dir, note), None, "saved buffer");
        swap(&dir, note, "swo", me, true);
        assert_eq!(unsaved_pid(&dir, note), Some(me), "second swap, modified");
        fs::remove_file(dir.join(format!("{}.swo", encode(note)))).unwrap();
        // A stale swap (process gone) does not count.
        swap(&dir, note, "swp", u32::MAX - 1, true);
        assert_eq!(unsaved_pid(&dir, note), None);
        // Another note's swap does not count either.
        swap(&dir, Path::new("/notes/a b/plan.md.bak"), "swp", me, true);
        assert_eq!(unsaved_pid(&dir, note), None);
        assert_eq!(unsaved_pid(&dir.join("missing"), note), None);
        fs::remove_dir_all(&dir).unwrap();
    }
}
