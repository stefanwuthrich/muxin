# How to install runners on a machine

You will register GitHub Actions runners on a machine already in Muxin. This changes that machine and your GitHub account. A registration token comes from your `gh` login. Muxin does not ask you to paste one.

## Prerequisites

- The machine is in Muxin and answers. [How to add a machine](howto-add-a-machine.md).
- `gh` is logged in. Watching an org needs `admin:org`. A failure includes the `hint` command. Run that.
- A scope is watched (`muxin status --json`, field `github.watching`). If none is, pick the org or `owner/name` repo.

  ```sh
  muxin watch acme
  ```

## Steps

1. Install. `auto` is the default mode: a service where the machine allows it (macOS, Windows, Linux with passwordless sudo), otherwise Docker, otherwise a background `run.sh`.

   ```sh
   muxin install mac-studio --count 2
   ```

   Pass `--scope acme/widget` when more than one org or repo is watched. Names default to `<machine-id>-01`, `<machine-id>-02`.

2. Read the runners it created.

   ```sh
   muxin runners --machine mac-studio --json
   ```

   Each id looks like `mac-studio/mac-studio-01`. `state` is `working`, `idle`, `offline`, or `unknown`.

You can now see those runners on the machine's page and in `muxin jobs` when a job's labels match.

## Verification

`muxin runners --machine mac-studio --json` lists one entry per runner you asked for, with `declared: true`. A queued job whose labels they carry shows `matching_online` greater than 0 in `muxin jobs --json`.

## Troubleshooting

The install waits (up to 15 minutes, `INSTALL_TIMEOUT` in `src/ops/mod.rs`) and prints a log. Lines starting with `▸ ` are Muxin's steps, `✓ ` is success, `error: ` is the failure.

- `github`: the token cannot register runners. Run the `hint`. Do not invent a scope.
- `unreachable`: the machine did not answer. See [How to see what needs attention](howto-see-what-needs-attention.md).
- `busy`: that machine is already installing or restarting something. Wait, then retry.
- You want containers, not a service: `muxin install mac-studio --count 2 --mode docker`. `--docker-socket` lets jobs use the host Docker socket. That is root-equivalent. Pass it only when the jobs must run Docker and you trust them.
- Take one down: `muxin runner remove mac-studio/mac-studio-01`. That deregisters it on GitHub. Add `--delete-files` only when you want its folder gone.

## Related

- [Surfaces](reference-surfaces.md)
- [Cloud machines](reference-cloud.md) if the machine is a droplet you still have to create first
