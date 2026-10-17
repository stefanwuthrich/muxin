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

    pub fn run(rx: mpsc::Receiver<String>, changed: impl Fn(&str) -> bool) {
        // What's there now is old news; only changes from here on count.
        let mut last = paste().unwrap_or_default();
        loop {
            match rx.recv_timeout(Duration::from_millis(600)) {
                Ok(text) => {
                    copy(&text);
                    last = text;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            if let Some(now) = paste() {
                if now != last && now.len() <= MAX {
                    if !changed(&now) {
                        return;
                    }
                    last = now;
                }
            }
        }
    }
}
