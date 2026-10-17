use collections::HashMap;

#[derive(Debug, Hash, PartialEq, Eq)]
pub(crate) enum SerialKind {
    DataDevice,
    InputMethod,
    MouseEnter,
    MousePress,
    KeyPress,
}

#[derive(Debug)]
struct SerialData {
    serial: u32,
    /// Order this kind was last updated. Wayland serial numbers are not one
    /// sequence: an input-method serial can be numerically larger than a
    /// pointer serial and still be the wrong token for `set_selection`.
    seen: u64,
}

impl SerialData {
    fn new(value: u32, seen: u64) -> Self {
        Self { serial: value, seen }
    }
}

#[derive(Debug)]
/// Helper for tracking of different serial kinds.
pub(crate) struct SerialTracker {
    serials: HashMap<SerialKind, SerialData>,
    next_seen: u64,
}

impl SerialTracker {
    pub fn new() -> Self {
        Self { serials: HashMap::default(), next_seen: 0 }
    }

    pub fn update(&mut self, kind: SerialKind, value: u32) {
        self.next_seen += 1;
        self.serials.insert(kind, SerialData::new(value, self.next_seen));
    }

    /// Returns the latest tracked serial of the provided [`SerialKind`]
    ///
    /// Will return 0 if not tracked.
    pub fn get(&self, kind: SerialKind) -> u32 {
        self.serials.get(&kind).map(|serial_data| serial_data.serial).unwrap_or(0)
    }

    /// The serial to pass to `wl_data_device.set_selection`.
    ///
    /// GNOME (mutter) ignores a selection whose serial is not from a real key
    /// or pointer event this client just received. Copying from a mouse click
    /// has no key-press serial — that value is 0 until the window has seen a
    /// key — so the clipboard write is dropped and nothing is pasted elsewhere.
    /// Use whichever of `kinds` was updated most recently, not the largest
    /// number and not a serial from another protocol.
    pub fn latest_of(&self, kinds: &[SerialKind]) -> u32 {
        kinds.iter().filter_map(|kind| self.serials.get(kind)).max_by_key(|data| data.seen).map(|data| data.serial).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_of_prefers_the_most_recent_input() {
        let mut tracker = SerialTracker::new();
        let kinds = [SerialKind::KeyPress, SerialKind::MousePress];
        assert_eq!(tracker.latest_of(&kinds), 0);
        tracker.update(SerialKind::KeyPress, 10);
        tracker.update(SerialKind::MousePress, 4);
        assert_eq!(tracker.latest_of(&kinds), 4, "a click is newer than an earlier key, even if its serial is smaller");
        tracker.update(SerialKind::InputMethod, 99);
        assert_eq!(tracker.latest_of(&kinds), 4, "an input-method serial is not a clipboard token");
        tracker.update(SerialKind::KeyPress, 12);
        assert_eq!(tracker.latest_of(&kinds), 12);
    }
}
