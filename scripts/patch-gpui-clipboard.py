#!/usr/bin/env python3
"""Patch the cached gpui 0.2.2 so Wayland clipboard writes use the click serial.

GNOME drops wl_data_device.set_selection when the serial is the last key press
and the user copied with the mouse (that serial is 0). Connect an agent, and
every other Copy button, goes through that call.

Idempotent. Cargo runs this from scripts/ensure-gpui-patch.sh before rustc.
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

OLD_SERIAL = "state.serial_tracker.get(SerialKind::KeyPress)"
NEW_SERIAL = "state.serial_tracker.latest_of(&[SerialKind::KeyPress, SerialKind::MousePress])"
MARKER = "fn latest_of"


def registry_roots() -> list[Path]:
    cargo_home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
    src = cargo_home / "registry" / "src"
    if not src.is_dir():
        return []
    return [p for p in src.iterdir() if p.is_dir()]


def main() -> int:
    repo = Path(__file__).resolve().parent.parent
    replacement = repo / "patches" / "gpui-0.2.2" / "src" / "platform" / "linux" / "wayland" / "serial.rs"
    if not replacement.is_file():
        print(f"gpui clipboard patch: missing {replacement}", file=sys.stderr)
        return 0
    new_serial = replacement.read_text()
    patched = 0
    for root in registry_roots():
        for crate in root.glob("gpui-0.2.2"):
            wayland = crate / "src" / "platform" / "linux" / "wayland"
            serial_rs = wayland / "serial.rs"
            client_rs = wayland / "client.rs"
            if not serial_rs.is_file() or not client_rs.is_file():
                continue
            serial_text = serial_rs.read_text()
            if MARKER not in serial_text:
                serial_rs.write_text(new_serial)
                patched += 1
            client_text = client_rs.read_text()
            if OLD_SERIAL in client_text:
                client_rs.write_text(client_text.replace(OLD_SERIAL, NEW_SERIAL))
                patched += 1
    if patched:
        print(f"gpui clipboard patch: updated {patched} file(s)", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
