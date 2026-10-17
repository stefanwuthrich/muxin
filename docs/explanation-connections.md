# How Muxin reaches a machine

A machine in Muxin is a computer you can already ssh to, or this computer when its host is `local`. The first check is just ssh. After that, a small program on the machine (`muxen-agent`) keeps sessions, the desktop, and the file listing alive so Muxin does not open a new ssh process for every click.

The picture of who talks to whom is [diagrams/architecture.html](diagrams/architecture.html). Why there is one API is [explanation-architecture.md](explanation-architecture.md). The fields on a connection are listed with the other surfaces in [reference-surfaces.md](reference-surfaces.md).

## The problem

ssh is a good way to get onto a machine and a bad way to hold a terminal open. Each call is a new process. A Claude Code session started that way dies when the process dies, which is every time you close the window or the network blips. A desktop picture has the same problem: you want a stream, not a fresh login per frame.

The other failure is a machine that is off. Retrying ssh at it forever looks like a bug in the app. The useful answer is "it is off, and here is when it was last seen."

## The approach

`src/fleet/connection.rs` names three ways Muxin is reaching a machine right now:

| `via` | What it means |
|---|---|
| `direct` | QUIC to `muxen-agent listen`, after the two sides have paired certificates |
| `ssh` | One live ssh connection, with `muxen-agent link` multiplexed on its stdin and stdout |
| `none` | No link up |

The order is fixed. Muxin does not start with QUIC.

1. You add the machine with a host ssh already accepts (`user@host`, an alias from `~/.ssh/config`, or `local`). Key-based ssh has to work without a password prompt. Nothing is installed by adding it.
2. The check runs over that ssh (or locally, for `local`): CPU, memory, disk, uptime, and which runners are there.
3. Muxin installs `muxen-agent` under `~/.muxen/bin` on that machine. `agent/src/main.rs` lists what it answers: `version`, `hash`, `ctl`, `link`, `listen`, `attach`, `serve`, plus desktop, clipboard, and schedule commands.
4. Direct mode is optional. `direct_connection` (CLI: `muxin connection <machine> --direct on`) pairs over ssh, then runs the listener as a user service (systemd or launchd) on UDP port 47821. Off removes that service.

If direct was asked for and UDP 47821 does not connect, the code stays on ssh. The note in `connection.rs` is "Direct didn't connect (UDP 47821 blocked?), so it's ssh." A VM that has no forwarded UDP port gets the same treatment: ssh, with a note that VMs Muxin makes now get a port.

Tailscale is how a machine stays reachable when it is not on the local network. Muxin uses the local network when the machine is on it, and Tailscale otherwise. It does not change Tailscale's settings. If Tailscale is off on this computer and the machine is only on the tailnet, `unreachable.kind` is `tailscale-off`.

A machine can sit on another of your machines. `get_machine`'s `parent` says so, with `relation` `vm`, `container`, `inside`, or `through` (ssh only goes through it, a jump host). When the parent is off, `unreachable.kind` is `host-down`. When the container is not running, it is `stopped`.

Disconnect is a choice, stored in `config.toml`. Status becomes `disconnected`. Muxin does not check it and does not read its sessions. The sessions keep running there. Anything that would reach it fails with "<name> is disconnected. Connect to it first."

## Trade-offs

ssh first means your keys, your agent, and your `~/.ssh/config` aliases work on day one. The cost is that the first path is only as healthy as ssh. A host key change, a refused port, or an auth failure is reported as that kind, not as a generic timeout.

QUIC is faster and does not hold an ssh process open, and it needs a hole for UDP 47821 plus a pairing step. Until you turn it on, you pay for ssh. Muxin will not switch you over by itself.

Sessions living in `muxen-agent` means closing the app is safe. It also means a machine that is asleep has no session to attach to until it wakes. `muxin wake <machine>` sends Wake on LAN when Muxin has seen the network card and the machine is set to wake that way.

## Alternatives considered

Opening a new ssh for every request was the starting point (the probe still does that for a check). Holding sessions that way lost them. `muxen-agent serve` is the background server, started on demand, that `attach` connects a terminal to.

A VPN Muxin configures for you was not built. Tailscale is detected and preferred when the LAN path is gone. Turning Tailscale on is left to you.

## Related

- [How to add a machine](howto-add-a-machine.md)
- [How to see what needs attention](howto-see-what-needs-attention.md)
- [Architecture](explanation-architecture.md)
- [Surfaces](reference-surfaces.md)
