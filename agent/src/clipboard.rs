//! `muxen-agent clip`: this machine's clipboard, shared with the app. Frames
//! on stdin set it (text); frames on stdout say it changed (text copied here).
//! Linux: owns the Wayland clipboard where the compositor allows it, and the
//! X clipboard otherwise (see `src/os_clip.rs`). macOS: pbcopy and pbpaste.

use std::io::{self, Write};
use std::sync::mpsc;
use std::thread;
#[cfg(target_os = "macos")]
use std::time::Duration;

/// A clipboard's text (the frame kind on the link and on stdin/stdout).
pub const CLIP: u8 = 40;
/// Bigger text isn't shared (X serves it in pieces, which this doesn't do).
#[cfg_attr(target_os = "linux", allow(dead_code))]
pub const MAX: usize = 256 * 1024;

#[cfg(target_os = "linux")]
#[path = "../../src/os_clip.rs"]
mod os_clip;

pub fn run() -> i32 {
    // What the app sets, as it arrives.
    let (tx, rx) = mpsc::channel::<String>();
    thread::spawn(move || {
        let mut stdin = io::stdin().lock();
        while let Ok((kind, payload)) = crate::proto::read_frame(&mut stdin) {
            if kind == CLIP
                && tx
                    .send(String::from_utf8_lossy(&payload).into_owned())
                    .is_err()
            {
                break;
            }
        }
    });
    let changed = |text: &str| -> bool {
        let mut out = io::stdout().lock();
        crate::proto::write_frame(&mut out, CLIP, text.as_bytes())
            .and_then(|_| out.flush())
            .is_ok()
    };
    #[cfg(target_os = "linux")]
    {
        match os_clip::run(rx, changed) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("clipboard: {e}");
                1
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        mac::run(rx, changed);
        0
    }
    #[cfg(windows)]
    {
        crate::win_screen::clip(rx, changed);
        0
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use std::process::{Command, Stdio};

    fn paste() -> Option<String> {
        let out = Command::new("pbpaste")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn copy(text: &str) {
        if let Ok(mut child) = Command::new("pbcopy")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
        }
    }

    unsafe extern "C" {
        fn muxen_pasteboard(concealed: *mut i32) -> i64;
    }

    /// The pasteboard's change count, and whether a password manager marked
    /// what's there private (see mac_screen.m).
    fn pasteboard() -> (i64, bool) {
        let mut concealed = 0;
        let count = unsafe { muxen_pasteboard(&mut concealed) };
        (count, concealed != 0)
    }

    pub fn run(rx: mpsc::Receiver<String>, changed: impl Fn(&str) -> bool) {
        // What's there now is old news; only copies from here on count.
        let mut seen = pasteboard().0;
        loop {
            match rx.recv_timeout(Duration::from_millis(600)) {
                Ok(text) => {
                    copy(&text);
                    seen = pasteboard().0;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            let (count, private) = pasteboard();
            if count == seen {
                continue;
            }
            // A password manager's copy stays on this Mac.
            let text = if private { None } else { paste() };
            // Copied again while reading: that copy is looked at next time.
            if pasteboard().0 != count {
                continue;
            }
            seen = count;
            if let Some(now) = text.filter(|t| !t.is_empty() && t.len() <= MAX)
                && !changed(&now)
            {
                return;
            }
        }
    }
}
