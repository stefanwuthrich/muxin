# How to add a machine

You will add a computer you can already ssh to, and Muxin will check it. Nothing is installed on it.

## Prerequisites

- Muxin on your `PATH`, or a build of this tree.
- Key-based ssh to that host, with no password prompt. `muxin ssh-hosts` lists aliases from `~/.ssh/config`, your Tailscale network, and this network.
- For this computer, the host is `local`. No ssh.

## Steps

1. Add it. The name is what you see. The id is the slug of that name.

   ```sh
   muxin add-machine --name "Mac Studio" --host ci@mac-studio.local
   ```

   Use your host. `user@host` and an ssh config alias both work. The command writes `config.toml` and runs the check.

2. Read the check.

   ```sh
   muxin machine mac-studio --json
   ```

   Use the id the first command printed, or the slug of the name.

You can now see that machine in `muxin status` and in the window. Runners are a separate step: [How to install runners on a machine](howto-install-runners.md).

## Verification

`muxin machines --json` includes the new id. `status` is `online` when the check answered. `unchecked` means you passed `--no-check`.

## Troubleshooting

Exit `2` is bad input or a name Muxin cannot use. Exit `1` is the check or the write failing. On `--json` the body is `{"error":{...}}`.

- The host never answers: `unreachable.kind` is `offline`, `refused`, `auth`, `unknown-host`, or `host-key`. The kinds are in [How to see what needs attention](howto-see-what-needs-attention.md). Fix ssh itself. Adding the machine again does not repair a key.
- Tailscale is off on this computer and the machine is only on the tailnet: `kind` is `tailscale-off`. Turn Tailscale on. Muxin does not do that.
- You only wanted it in the file, not checked yet: `muxin add-machine --name "Mac Studio" --host ci@mac-studio.local --no-check`.

`muxin remove-machine <id>` takes it out of `config.toml`. The computer and any GitHub runners stay as they are.

## Related

- [How a machine is reached](explanation-connections.md)
- [Surfaces](reference-surfaces.md)
