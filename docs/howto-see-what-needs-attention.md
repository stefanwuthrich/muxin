# How to see what needs attention

You will print the fleet's problems, worst first, and know which machine or runner each line is about.

## Prerequisites

- Muxin 0.5.6 or the same commands from a build of this tree (`cargo build --release`, or `--no-default-features` if you only want the CLI).
- `muxin` on your `PATH`, or `./target/release/muxin`.
- At least one machine in `~/.config/muxin/config.toml`, or none, in which case the command tells you how to add one.

## Steps

1. Print the overview as JSON and read `attention` first.

   ```sh
   muxin status --json
   ```

   `attention` is the list of problems, most severe first. Each item names the `machine_id` or `runner_id` it is about. The rest of the payload is counts and the last check, not a live probe.

2. Look at `last_check_at`. If that time is older than you trust, check again. A check is read-only.

   ```sh
   muxin status --json --refresh
   ```

   The same probe, as a one-shot report:

   ```sh
   muxin check --json
   ```

3. Open the one machine the first attention item names.

   ```sh
   muxin machine mac-studio --json
   ```

   Use the id from step 1, not this example, unless that machine is yours. The payload has `status`, `unreachable` when it is offline, each runner's `state` and `source`, and `diagnostics` with a `hint`.

You can now see whether the problem is a machine that is off, a runner that is stopped, or a queued job with no online runner that carries its labels.

## Verification

`attention` is an array. An empty array means the last check found nothing to flag. A non-empty array has a `machine_id` or `runner_id` you can pass to `muxin machine` or `muxin runners`.

Runner `state` is `working`, `idle`, `offline`, or `unknown`. `source` says where that came from: `github` (live), `probe` (the last check), or `none`.

Machine `status` is `online`, `offline`, `unchecked`, `manual`, or `disconnected`.

## Troubleshooting

The process exits `0` on success, `1` when the operation failed, and `2` for bad input or a missing object (README, "The CLI"). On `--json`, the error is `{"error": {...}}` on stdout. The `code` is one of `not_found`, `invalid`, `busy`, `github`, `unreachable`, `operation_failed`, `internal`. If `hint` is set, run that command as written.

- `not_found`: the id is wrong, or it is a display name that is not unique. `muxin machines --json` lists the ids.
- No machines yet: `muxin add-machine --name <name> --host <ssh-host>`. Host is anything ssh accepts, or `local` for this computer. Adding writes `config.toml` and checks. It does not install a runner.
- `unreachable`: read `unreachable.kind` on the machine.

  | `kind` | What it is |
  |---|---|
  | `offline` | No answer. Shut down, asleep, or off the network. `last_seen_at` is the last time it answered. |
  | `refused` | The port refused the connection. |
  | `auth` | The key was rejected. |
  | `unknown-host` | The name did not resolve. |
  | `host-key` | The host key changed. |
  | `tailscale-off` | Tailscale on this computer is off or signed out. |
  | `powered-off` | A cloud machine that is powered off. |
  | `host-down` | The machine it runs on, or the jump host, is off. |
  | `stopped` | Its container is not running. |
  | `installing` | A VM Muxin just made, not answering yet. |
  | `error` | The check failed for another reason. Read `title`. |

- `github`: the `gh` login cannot see the org or repo. The `hint` is the refresh command. Do not invent a different scope.
- `disconnected`: you turned the machine off in Muxin. `muxin connect <id>` connects and checks it. Sessions that were already running there were not stopped.
- `busy`: that machine is already in the middle of a change. Wait, then retry the read. A status check itself does not take the lock.

Do not start, stop, or remove a runner from this page. Those commands change the machine and GitHub. [reference-surfaces.md](reference-surfaces.md) names them.

## Related

- [How a machine is reached](explanation-connections.md)
- [Surfaces](reference-surfaces.md)
- [Cloud machines](reference-cloud.md)
