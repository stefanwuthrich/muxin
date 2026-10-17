# Why Muxin is one program with three doors

Muxin 0.5.6 is the program on the computer you are sitting at. It reaches the other computers you own, and it reaches GitHub with the `gh` login you already have. The window, the terminal, and an AI agent are three ways into that same program. If they kept separate copies of "what is a machine", a runner could look idle in the window and offline in `muxin status --json`, and you would not know which one to trust.

The picture is [diagrams/architecture.html](diagrams/architecture.html). This page is why the boxes are arranged that way. The commands and tool names are in [reference-surfaces.md](reference-surfaces.md). How a machine is actually reached is in [explanation-connections.md](explanation-connections.md).

## The problem

A fleet app usually grows a UI first and bolts a script interface on later. The script then lags: a new page ships, and the agent still cannot see it. The other failure is the reverse. An agent API ships, and the window invents a second cache of machine state that goes stale on its own clock.

Muxin also has a second process, `muxen-agent`, that has to keep running after you close the window. Agent sessions (Claude Code, Codex, OpenCode, Gemini CLI, Amp, Pi, or a shell) are terminals. If they lived inside the window process, closing the app would kill them.

## The approach

```text
you
 |
 | window (GPUI)  |  CLI (clap, --json)  |  MCP (muxin mcp)
 v
src/api.rs                         one set of functions and JSON shapes
 |
 +-- fleet::Engine                 the snapshot: machines, runners, jobs
 +-- ops                           changes a machine or GitHub (install, start, stop)
 |
 | ssh first, then a live link or QUIC
 v
muxen-agent  (~/.muxen/bin)        sessions, desktop, files, services
 |
 +-- GitHub Actions runner         service, Docker, or a background run.sh
 |
 v
GitHub                             gh login: runners, jobs, issues, pulls
```

`src/main.rs` wires the modules: `api`, `cli`, `cloud`, `config`, `fleet`, `github`, `mcp`, `ops`, `probe`, `ui` (only with the `gui` feature), and `update`. The window is GPUI 0.2.2, the same toolkit as the Zed editor, behind the default `gui` feature. `cargo build --release --no-default-features` is the headless build: CLI and MCP, no window libraries.

`src/api.rs` is the contract. Its module comment says the responses are compact on purpose: stable ids, plain state strings, a one-line `summary`, operation logs capped to a tail. Errors carry a `code` (`not_found`, `invalid`, `busy`, `github`, `unreachable`, `operation_failed`, `internal`) and, when the fix is known, a `hint` you should pass on as written.

The window, `muxin mcp`, and `muxin <command> --json` can run at the same time. They share `~/.config/muxin` (`%APPDATA%\muxin` on Windows): `config.toml` for what you set, `state.json` for the last machine check. `muxin paths` prints the folder this copy uses.

## Trade-offs

One API means a new capability is not done until the window, the CLI, and MCP can all reach it. That is slower than shipping a page on its own. The gain is that an agent and the window cannot disagree about the shape of a machine.

The hub is the Muxin process on this computer, not a daemon you install here. Quit the app and the window is gone. Sessions are not. They stay in `muxen-agent` on the other machine, the way tmux keeps a shell. The cost is that the other machine must have that agent, and a machine that is off has nothing for Muxin to attach to.

GitHub is not a database Muxin owns. Live runner state and jobs come from the GitHub API through your `gh` login. When that token cannot see an org's runners, the error's `hint` is the command to run (often `gh auth refresh`). Muxin does not invent a second GitHub app.

Cloud machines are DigitalOcean droplets only. See [reference-cloud.md](reference-cloud.md). There is no second provider in `src/cloud`.

## Alternatives considered

A menu of cloud vendors was not built. `src/cloud/mod.rs` says droplets Muxin creates in the user's DigitalOcean account. The `provider` string stored on a machine is `"digitalocean"`. A search of the Rust sources for Cloudflare returns nothing. (`workers` in `src/ui/desktop.rs` and `agent/src/desktop.rs` is a thread pool for screen tiles, not Cloudflare Workers.)

Putting sessions inside the window was the other obvious design. `agent/src/main.rs` says the agent keeps sessions alive the way tmux does, so they outlive ssh and the app. That is why closing Muxin does not end a Claude Code session you started on another machine.

## In this working tree

These are facts about the checkout, not promises of a release.

- `Cargo.toml` names the repository `https://github.com/stefanwuthrich/muxin`. Version `0.5.6`, edition 2024. The workspace members are this crate and `agent` (`muxen-agent`).
- The Homebrew cask line in the README is still `brew install --cask lumarylabsllc/tap/muxin`. `scripts/homebrew.sh` still clones `LumaryLabsLLC/homebrew-tap`. The cask download URL points at `stefanwuthrich/muxin`.
- Wayland clipboard writes go through GPUI 0.2.2. On GNOME a click does not update the key-press serial GPUI was sending, so the compositor drops the write. This checkout patches that crate at compile time: `.cargo/config.toml` sets `rustc-wrapper` to `scripts/ensure-gpui-patch.sh`, which copies `patches/gpui-0.2.2/.../serial.rs` into the cargo registry before `rustc` runs. That is a build fix for this tree. It is not a setting in the app.

## Related

- [How a machine is reached](explanation-connections.md)
- [Surfaces: window, CLI, MCP](reference-surfaces.md)
- [Cloud machines](reference-cloud.md)
- [How to see what needs attention](howto-see-what-needs-attention.md)
