# Cloud machines

Muxin can create a DigitalOcean droplet in the account you sign in, set it up, and add it to your machines. That is the only cloud provider in this tree. `src/cloud/mod.rs` says so in the module comment, and the `provider` string written into `config.toml` is `digitalocean` (`src/config.rs`).

A search of `*.rs` for `cloudflare` or `Cloudflare` returns no matches. The word `workers` in `src/ui/desktop.rs` and `agent/src/desktop.rs` is a thread count for encoding screen tiles.

Creating one costs money every hour it exists. Powering it off does not stop the bill. Only destroying it does. `cloud_power`'s description in `src/mcp.rs` says that in those words.

## API / Interface

CLI (`src/main.rs`, `CloudCommand`):

| Command | Effect |
|---|---|
| `muxin cloud options` | Account, regions, sizes, defaults. Read-only. |
| `muxin cloud usage` | Hourly rate, monthly cap, cost this month and since created, totals, and the account bill when the sign-in can read it. |
| `muxin cloud create <name> [--region SLUG] [--size SLUG] [--install LIST] [--no-wait]` | Creates the droplet, sets it up, adds the machine. |
| `muxin cloud power <machine> on\|off` | Powers it. Still billed while off. |
| `muxin cloud destroy <machine>` | Deletes the droplet and removes the machine from Muxin. |
| `muxin cloud login --token TOKEN` | API token. |
| `muxin cloud login --doctl` | Use the `doctl` login already on this computer. |
| `muxin cloud login --browser` | Browser sign-in, when this copy of Muxin has it (`oauth_available`). |
| `muxin cloud logout` | Forgets the sign-in. |

MCP tools, same work: `cloud_options`, `cloud_usage`, `cloud_create`, `cloud_power`, `cloud_destroy`. `machine_power` also powers a cloud machine, and it powers a VM container. See [reference-surfaces.md](reference-surfaces.md).

`CloudOptions.method` is one of `token`, `oauth`, `doctl` (`src/cloud/mod.rs`). `signed_in` says whether a call can be made. Regions and sizes come from the account. `default_region` and `default_size` come from settings when you omit them.

`CloudCreate` (`src/cloud/mod.rs`):

| Field | Type | Default | Constraint |
|---|---|---|---|
| `name` | string | required | The machine's name in Muxin. |
| `region` | string or omitted | Settings, else the nearest | A region slug such as `nyc3`. |
| `size` | string or omitted | Settings | A size slug such as `s-2vcpu-4gb`. |
| `install` | list of strings or omitted | Settings | Extra ids. `""` on the CLI means nothing. |

Unknown JSON fields are rejected (`deny_unknown_fields`).

Sizes Muxin offers are basic droplets on regular CPUs. `regular_size` keeps slugs that start with `s-` and drops the ones that end in `-amd` or `-intel`. `offered` is those sizes, in a region, cheapest first, in the order DigitalOcean lists them. `cities` picks one region per city (the one with the most offered sizes), in a fixed city order starting New York, Toronto, Atlanta, Richmond, San Francisco.

`install` ids, from the `CloudCreateArgs` comment in `src/main.rs`: `docker`, `gh`, `node`, `python`, `go`, `rust`, `php`, `ruby`, `java`, `bun`, `deno`, `dotnet`, `claude`, `codex`, `gemini`, `opencode`, `amp`, `pi`, `tailscale`. The droplet always gets your user and SSH key. The public key Muxin offers is `~/.ssh/id_ed25519.pub` (`CloudOptions.ssh_key`).

A Tailscale auth key for new machines is stored under the secret name `tailscale-authkey` (`TAILSCALE_KEY` in `src/cloud/mod.rs`).

The record in `config.toml` looks like this (the comment in `src/config.rs`):

```toml
cloud = { provider = "digitalocean", id = 123, region = "nyc3", size = "s-2vcpu-4gb", price_hourly = 0.03571, created_at = "..." }
```

`get_machine`'s `cloud` field is that record plus cost so far and how long the machine has been idle.

`muxin power <machine> on|off` is the same switch for a cloud machine and for a VM Muxin made (the container on the host machine). Stopping a VM waits up to two minutes so it can shut down cleanly.

## Examples

Read the menu before creating anything. This does not spend money.

```sh
muxin cloud options --json
muxin cloud usage --json
```

Create is the call that starts billing. The MCP description says to tell the user the size and price from `cloud_options` and get an OK first. The CLI is the same operation:

```sh
muxin cloud create build-box --region nyc3 --size s-2vcpu-4gb --install docker,gh
```

`--no-wait` returns an operation id. Poll `get_operation` (MCP) until `status` is no longer `running`. The log tail uses `▸ ` for Muxin's steps, `✓ ` for success, and `error: ` for problems (`src/ops/mod.rs`).

## Related

- [How Muxin reaches a machine](explanation-connections.md) covers `powered-off`, which is the unreachable kind for a cloud machine that is off.
- [Surfaces](reference-surfaces.md) lists `vm_create` next to this. A VM runs on a machine you already have. A droplet runs in DigitalOcean's account.
- [How to see what needs attention](howto-see-what-needs-attention.md)
