//! Running an external command with a time limit.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Runs a command with a timeout, returning stdout or the reason it failed.
pub fn run(argv: &[String], timeout: Duration) -> Result<String, String> {
    let mut child = Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => format!("{} is not on PATH", argv[0]),
            _ => format!("{}: {e}", argv[0]),
        })?;
    let deadline = Instant::now() + timeout;
    let mut out = String::new();
    let mut stdout = child.stdout.take();
    // Read in a thread so a large output can't block the child.
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(o) = stdout.as_mut() {
            let _ = o.read_to_string(&mut s);
        }
        s
    });
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{} did not answer in time", argv[0]));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    out.push_str(&reader.join().unwrap_or_default());
    if !status.success() {
        let mut err = String::new();
        if let Some(mut e) = child.stderr.take() {
            let _ = e.read_to_string(&mut err);
        }
        let first = err.lines().next().unwrap_or("");
        let code = status.code().unwrap_or(-1);
        return Err(if first.is_empty() {
            format!("{} exited {code}", argv[0])
        } else {
            format!("{} exited {code}: {first}", argv[0])
        });
    }
    Ok(out)
}

