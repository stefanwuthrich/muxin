//! Wayland screen capture. X11's picture of a Wayland desktop is black, so the
//! frames come from the desktop portal's screen share. GStreamer reads the
//! PipeWire stream the portal opens; this process stays free of libpipewire
//! (the shipped agent is statically linked).

use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value as ZValue};

use super::Screen;

/// A Wayland session: the compositor owns the picture, not the X root window.
pub(super) fn wayland_session() -> bool {
    match std::env::var("XDG_SESSION_TYPE").ok().as_deref() {
        Some("wayland") => true,
        Some("x11" | "tty") => false,
        _ => std::env::var_os("WAYLAND_DISPLAY").is_some(),
    }
}

/// The frame size after fitting `max_width`.
fn frame_size(width: u32, height: u32, max_width: u32) -> (u32, u32) {
    let (width, height) = (width.max(1), height.max(1));
    if width > max_width {
        let height = (height as u64 * max_width as u64 / width as u64).max(1) as u32;
        (max_width, height)
    } else {
        (width, height)
    }
}

fn request_path(unique_name: &str, token: &str) -> String {
    let sender = unique_name.trim_start_matches(':').replace('.', "_");
    format!("/org/freedesktop/portal/desktop/request/{sender}/{token}")
}

fn gst_bin() -> Result<PathBuf, String> {
    let missing = "This Wayland desktop can't be shown through X11 (that picture is black). Install GStreamer (gst-launch-1.0) and open Desktop again.";
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join("gst-launch-1.0");
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(missing.into())
}

fn token_file() -> PathBuf {
    crate::home_dir().join("screencast.token")
}

fn saved_token() -> Option<String> {
    let text = std::fs::read_to_string(token_file()).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn save_token(token: &str) {
    if token.is_empty() {
        return;
    }
    let path = token_file();
    let tmp = path.with_extension("token.tmp");
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let Ok(mut file) = opts.open(&tmp) else {
        return;
    };
    if file.write_all(token.as_bytes()).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

fn dict(pairs: Vec<(&str, ZValue<'static>)>) -> HashMap<String, ZValue<'static>> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

/// Calls one portal method and waits for its Request.Response.
fn request(
    conn: &Connection,
    method: &str,
    body: &(impl Serialize + zbus::zvariant::DynamicType),
    token: &str,
) -> Result<(u32, HashMap<String, OwnedValue>), String> {
    let unique = conn
        .unique_name()
        .map(|n| n.as_str().to_string())
        .ok_or_else(|| "Can't reach the desktop bus.".to_string())?;
    let path = request_path(&unique, token);
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (tx, rx) = std::sync::mpsc::channel();
    let conn_wait = conn.clone();
    let path_wait = path.clone();
    std::thread::spawn(move || {
        let Ok(proxy) = Proxy::new(
            &conn_wait,
            "org.freedesktop.portal.Desktop",
            path_wait.as_str(),
            "org.freedesktop.portal.Request",
        ) else {
            let _ = ready_tx.send(());
            return;
        };
        let Ok(mut signals) = proxy.receive_signal("Response") else {
            let _ = ready_tx.send(());
            return;
        };
        let _ = ready_tx.send(());
        if let Some(msg) = signals.next() {
            let _ = tx.send(msg);
        }
    });
    let _ = ready_rx.recv_timeout(Duration::from_secs(2));
    let portal = Proxy::new(
        conn,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.ScreenCast",
    )
    .map_err(|e| format!("Can't reach the screen share service ({e})."))?;
    portal
        .call_method(method, body)
        .map_err(|e| format!("The screen share service refused {method} ({e})."))?;
    let msg = rx.recv_timeout(Duration::from_secs(90)).map_err(|_| {
        "The screen share dialog wasn't answered. Open Desktop again and choose a monitor to share."
            .to_string()
    })?;
    msg.body()
        .deserialize()
        .map_err(|_| "The screen share service gave an unexpected answer.".to_string())
}

fn outcome(
    code: u32,
    results: HashMap<String, OwnedValue>,
) -> Result<HashMap<String, OwnedValue>, String> {
    match code {
        0 => Ok(results),
        1 => Err("Screen sharing was cancelled. Open Desktop again and allow the share.".into()),
        _ => Err("Screen sharing was refused.".into()),
    }
}

fn owned_string(value: &OwnedValue) -> Option<String> {
    String::try_from(value.try_clone().ok()?).ok()
}

fn owned_path(value: &OwnedValue) -> Option<String> {
    OwnedObjectPath::try_from(value.try_clone().ok()?)
        .ok()
        .map(|p| p.to_string())
}

fn owned_u32(value: &OwnedValue) -> Option<u32> {
    let cloned = value.try_clone().ok()?;
    if let Ok(n) = i32::try_from(cloned.try_clone().ok()?) {
        return Some(n.max(0) as u32);
    }
    u32::try_from(cloned).ok()
}

fn size_of(props: &HashMap<String, OwnedValue>) -> Option<(u32, u32)> {
    let value = props.get("size")?.try_clone().ok()?;
    let structure = zbus::zvariant::Structure::try_from(value).ok()?;
    let fields = structure.into_fields();
    let w = owned_u32(&OwnedValue::try_from(fields.first()?.try_clone().ok()?).ok()?)?;
    let h = owned_u32(&OwnedValue::try_from(fields.get(1)?.try_clone().ok()?).ok()?)?;
    Some((w.max(1), h.max(1)))
}

fn first_stream(results: &HashMap<String, OwnedValue>) -> Result<(u32, u32, u32), String> {
    let streams = results
        .get("streams")
        .ok_or_else(|| "The screen share had no picture.".to_string())?;
    let list: Vec<zbus::zvariant::Structure> = Vec::try_from(
        streams
            .try_clone()
            .map_err(|_| "The screen share service gave an unexpected picture.".to_string())?,
    )
    .map_err(|_| "The screen share service gave an unexpected picture.".to_string())?;
    let structure = list
        .into_iter()
        .next()
        .ok_or_else(|| "No monitor was shared. Open Desktop again and choose one.".to_string())?;
    let fields = structure.into_fields();
    let node = owned_u32(
        &OwnedValue::try_from(
            fields
                .first()
                .ok_or_else(|| "The screen share had no picture.".to_string())?
                .try_clone()
                .map_err(|_| "The screen share had no picture.".to_string())?,
        )
        .map_err(|_| "The screen share had no picture.".to_string())?,
    )
    .ok_or_else(|| "The screen share had no picture.".to_string())?;
    let props: HashMap<String, OwnedValue> = HashMap::try_from(
        OwnedValue::try_from(
            fields
                .get(1)
                .ok_or_else(|| "The shared screen didn't say its size.".to_string())?
                .try_clone()
                .map_err(|_| "The shared screen didn't say its size.".to_string())?,
        )
        .map_err(|_| "The shared screen didn't say its size.".to_string())?,
    )
    .map_err(|_| "The shared screen didn't say its size.".to_string())?;
    let (w, h) = size_of(&props).ok_or_else(|| {
        "The shared screen didn't say its size. Open Desktop again and choose a monitor."
            .to_string()
    })?;
    Ok((node, w, h))
}

fn string_field(results: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    owned_string(results.get(key)?)
}

fn object_field(results: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    let value = results.get(key)?;
    // The portal sends the session as a string, not an object path.
    owned_string(value).or_else(|| owned_path(value))
}

/// Opens a portal screen share and the GStreamer process that reads it.
pub(super) fn open(fps: u32, max_width: u32) -> Result<PortalScreen, String> {
    let gst = gst_bin()?;
    let conn = Connection::session().map_err(|_| {
        "Can't reach this session's desktop bus, so the Wayland screen can't be shared.".to_string()
    })?;
    let pid = std::process::id();
    let create_token = format!("muxenc{pid}");
    let created = {
        let (code, results) = request(
            &conn,
            "CreateSession",
            &dict(vec![
                ("handle_token", ZValue::from(create_token.clone())),
                ("session_handle_token", ZValue::from(format!("muxens{pid}"))),
            ]),
            &create_token,
        )?;
        outcome(code, results)?
    };
    let session = object_field(&created, "session_handle")
        .ok_or_else(|| "The screen share service didn't open a session.".to_string())?;
    let select_token = format!("muxenl{pid}");
    let mut select = dict(vec![
        ("handle_token", ZValue::from(select_token.clone())),
        ("types", ZValue::from(1u32)),
        ("multiple", ZValue::from(false)),
        ("cursor_mode", ZValue::from(2u32)),
        ("persist_mode", ZValue::from(2u32)),
    ]);
    if let Some(saved) = saved_token() {
        select.insert("restore_token".into(), ZValue::from(saved));
    }
    let (code, selected) = request(
        &conn,
        "SelectSources",
        &(
            OwnedObjectPath::try_from(session.as_str())
                .map_err(|_| "The screen share service gave a bad session.".to_string())?,
            select,
        ),
        &select_token,
    )?;
    outcome(code, selected).map_err(|e| {
        close_session(&conn, &session);
        e
    })?;
    let start_token = format!("muxent{pid}");
    let (code, started) = request(
        &conn,
        "Start",
        &(
            OwnedObjectPath::try_from(session.as_str())
                .map_err(|_| "The screen share service gave a bad session.".to_string())?,
            "",
            dict(vec![("handle_token", ZValue::from(start_token.clone()))]),
        ),
        &start_token,
    )
    .map_err(|e| {
        close_session(&conn, &session);
        e
    })?;
    let started = outcome(code, started).map_err(|e| {
        close_session(&conn, &session);
        e
    })?;
    if let Some(token) = string_field(&started, "restore_token") {
        save_token(&token);
    }
    let (node, width, height) = first_stream(&started).map_err(|e| {
        close_session(&conn, &session);
        e
    })?;
    // The node lives on the portal's PipeWire connection. The default
    // connection is this machine's camera, which is the wrong picture.
    let pipewire = open_pipewire(&conn, &session).map_err(|e| {
        close_session(&conn, &session);
        e
    })?;
    let (width, height) = frame_size(width, height, max_width);
    let (mut child, stdout, log) = spawn_gst(gst.as_path(), pipewire, node, width, height, fps)
        .map_err(|e| {
            close_session(&conn, &session);
            e
        })?;
    let started_at = std::time::Instant::now();
    loop {
        if child.try_wait().ok().flatten().is_some() {
            let note = log.lock().ok().map(|g| g.clone()).unwrap_or_default();
            return Err(PortalScreen::fail(&conn, &session, &mut child, &note));
        }
        if started_at.elapsed() > Duration::from_millis(400) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(PortalScreen {
        conn,
        session,
        gst: child,
        stdout,
        log,
        w: width as usize,
        h: height as usize,
        buf: vec![0; width as usize * height as usize * 4],
    })
}

/// One shared monitor, delivered as BGRx frames.
pub(super) struct PortalScreen {
    conn: Connection,
    session: String,
    gst: Child,
    stdout: std::process::ChildStdout,
    log: Arc<Mutex<String>>,
    w: usize,
    h: usize,
    buf: Vec<u8>,
}

impl PortalScreen {
    fn fail(conn: &Connection, session: &str, gst: &mut Child, log: &str) -> String {
        let _ = gst.kill();
        let _ = gst.wait();
        close_session(conn, session);
        if log.is_empty() {
            "Couldn't read the shared screen.".into()
        } else {
            format!("Couldn't read the shared screen. {log}")
        }
    }
}

fn close_session(conn: &Connection, session: &str) {
    if session.is_empty() {
        return;
    }
    let Ok(proxy) = Proxy::new(
        conn,
        "org.freedesktop.portal.Desktop",
        session,
        "org.freedesktop.portal.Session",
    ) else {
        return;
    };
    let _ = proxy.call_noreply("Close", &());
}

impl Screen for PortalScreen {
    fn size(&self) -> (usize, usize) {
        (self.w, self.h)
    }

    fn grab(&mut self) -> Option<Option<&[u8]>> {
        match self.stdout.read_exact(&mut self.buf) {
            Ok(()) => Some(Some(&self.buf)),
            Err(_) => {
                let log = self.log.lock().ok().map(|g| g.clone()).unwrap_or_default();
                if !log.is_empty() {
                    eprintln!("screen: {log}");
                }
                None
            }
        }
    }

    fn apply(&mut self, _event: &Value, _frame: (usize, usize)) {}
}

impl Drop for PortalScreen {
    fn drop(&mut self) {
        let _ = self.gst.kill();
        let _ = self.gst.wait();
        close_session(&self.conn, &self.session);
    }
}

fn open_pipewire(conn: &Connection, session: &str) -> Result<zbus::zvariant::OwnedFd, String> {
    let portal = Proxy::new(
        conn,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.ScreenCast",
    )
    .map_err(|e| format!("Can't reach the screen share service ({e})."))?;
    let session = OwnedObjectPath::try_from(session)
        .map_err(|_| "The screen share service gave a bad session.".to_string())?;
    let empty: HashMap<String, ZValue> = HashMap::new();
    portal
        .call("OpenPipeWireRemote", &(session, empty))
        .map_err(|_| "The screen share didn't hand over its picture.".to_string())
}

fn spawn_gst(
    bin: &std::path::Path,
    pipewire: zbus::zvariant::OwnedFd,
    node: u32,
    width: u32,
    height: u32,
    fps: u32,
) -> Result<(Child, std::process::ChildStdout, Arc<Mutex<String>>), String> {
    // `path` is the portal's node id. `target-object` matches a serial, and
    // on this machine that serial is the webcam.
    let caps = format!("video/x-raw,format=BGRx,width={width},height={height},framerate={fps}/1");
    let raw = pipewire.as_raw_fd();
    let mut cmd = Command::new(bin);
    cmd.args([
        "-q",
        "pipewiresrc",
        "fd=3",
        &format!("path={node}"),
        "client-name=muxen-agent",
        "!",
        "videoconvert",
        "!",
        "videoscale",
        "!",
        "videorate",
        "!",
        &caps,
        "!",
        "fdsink",
        "fd=1",
        "sync=false",
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    // The portal fd is close-on-exec. Hand it to GStreamer as fd 3.
    unsafe {
        cmd.pre_exec(move || {
            if libc::dup2(raw, 3) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = cmd
        .spawn()
        .map_err(|_| "Couldn't start GStreamer to read the shared screen.".to_string())?;
    drop(pipewire);
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Couldn't read the shared screen.".to_string())?;
    let stderr = child.stderr.take();
    let log = Arc::new(Mutex::new(String::new()));
    if let Some(mut err) = stderr {
        let log = log.clone();
        std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = err.read_to_string(&mut buf);
            if let Ok(mut slot) = log.lock() {
                *slot = buf
                    .chars()
                    .rev()
                    .take(400)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect();
            }
        });
    }
    Ok((child, stdout, log))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_a_wide_screen_to_the_frame() {
        assert_eq!(frame_size(3840, 2160, 1920), (1920, 1080));
        assert_eq!(frame_size(1280, 800, 1920), (1280, 800));
        assert_eq!(frame_size(0, 0, 1920), (1, 1));
    }

    #[test]
    fn request_paths_follow_the_portal() {
        assert_eq!(
            request_path(":1.42", "muxenc7"),
            "/org/freedesktop/portal/desktop/request/1_42/muxenc7"
        );
    }
}
