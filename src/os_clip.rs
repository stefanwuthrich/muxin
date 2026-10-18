//! This computer's clipboard on Linux, for the shared clipboard.
//!
//! GPUI's Wayland clipboard only reads and writes while a window is focused
//! and a click or keypress has produced a serial, so a background poll cannot
//! use it. This owns the selection itself:
//!
//! - **Wayland** `ext-data-control` or `wlr-data-control`, when the compositor
//!   offers one (Sway, Hyprland, KDE). Those protocols set the clipboard
//!   without a focus serial.
//! - **X11** `CLIPBOARD` otherwise. A pure X session uses it directly. GNOME
//!   Wayland does not offer data-control; it bridges this selection to the
//!   Wayland clipboard, so the same path covers both directions there.
//!
//! The agent (`muxen-agent clip`) and the window share this file.

use std::sync::{OnceLock, mpsc};
use std::thread;

/// Bigger text isn't shared.
const MAX: usize = 256 * 1024;
/// Offered with the text so a selection we just set is recognizable without
/// reading it back. Reading our own offer deadlocks: the compositor asks us
/// for the bytes on this same thread.
const MARKER: &str = "application/x-muxen-clip";
const TEXT_MIMES: &[&str] = &[
    "text/plain;charset=utf-8",
    "text/plain;charset=UTF-8",
    "text/plain",
    "UTF8_STRING",
    "TEXT",
    "STRING",
];
const OFFER_MIMES: &[&str] = &[
    "text/plain;charset=utf-8",
    "text/plain",
    "UTF8_STRING",
    "TEXT",
    "STRING",
    MARKER,
];

/// Which desktop clipboard to own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Backend {
    Ext,
    Wlr,
    X11,
    None,
}

/// `wayland` is whether `WAYLAND_DISPLAY` is set, `globals` the compositor's
/// interface names, `display` whether `DISPLAY` is set.
fn choose_backend(wayland: bool, globals: &[&str], display: bool) -> Backend {
    if wayland && globals.contains(&"ext_data_control_manager_v1") {
        Backend::Ext
    } else if wayland && globals.contains(&"zwlr_data_control_manager_v1") {
        Backend::Wlr
    } else if display {
        Backend::X11
    } else {
        Backend::None
    }
}

fn env_set(key: &str) -> bool {
    std::env::var_os(key).is_some_and(|v| !v.is_empty())
}

fn text_mime(offered: &[String]) -> Option<&'static str> {
    TEXT_MIMES
        .iter()
        .copied()
        .find(|want| offered.iter().any(|have| have == want))
}

fn worth_sharing(last: &str, text: &str) -> bool {
    !text.is_empty() && text.len() <= MAX && text != last
}

/// KeePassXC and other password managers on Linux mark a copy with
/// `x-kde-passwordManagerHint: secret`. Those stay on this computer.
fn password_secret(hint: &[u8]) -> bool {
    hint.trim_ascii() == b"secret"
}

/// Watches the desktop clipboard until `rx` disconnects. `on_change` is a new
/// copy made here; returning false stops the watch. Text sent on `rx` is copied here.
pub fn run(
    rx: mpsc::Receiver<String>,
    on_change: impl FnMut(&str) -> bool + 'static,
) -> Result<(), String> {
    let wayland = env_set("WAYLAND_DISPLAY");
    let display = env_set("DISPLAY");
    let backend = if wayland {
        match wayland::globals() {
            Ok(names) => {
                let refs: Vec<&str> = names.iter().map(String::as_str).collect();
                choose_backend(true, &refs, display)
            }
            Err(_) => choose_backend(false, &[], display),
        }
    } else {
        choose_backend(false, &[], display)
    };
    match backend {
        Backend::Ext => wayland::run(wayland::Protocol::Ext, rx, on_change),
        Backend::Wlr => wayland::run(wayland::Protocol::Wlr, rx, on_change),
        Backend::X11 => x11::run(rx, on_change),
        Backend::None => Err("No desktop session here, so no clipboard.".into()),
    }
}

static SET: OnceLock<mpsc::Sender<String>> = OnceLock::new();

/// Starts the watch on a background thread. The window calls this once.
/// The agent calls [`run`] instead.
#[allow(dead_code)]
pub fn ensure_started(on_change: impl FnMut(&str) -> bool + Send + 'static) {
    let (tx, rx) = mpsc::channel();
    if SET.set(tx).is_err() {
        return;
    }
    thread::spawn(move || {
        if let Err(e) = run(rx, on_change) {
            eprintln!("clipboard: {e}");
        }
    });
}

/// Puts `text` on this computer's clipboard. False until [`ensure_started`].
#[allow(dead_code)]
pub fn set_text(text: &str) -> bool {
    SET.get()
        .is_some_and(|tx| tx.send(text.to_string()).is_ok())
}

mod x11 {
    use super::{MAX, password_secret, worth_sharing};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;
    use x11rb::connection::Connection;
    use x11rb::protocol::Event;
    use x11rb::protocol::xfixes::{self, ConnectionExt as _};
    use x11rb::protocol::xproto::*;
    use x11rb::wrapper::ConnectionExt as _;

    const NONE: u32 = 0;

    pub fn run(
        rx: mpsc::Receiver<String>,
        mut changed: impl FnMut(&str) -> bool,
    ) -> Result<(), String> {
        let (conn, screen_num) = x11rb::connect(None).map_err(|e| e.to_string())?;
        let root = conn.setup().roots[screen_num].root;
        let win = conn.generate_id().map_err(|e| e.to_string())?;
        conn.create_window(
            0,
            win,
            root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            0,
            &CreateWindowAux::new(),
        )
        .map_err(|e| e.to_string())?;
        let atom = |name: &str| -> Result<u32, String> {
            Ok(conn
                .intern_atom(false, name.as_bytes())
                .map_err(|e| e.to_string())?
                .reply()
                .map_err(|e| e.to_string())?
                .atom)
        };
        let (clipboard, utf8, targets, text_atom, plain, prop, incr) = (
            atom("CLIPBOARD")?,
            atom("UTF8_STRING")?,
            atom("TARGETS")?,
            atom("TEXT")?,
            atom("text/plain;charset=utf-8")?,
            atom("MUXEN_CLIP")?,
            atom("INCR")?,
        );
        let hint = atom("x-kde-passwordManagerHint")?;
        let string: u32 = AtomEnum::STRING.into();
        conn.xfixes_query_version(5, 0)
            .map_err(|e| e.to_string())?
            .reply()
            .map_err(|e| format!("no XFixes: {e}"))?;
        conn.xfixes_select_selection_input(
            win,
            clipboard,
            xfixes::SelectionEventMask::SET_SELECTION_OWNER,
        )
        .map_err(|e| e.to_string())?;
        conn.flush().map_err(|e| e.to_string())?;

        // Text this window offers while it owns the clipboard, and the last
        // text either way (so nothing echoes back).
        let mut owned: Option<Vec<u8>> = None;
        let mut last = String::new();
        // When the copy being asked about was made (answers about an older one are dropped).
        let mut asked_at = x11rb::CURRENT_TIME;
        loop {
            while let Some(event) = conn.poll_for_event().map_err(|e| e.to_string())? {
                match event {
                    // Something else took the clipboard: ask what it offers
                    // first, since a password manager marks its copies.
                    Event::XfixesSelectionNotify(e) if e.owner != win && e.owner != NONE => {
                        asked_at = e.selection_timestamp;
                        let _ = conn.convert_selection(win, clipboard, targets, prop, asked_at);
                        let _ = conn.flush();
                    }
                    Event::SelectionNotify(e)
                        if e.requestor == win
                            && (e.time == asked_at || e.time == x11rb::CURRENT_TIME) =>
                    {
                        if e.property == NONE {
                            if e.target == targets {
                                let _ =
                                    conn.convert_selection(win, clipboard, utf8, prop, asked_at);
                                let _ = conn.flush();
                            }
                            continue;
                        }
                        let Ok(cookie) = conn.get_property(
                            true,
                            win,
                            prop,
                            AtomEnum::ANY,
                            0,
                            (MAX / 4) as u32 + 1,
                        ) else {
                            continue;
                        };
                        let Ok(reply) = cookie.reply() else { continue };
                        if e.target == targets {
                            let offered: Vec<u32> = reply
                                .value32()
                                .map(|atoms| atoms.collect())
                                .unwrap_or_default();
                            let next = if offered.contains(&hint) { hint } else { utf8 };
                            let _ = conn.convert_selection(win, clipboard, next, prop, asked_at);
                            let _ = conn.flush();
                            continue;
                        }
                        if e.target == hint {
                            if !password_secret(&reply.value) {
                                let _ =
                                    conn.convert_selection(win, clipboard, utf8, prop, asked_at);
                                let _ = conn.flush();
                            }
                            continue;
                        }
                        if reply.type_ == incr || reply.value.len() > MAX {
                            continue;
                        }
                        let text = String::from_utf8_lossy(&reply.value).into_owned();
                        if worth_sharing(&last, &text) {
                            if !changed(&text) {
                                return Ok(());
                            }
                            last = text;
                        }
                    }
                    // A program pastes what this window offers.
                    Event::SelectionRequest(e) => {
                        let mut property = if e.property == NONE {
                            e.target
                        } else {
                            e.property
                        };
                        match &owned {
                            _ if e.target == targets => {
                                let _ = conn.change_property32(
                                    PropMode::REPLACE,
                                    e.requestor,
                                    property,
                                    AtomEnum::ATOM,
                                    &[targets, utf8, string, text_atom, plain],
                                );
                            }
                            Some(bytes) if [utf8, string, text_atom, plain].contains(&e.target) => {
                                let ty = if e.target == string { string } else { utf8 };
                                let _ = conn.change_property8(
                                    PropMode::REPLACE,
                                    e.requestor,
                                    property,
                                    ty,
                                    bytes,
                                );
                            }
                            _ => property = NONE,
                        }
                        let notify = SelectionNotifyEvent {
                            response_type: SELECTION_NOTIFY_EVENT,
                            sequence: 0,
                            time: e.time,
                            requestor: e.requestor,
                            selection: e.selection,
                            target: e.target,
                            property,
                        };
                        let _ = conn.send_event(false, e.requestor, EventMask::NO_EVENT, notify);
                        let _ = conn.flush();
                    }
                    Event::SelectionClear(e) if e.owner == win => owned = None,
                    _ => {}
                }
            }
            loop {
                match rx.try_recv() {
                    Ok(text) => {
                        owned = Some(text.clone().into_bytes());
                        last = text;
                        let _ = conn.set_selection_owner(win, clipboard, x11rb::CURRENT_TIME);
                        let _ = conn.flush();
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    // The app let go.
                    Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
}

mod wayland {
    use super::{MARKER, MAX, OFFER_MIMES, password_secret, text_mime, worth_sharing};
    use std::io::{self, Read, Write};
    use std::os::fd::{AsFd, AsRawFd};
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc;
    use std::time::Duration;

    use wayland_client::event_created_child;
    use wayland_client::globals::{GlobalListContents, registry_queue_init};
    use wayland_client::protocol::{wl_registry, wl_seat::WlSeat};
    use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};
    use wayland_protocols::ext::data_control::v1::client::ext_data_control_device_v1::{
        self, ExtDataControlDeviceV1,
    };
    use wayland_protocols::ext::data_control::v1::client::ext_data_control_manager_v1::ExtDataControlManagerV1;
    use wayland_protocols::ext::data_control::v1::client::ext_data_control_offer_v1::{
        self, ExtDataControlOfferV1,
    };
    use wayland_protocols::ext::data_control::v1::client::ext_data_control_source_v1::{
        self, ExtDataControlSourceV1,
    };
    use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_device_v1::{
        self, ZwlrDataControlDeviceV1,
    };
    use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_manager_v1::ZwlrDataControlManagerV1;
    use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_offer_v1::{
        self, ZwlrDataControlOfferV1,
    };
    use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_source_v1::{
        self, ZwlrDataControlSourceV1,
    };

    pub enum Protocol {
        Ext,
        Wlr,
    }

    struct Probe;

    impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Probe {
        fn event(
            _: &mut Self,
            _: &wl_registry::WlRegistry,
            _: wl_registry::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    /// Interface names advertised by the compositor, when Wayland connects.
    pub fn globals() -> Result<Vec<String>, ()> {
        let conn = Connection::connect_to_env().map_err(|_| ())?;
        let (list, _) = registry_queue_init::<Probe>(&conn).map_err(|_| ())?;
        Ok(list
            .contents()
            .clone_list()
            .into_iter()
            .map(|g| g.interface)
            .collect())
    }

    trait OfferApi {
        fn id(&self) -> wayland_client::backend::ObjectId;
        fn recv(&self, mime: &str, fd: std::os::fd::BorrowedFd<'_>);
        fn destroy(&self);
    }

    impl OfferApi for ExtDataControlOfferV1 {
        fn id(&self) -> wayland_client::backend::ObjectId {
            Proxy::id(self)
        }
        fn recv(&self, mime: &str, fd: std::os::fd::BorrowedFd<'_>) {
            self.receive(mime.to_string(), fd);
        }
        fn destroy(&self) {
            ExtDataControlOfferV1::destroy(self);
        }
    }

    impl OfferApi for ZwlrDataControlOfferV1 {
        fn id(&self) -> wayland_client::backend::ObjectId {
            Proxy::id(self)
        }
        fn recv(&self, mime: &str, fd: std::os::fd::BorrowedFd<'_>) {
            self.receive(mime.to_string(), fd);
        }
        fn destroy(&self) {
            ZwlrDataControlOfferV1::destroy(self);
        }
    }

    trait HeldSource {
        fn id(&self) -> wayland_client::backend::ObjectId;
        fn destroy(&self);
    }

    impl HeldSource for ExtDataControlSourceV1 {
        fn id(&self) -> wayland_client::backend::ObjectId {
            Proxy::id(self)
        }
        fn destroy(&self) {
            ExtDataControlSourceV1::destroy(self);
        }
    }

    impl HeldSource for ZwlrDataControlSourceV1 {
        fn id(&self) -> wayland_client::backend::ObjectId {
            Proxy::id(self)
        }
        fn destroy(&self) {
            ZwlrDataControlSourceV1::destroy(self);
        }
    }

    struct LiveOffer {
        api: Box<dyn OfferApi>,
        mimes: Vec<String>,
    }

    struct Hub {
        offers: Vec<LiveOffer>,
        current: Option<wayland_client::backend::ObjectId>,
        held: Vec<Box<dyn HeldSource>>,
        last: String,
        /// False during the first roundtrip, which reports whatever is already
        /// copied. Only copies made after the watch starts are shared.
        live: bool,
        stop: bool,
        failed: bool,
        on_change: Box<dyn FnMut(&str) -> bool>,
    }

    impl Hub {
        fn new(on_change: impl FnMut(&str) -> bool + 'static) -> Self {
            Self {
                offers: Vec::new(),
                current: None,
                held: Vec::new(),
                last: String::new(),
                live: false,
                stop: false,
                failed: false,
                on_change: Box::new(on_change),
            }
        }

        fn note_offer(&mut self, api: Box<dyn OfferApi>) {
            if self.offers.len() >= 8 {
                let current = self.current.clone();
                if let Some(i) = self.offers.iter().position(|o| Some(o.api.id()) != current) {
                    self.offers[i].api.destroy();
                    self.offers.remove(i);
                }
            }
            self.offers.push(LiveOffer {
                api,
                mimes: Vec::new(),
            });
        }

        fn push_mime(&mut self, id: &wayland_client::backend::ObjectId, mime: String) {
            if let Some(offer) = self.offers.iter_mut().find(|o| o.api.id() == *id) {
                offer.mimes.push(mime);
            }
        }

        fn hold(&mut self, source: Box<dyn HeldSource>) {
            self.held.push(source);
        }

        fn forget(&mut self, id: &wayland_client::backend::ObjectId) {
            if let Some(i) = self.held.iter().position(|s| s.id() == *id) {
                self.held[i].destroy();
                self.held.remove(i);
            }
        }

        fn accept(&mut self, text: String) {
            if !self.live || !worth_sharing(&self.last, &text) {
                return;
            }
            if (self.on_change)(&text) {
                self.last = text;
            } else {
                self.stop = true;
            }
        }

        /// A new clipboard selection. Offers that carry [`MARKER`] are ones
        /// this process just set.
        fn on_selection(&mut self, id: wayland_client::backend::ObjectId, conn: &Connection) {
            if self.current.as_ref() != Some(&id) {
                if let Some(prev) = self.current.take() {
                    if let Some(i) = self.offers.iter().position(|o| o.api.id() == prev) {
                        self.offers[i].api.destroy();
                        self.offers.remove(i);
                    }
                }
                self.current = Some(id.clone());
            }
            if !self.live {
                return;
            }
            let mime = {
                let Some(offer) = self.offers.iter().find(|o| o.api.id() == id) else {
                    return;
                };
                if offer.mimes.iter().any(|m| m == MARKER) {
                    return;
                }
                if offer.mimes.iter().any(|m| m == "x-kde-passwordManagerHint") {
                    if let Some(bytes) =
                        read_offer(offer.api.as_ref(), "x-kde-passwordManagerHint", conn)
                        && password_secret(&bytes)
                    {
                        return;
                    }
                }
                text_mime(&offer.mimes).map(str::to_string)
            };
            let Some(mime) = mime else { return };
            let Ok((read, write)) = UnixStream::pair() else {
                return;
            };
            if let Some(offer) = self.offers.iter().find(|o| o.api.id() == id) {
                offer.api.recv(&mime, write.as_fd());
            }
            drop(write);
            let _ = conn.flush();
            if let Some(bytes) = read_all(read) {
                self.accept(String::from_utf8_lossy(&bytes).into_owned());
            }
        }
    }

    fn read_offer(api: &dyn OfferApi, mime: &str, conn: &Connection) -> Option<Vec<u8>> {
        let Ok((read, write)) = UnixStream::pair() else {
            return None;
        };
        api.recv(mime, write.as_fd());
        drop(write);
        let _ = conn.flush();
        read_all(read)
    }

    fn read_all(mut read: UnixStream) -> Option<Vec<u8>> {
        read.set_read_timeout(Some(Duration::from_millis(400)))
            .ok()?;
        let mut out = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            match read.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if out.len() + n > MAX {
                        return None;
                    }
                    out.extend_from_slice(&buf[..n]);
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return None,
            }
        }
        (!out.is_empty()).then_some(out)
    }

    fn wait<S>(queue: &EventQueue<S>, timeout_ms: i32) -> Result<(), String> {
        queue.flush().map_err(|e| e.to_string())?;
        if let Some(guard) = queue.prepare_read() {
            let mut pfd = libc::pollfd {
                fd: guard.connection_fd().as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: `pfd` points at one live `pollfd`, and `nfds` is 1.
            let rc = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
            if rc > 0 {
                if let Err(err) = guard.read() {
                    let again = matches!(
                        &err,
                        wayland_client::backend::WaylandError::Io(e)
                            if e.kind() == io::ErrorKind::WouldBlock
                                || e.kind() == io::ErrorKind::Interrupted
                    );
                    if !again {
                        return Err(err.to_string());
                    }
                }
            }
        }
        Ok(())
    }

    fn publish_bytes(text: &str) -> Option<Vec<u8>> {
        if text.is_empty() || text.len() > MAX {
            None
        } else {
            Some(text.as_bytes().to_vec())
        }
    }

    impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Hub {
        fn event(
            _: &mut Self,
            _: &wl_registry::WlRegistry,
            _: wl_registry::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<WlSeat, ()> for Hub {
        fn event(
            _: &mut Self,
            _: &WlSeat,
            _: wayland_client::protocol::wl_seat::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<ExtDataControlManagerV1, ()> for Hub {
        fn event(
            _: &mut Self,
            _: &ExtDataControlManagerV1,
            _: wayland_protocols::ext::data_control::v1::client::ext_data_control_manager_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<ZwlrDataControlManagerV1, ()> for Hub {
        fn event(
            _: &mut Self,
            _: &ZwlrDataControlManagerV1,
            _: wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_manager_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<ExtDataControlDeviceV1, ()> for Hub {
        fn event(
            state: &mut Self,
            _: &ExtDataControlDeviceV1,
            event: ext_data_control_device_v1::Event,
            _: &(),
            conn: &Connection,
            _: &QueueHandle<Self>,
        ) {
            match event {
                ext_data_control_device_v1::Event::DataOffer { id } => {
                    state.note_offer(Box::new(id))
                }
                ext_data_control_device_v1::Event::Selection { id } => match id {
                    Some(offer) => state.on_selection(Proxy::id(&offer), conn),
                    None => state.current = None,
                },
                ext_data_control_device_v1::Event::Finished => state.failed = true,
                _ => {}
            }
        }

        event_created_child!(Hub, ExtDataControlDeviceV1, [
            ext_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (ExtDataControlOfferV1, ()),
        ]);
    }

    impl Dispatch<ExtDataControlOfferV1, ()> for Hub {
        fn event(
            state: &mut Self,
            offer: &ExtDataControlOfferV1,
            event: ext_data_control_offer_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            if let ext_data_control_offer_v1::Event::Offer { mime_type } = event {
                state.push_mime(&Proxy::id(offer), mime_type);
            }
        }
    }

    impl Dispatch<ExtDataControlSourceV1, Vec<u8>> for Hub {
        fn event(
            state: &mut Self,
            source: &ExtDataControlSourceV1,
            event: ext_data_control_source_v1::Event,
            data: &Vec<u8>,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            match event {
                ext_data_control_source_v1::Event::Send { fd, .. } => {
                    let mut file = std::fs::File::from(fd);
                    let _ = file.write_all(data);
                }
                ext_data_control_source_v1::Event::Cancelled => state.forget(&Proxy::id(source)),
                _ => {}
            }
        }
    }

    impl Dispatch<ZwlrDataControlDeviceV1, ()> for Hub {
        fn event(
            state: &mut Self,
            _: &ZwlrDataControlDeviceV1,
            event: zwlr_data_control_device_v1::Event,
            _: &(),
            conn: &Connection,
            _: &QueueHandle<Self>,
        ) {
            match event {
                zwlr_data_control_device_v1::Event::DataOffer { id } => {
                    state.note_offer(Box::new(id))
                }
                zwlr_data_control_device_v1::Event::Selection { id } => match id {
                    Some(offer) => state.on_selection(Proxy::id(&offer), conn),
                    None => state.current = None,
                },
                zwlr_data_control_device_v1::Event::Finished => state.failed = true,
                _ => {}
            }
        }

        event_created_child!(Hub, ZwlrDataControlDeviceV1, [
            zwlr_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (ZwlrDataControlOfferV1, ()),
        ]);
    }

    impl Dispatch<ZwlrDataControlOfferV1, ()> for Hub {
        fn event(
            state: &mut Self,
            offer: &ZwlrDataControlOfferV1,
            event: zwlr_data_control_offer_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            if let zwlr_data_control_offer_v1::Event::Offer { mime_type } = event {
                state.push_mime(&Proxy::id(offer), mime_type);
            }
        }
    }

    impl Dispatch<ZwlrDataControlSourceV1, Vec<u8>> for Hub {
        fn event(
            state: &mut Self,
            source: &ZwlrDataControlSourceV1,
            event: zwlr_data_control_source_v1::Event,
            data: &Vec<u8>,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            match event {
                zwlr_data_control_source_v1::Event::Send { fd, .. } => {
                    let mut file = std::fs::File::from(fd);
                    let _ = file.write_all(data);
                }
                zwlr_data_control_source_v1::Event::Cancelled => state.forget(&Proxy::id(source)),
                _ => {}
            }
        }
    }

    fn offer_types(source_offer: &impl Fn(&str)) {
        for mime in OFFER_MIMES {
            source_offer(mime);
        }
    }

    pub fn run(
        protocol: Protocol,
        rx: mpsc::Receiver<String>,
        on_change: impl FnMut(&str) -> bool + 'static,
    ) -> Result<(), String> {
        let conn = Connection::connect_to_env().map_err(|e| e.to_string())?;
        let (globals, mut queue) = registry_queue_init::<Hub>(&conn).map_err(|e| e.to_string())?;
        let qh = queue.handle();
        let seat: WlSeat = globals.bind(&qh, 1..=1, ()).map_err(|e| e.to_string())?;
        // The device has to exist before the roundtrip, or the clipboard
        // already there is never delivered (and then never ignored).
        let mut publish: Box<dyn FnMut(&mut Hub, String)> = match protocol {
            Protocol::Ext => {
                let mgr: ExtDataControlManagerV1 =
                    globals.bind(&qh, 1..=1, ()).map_err(|e| e.to_string())?;
                let dev = mgr.get_data_device(&seat, &qh, ());
                let qh = qh.clone();
                Box::new(move |state, text| {
                    let Some(bytes) = publish_bytes(&text) else {
                        return;
                    };
                    let source = mgr.create_data_source(&qh, bytes);
                    offer_types(&|mime| source.offer(mime.to_string()));
                    dev.set_selection(Some(&source));
                    state.last = text;
                    state.hold(Box::new(source));
                })
            }
            Protocol::Wlr => {
                let mgr: ZwlrDataControlManagerV1 =
                    globals.bind(&qh, 1..=2, ()).map_err(|e| e.to_string())?;
                let dev = mgr.get_data_device(&seat, &qh, ());
                let qh = qh.clone();
                Box::new(move |state, text| {
                    let Some(bytes) = publish_bytes(&text) else {
                        return;
                    };
                    let source = mgr.create_data_source(&qh, bytes);
                    offer_types(&|mime| source.offer(mime.to_string()));
                    dev.set_selection(Some(&source));
                    state.last = text;
                    state.hold(Box::new(source));
                })
            }
        };
        let mut state = Hub::new(on_change);
        queue.roundtrip(&mut state).map_err(|e| e.to_string())?;
        state.live = true;
        loop {
            if state.failed {
                return Err("the compositor closed the clipboard".into());
            }
            if state.stop {
                return Ok(());
            }
            queue
                .dispatch_pending(&mut state)
                .map_err(|e| e.to_string())?;
            loop {
                match rx.try_recv() {
                    Ok(text) => publish(&mut state, text),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
                }
            }
            wait(&queue, 20)?;
            queue
                .dispatch_pending(&mut state)
                .map_err(|e| e.to_string())?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_password_managers_copy_says_secret() {
        assert!(password_secret(b"secret"));
        assert!(password_secret(b"secret\n"));
        assert!(!password_secret(b""));
        assert!(!password_secret(b"public"));
    }

    #[test]
    fn picks_a_clipboard_backend() {
        let cases = [
            (
                true,
                &[
                    "ext_data_control_manager_v1",
                    "zwlr_data_control_manager_v1",
                ][..],
                true,
                Backend::Ext,
            ),
            (
                true,
                &["zwlr_data_control_manager_v1"][..],
                true,
                Backend::Wlr,
            ),
            (
                true,
                &["zwlr_data_control_manager_v1"][..],
                false,
                Backend::Wlr,
            ),
            (true, &[][..], true, Backend::X11),
            (true, &[][..], false, Backend::None),
            (
                false,
                &["ext_data_control_manager_v1"][..],
                true,
                Backend::X11,
            ),
            (false, &[][..], true, Backend::X11),
            (false, &[][..], false, Backend::None),
        ];
        for (wayland, globals, display, want) in cases {
            assert_eq!(
                choose_backend(wayland, globals, display),
                want,
                "{wayland} {globals:?} {display}"
            );
        }
    }

    #[test]
    fn prefers_utf8_text() {
        assert_eq!(
            text_mime(&["text/plain".into(), "text/plain;charset=utf-8".into()]),
            Some("text/plain;charset=utf-8")
        );
        assert_eq!(
            text_mime(&["STRING".into(), "text/html".into()]),
            Some("STRING")
        );
        assert_eq!(text_mime(&[MARKER.into()]), None);
        assert_eq!(text_mime(&[]), None);
    }

    #[test]
    fn skips_echoes_and_empty_and_huge() {
        assert!(!worth_sharing("same", "same"));
        assert!(!worth_sharing("", ""));
        assert!(!worth_sharing("", &"x".repeat(MAX + 1)));
        assert!(worth_sharing("", "copied"));
        assert!(worth_sharing("old", "new"));
    }
}
