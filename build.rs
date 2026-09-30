//! Records the git commit the binary is built from, for `ttykasten --version`.
//! Builds without git (e.g. from a crates.io package) simply have no commit.

use std::path::Path;
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn main() {
    // Rebuild when the checked-out commit changes.
    println!("cargo:rerun-if-changed=.git/HEAD");
    if let Some(head) = std::fs::read_to_string(".git/HEAD")
        .ok()
        .and_then(|h| h.strip_prefix("ref: ").map(|r| r.trim().to_string()))
    {
        let r = Path::new(".git").join(head);
        println!("cargo:rerun-if-changed={}", r.display());
    }
    println!("cargo:rerun-if-changed=.git/index");

    let commit = match git(&["rev-parse", "--short=7", "HEAD"]) {
        // Only tracked files count: cargo leaves an untracked .cargo-ok in its checkouts.
        Some(c) => match git(&["status", "--porcelain", "--untracked-files=no"]) {
            Some(s) if !s.is_empty() => format!("{c}, modified"),
            _ => c,
        },
        None => String::new(),
    };
    println!("cargo:rustc-env=TTYKASTEN_COMMIT={commit}");
}
