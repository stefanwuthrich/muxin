#!/bin/sh
# cargo rustc-wrapper: patch cached gpui 0.2.2, then run rustc.
# The first argument is the rustc binary. See .cargo/config.toml.
set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
python3 "$root/scripts/patch-gpui-clipboard.py"
exec "$@"
