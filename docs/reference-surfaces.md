# Surfaces

Muxin 0.5.6 has three doors onto one API (`src/api.rs`): the window, the CLI, and the MCP server. They can run at the same time and they share `~/.config/muxin` (`config.toml`, `state.json`). `muxin paths` prints the directory. On Windows the directory is `%APPDATA%\muxin`.

This page lists the doors. Why there is one API is [explanation-architecture.md](explanation-architecture.md). How a machine is reached is [explanation-connections.md](explanation-connections.md). DigitalOcean fields are in [reference-cloud.md](reference-cloud.md).

## Window

The window is the default (`muxin` with no subcommand, or `muxin open`). It is compiled only with the `gui` feature, which is on by default. Routes are the `Route` enum in `src/ui/root.rs`:

| Route | What it is |
|---|---|
| `Overview` | Machines, GitHub, what needs attention |
| `Jobs` | Queued and running self-hosted jobs |
| `Workspaces`, `Workspace(id)` | Agent workspaces |
| `Github`, `GithubRepo`, `GithubItem` | Inbox, one repo, one issue or pull request |
| `Usage` | What Claude Code and Codex used |
| `Run` | One command on several machines |
| `Schedules` | Commands and sessions on a clock |
| `Activity` | Sessions |
| `Machine(id)`, `Runner(id)` | One machine, one runner |
| `Session(id)` | One agent session, id `machine/name` |
| `Desktop(id)` | That machine's screen |
| `Logs(id)`, `Services(id)` | Logs, services and containers |
| `Settings`, `About` | Settings, and how to update |

## CLI

Names are the kebab-case of the `Command` variants in `src/main.rs`. Every command accepts `--json` and `--demo`. On `--json`, stdout is JSON only and errors are `{"error":{...}}`. Exit `0` success, `1` the operation failed, `2` bad input or not found.

| Command | What it does |
|---|---|
| `muxin` / `muxin open` | The window |
| `muxin status [--refresh]` | Overview and `attention` |
| `muxin machines` | Machines, with runner counts |
| `muxin machine <id>` | One machine |
| `muxin add-machine --name <name> --host <ssh-host> [--no-check]` | Write it into `config.toml` and check it |
| `muxin remove-machine <id>` | Remove it from `config.toml`. The machine itself is unchanged |
| `muxin rename-machine <id> <new name>` | Display name only. The id stays |
| `muxin disconnect <id>` / `muxin connect <id>` | Stop reaching it, or reach it again and check |
| `muxin runners [--machine <id>] [--state offline]` | Runners |
| `muxin jobs` | Queued and running jobs |
| `muxin cancel-run <job-id>` | Cancels that job's whole workflow run |
| `muxin install <machine-id> [--scope] [--count N] [--mode auto\|service\|background\|docker]` | Install and register runners |
| `muxin runner <start\|stop\|restart\|logs\|remove> <runner-id> [--delete-files]` | Control one runner |
| `muxin watch` / `muxin unwatch` | GitHub orgs or `owner/name` repos |
| `muxin settings` | Show or change settings |
| `muxin ssh-hosts` | Aliases from `~/.ssh/config`, the tailnet, this network |
| `muxin files <machine>[:<folder>]` | List a folder. Alias `ls` |
| `muxin cp <machine>:<path>... <machine>:<folder>` | Copy between machines |
| `muxin workspace` | Subcommands below. Alias `ws` |
| `muxin github` | Subcommands below. Alias `gh` |
| `muxin checkouts [--machine] [--repo] [--git fetch\|pull\|push --path]` | Git checkouts on the machines |
| `muxin git status\|setup` | Whether git on a machine can commit and push, and setting that up |
| `muxin connection <machine> [--direct on\|off]` | How Muxin reaches it, and pairing for QUIC |
| `muxin desktop shot\|input\|displays\|add\|remove\|set` | Screenshot, input, monitors |
| `muxin logs <machine>` | System log, a file, a unit, a container |
| `muxin services <machine>` | Services and containers. Start, stop, restart, enable, disable |
| `muxin ports <machine>` | What is listening |
| `muxin usage [--days N] [--machine]` | Agent token use, priced at API list rates |
| `muxin edit <machine>:<folder>` | Open that folder in Zed, Cursor, or VS Code |
| `muxin ps <machine>` | Busiest processes and GPUs |
| `muxin kill <machine> <pid> [--force]` | Stop a process |
| `muxin run --on a,b -- <command>` | One command on each. `--all` for every machine online |
| `muxin wake <machine>` | Wake on LAN |
| `muxin power <machine> on\|off` | Cloud machine, or a VM's container |
| `muxin vm options\|create\|destroy` | Windows or Linux VM on one of your machines |
| `muxin schedule ls\|add\|rm\|run\|log` | A command or a session on a clock. Alias `schedules` |
| `muxin forward <machine> <port>` | That port at localhost here, until Ctrl-C |
| `muxin cloud options\|usage\|create\|destroy\|power\|login\|logout` | DigitalOcean. See [reference-cloud.md](reference-cloud.md) |
| `muxin session ls\|start\|read\|send\|do\|question\|answer\|attach\|restart\|rename\|stop` | Agent sessions. Alias `sessions` |
| `muxin mcp [--print-config] [--demo]` | MCP server on stdio. `--print-config` prints client snippets and exits |
| `muxin check` | Check every machine once |
| `muxin init` | Write a starter `config.toml` |
| `muxin paths` | Where this copy keeps its files |
| `muxin shell-command install\|remove\|status` | Put `muxin` on `PATH` |
| `muxin updates` | Whether a newer Muxin is out |

`muxin workspace` subcommands (`WorkspaceCommand`): `create`, `ls` (alias `list`), `show`, `exec`, `cat`, `write`, `files`, `changes`, `sync`, `shell`, `rm` (alias `destroy`), `allow`.

`muxin session start` agents, from the variant comment: `claude`, `codex`, `opencode`, `gemini`, `amp`, `pi`, or a shell. `session do` actions: `commit`, `commit_push`, `pull_request`, `review`, `fix_comments`, `conflicts`, `recap`.

`muxin github`: `inbox`, `repos`, `repo`, `accounts`, `use`, `favorite` (alias `fav`), `issue` (alias `pr`). An item is a URL or `owner/name#123`.

`muxin vm create` systems, from `VmCreateArgs`: `windows-11`, `windows-10`, `windows-server-2025`, `windows-server-2022`, `ubuntu-26.04`, `ubuntu-24.04`, `debian-13`, `fedora`, `alpine`, `arch`.

Runner id is `<machine-id>/<runner-name>`, or `github/<name>` when GitHub knows a runner no machine lists. A bare name works when it is unique. Machine id is a slug of the name (`mac-studio`). The display name is accepted when it is unique.

Runner `state`: `working`, `idle`, `offline`, `unknown`. `source`: `github`, `probe`, `none`.

Install `mode`: `auto` (service where it can, else Docker, else a background `run.sh`), `service`, `background`, `docker`. `--docker-socket` lets jobs talk to the host's Docker. That is root-equivalent on the host. The image for Docker mode is `ghcr.io/actions/actions-runner` (`RUNNER_IMAGE` in `src/ops/mod.rs`).

Long changes wait and return a log tail. `--no-wait` where a command has it returns an operation id. MCP uses `wait: false` and `get_operation`. One changing operation per machine. `busy` means wait.

## MCP

`muxin mcp` speaks MCP on stdio. The server name is `muxin`, version `CARGO_PKG_VERSION` (`src/mcp.rs`). `--print-config` prints ready-to-paste blocks for Claude Code, Codex, and a generic `mcpServers` JSON block. `--demo` serves simulated machines.

There are 89 tools. Each row is the `description` from `tools()` in `src/mcp.rs`, which is the text an agent sees.


| Tool | What it does |
|---|---|
| `status` | Overview of all the user's machines: counts of working/idle/offline runners, queued jobs, GitHub connection, every machine with CPU/memory/disk/uptime and a runner summary, and an `attention` list of problems (unreachable machines, stopped runners, jobs no runner can take, low disk). Call this first. Uses the last machine check unless refresh=true, which re-checks every machine over ssh first. |
| `get_machine` | Everything about one machine: resources, OS, each runner on it (state, where the state came from, labels, version, service, current job) and diagnostics with hints. Use it to explain why a machine or runner has a problem. |
| `list_runners` | List runners, optionally only those on one machine or in one state. Runners GitHub knows but no machine lists have machine_id null. |
| `list_jobs` | Queued and running self-hosted jobs from the watched GitHub orgs/repos, with labels, age, and for queued jobs how many online runners could take them (0 means the job is stuck until a matching runner comes online). |
| `check_machines` | Run the read-only machine check now (over ssh, or locally for host "local"): CPU, memory, disk, uptime, installed runners and whether they're running. One machine or all. Changes nothing on the machines. |
| `add_machine` | Add a machine to Muxin (written to config.toml, comments preserved) and check it. `host` is anything the system ssh accepts (user@host, an ~/.ssh/config alias; see list_ssh_hosts) or "local" for this computer. Key-based ssh must work without a password prompt. Doesn't install anything. |
| `remove_machine` | Remove a machine from config.toml. Nothing on the machine changes and its runners stay registered on GitHub; use runner_action remove first to take runners down. |
| `rename_machine` | Give a machine a new display name in config.toml (comments preserved). Its machine_id doesn't change, so its runners, sessions, workspaces and favorites stay with it. Nothing on the machine changes. |
| `connect_machine` | Disconnect from a machine (connected: false) or connect to it again. Disconnected, Muxin closes its connection and stops checking it and reading its sessions (status "disconnected"); its sessions keep running there, and anything that would reach it fails with "<name> is disconnected". Connecting checks it again. Saved in config.toml; nothing on the machine changes. Only when asked. |
| `install_runners` | Install GitHub Actions runners on a machine and register each with the GitHub org/repo (`scope`) using a registration token from the user's gh login, then add them to config.toml. How they run (`mode`): "auto" (default) picks a boot-time service when the machine allows it (macOS, Windows, Linux with passwordless sudo), else Docker containers when Docker is available, else a background run.sh; "docker" runs one container per runner from GitHub's actions-runner image (no sudo, restarted by Docker, jobs get a clean Linux userland). Modifies the real machine and GitHub. Needs admin on the org (admin:org scope) or repo. Names default to "<machine-id>-01", "-02", ... |
| `runner_action` | Control one runner on its machine: start, stop or restart its service; logs (read-only tail of its newest _diag log); or remove (stop, uninstall the service, deregister from GitHub, drop it from config.toml, optionally delete its folder). For a runner no machine lists ("github/<name>"), remove deregisters it on GitHub. stop and remove take the runner offline; confirm with the user first. |
| `cloud_options` | The user's DigitalOcean sign-in, regions, sizes (with hourly and monthly prices) and the defaults for a new cloud machine. Read-only. |
| `cloud_usage` | What the user's cloud machines cost: each droplet's hourly rate, monthly cap, cost this month and since it was created (destroyed ones this month too), the totals now / this month / by the month's end, and the DigitalOcean account's month-to-date bill when the sign-in may read it. Droplets are billed while they exist, powered off too; only destroying one stops the charges. Read-only. |
| `cloud_create` | Create a DigitalOcean droplet in the user's account, set it up (their user and SSH key, git, and the extras chosen: Docker, GitHub CLI, languages like Go, Rust, Python or PHP, agent CLIs, Tailscale), and add it to the user's machines. It COSTS MONEY every hour it exists: tell the user the size and price (cloud_options) and get their OK first. Takes a few minutes. |
| `cloud_destroy` | Destroy a cloud machine Muxin created: deletes the droplet (everything on it is lost) and removes it from Muxin. Confirm with the user first; make sure work there is pushed. |
| `cloud_power` | Power a cloud machine on or off. DigitalOcean still bills a droplet while it's off; only cloud_destroy stops the cost. |
| `machine_power` | Power a machine on or off. A cloud machine: through its provider (as cloud_power). A VM or container that runs in Docker on another of the user's machines (get_machine's `parent` names the machine and its `container`): starts or stops that container there; stopping gives it two minutes to shut down cleanly, and starting waits until it answers ssh. It changes the user's machines: only when asked. |
| `schedule_list` | Schedules on the user's machines (or one): commands or agent sessions that run there at set times, kept and run by Muxin's agent through the machine's own scheduler (cron, launchd, Task Scheduler), so they run whether or not Muxin is open. Each with `when` (and `when_text`, in the machine's own time), `what`, `enabled`, `running` and `last_run` (ok, exit_code, or the agent session it started). `problem` says when a machine can't run them (no cron). Read-only. |
| `schedule_save` | Create a schedule on a machine, or change one (pass its id). It runs `command` in the user's login shell in `folder`, or starts an agent session (`agent`: claude, codex, ...) with `prompt` in `folder`, which shows up in the app like any session. For an unattended agent run that ends by itself, use a command such as `claude -p "..."`. `when`: {every: "minutes"\|"hours", n} (hours at `at`'s minute), or {every: "day"\|"weekdays", at: "HH:MM"}, or {every: "week", days: ["mon", "thu"], at}; in the machine's own time. It runs on the user's machine unattended: only when asked. |
| `schedule_delete` | Delete a schedule from a machine, with its run history. Confirm with the user first unless they asked. |
| `schedule_run` | Run a schedule now, in the background on its machine (its next times don't change). Read the result with schedule_log. |
| `schedule_log` | A schedule's last runs (newest first: started, ok, exit_code, seconds, or the agent session) and one run's output: the newest, or the one whose `started` you pass. Read-only. |
| `vm_options` | Where VMs can run and what: each of the user's machines with Docker (online, kvm, cores, free memory and disk), the systems it can run (Windows needs a Linux x86-64 machine with KVM; Linux runs anywhere with Docker), why one can't, the system catalog (ids like windows-11, windows-10, windows-server-2025, ubuntu-24.04, debian-13, fedora, alpine, arch) and the defaults. Read-only. |
| `vm_create` | Create a VM on one of the user's machines and add it to them (shown under that machine). Windows (dockur/windows with KVM) downloads and installs itself, 15-30 minutes the first time: call with wait: false and poll get_operation. Linux is a container with ssh, ready in under a minute (it shares the host's kernel, no systemd). Muxin sets up ssh with this computer's key and its agent, so sessions, files, workspace_exec and the desktop work like on any machine. `host` defaults to the machine with the most free memory that can run it. The result names the new machine_id. It uses the user's machine and disk: only when asked, or when the task needs a fresh machine; destroy it when done unless they want to keep it. |
| `vm_destroy` | Destroy a VM Muxin made (vm_create): removes its container and disk on its host (everything in it is lost) and the machine from Muxin. Confirm with the user first unless they asked for it, or you made it for the task; copy out anything worth keeping. machine_power stops one without deleting it. |
| `cancel_run` | Cancel the GitHub Actions workflow run that a queued or running job belongs to (job_id from list_jobs). GitHub cancels whole runs, so every unfinished job in it stops. Confirm with the user first. |
| `get_operation` | Status and output of an install or runner action started by this server (use after wait=false or a timeout). Lines starting with "▸ " are Muxin's steps, "✓ " successes, "error: " problems. |
| `cancel_operation` | Stops a running operation that can be stopped (a copy_files copy). |
| `list_files` | What's in a folder on one of the user's machines (this computer included): names, file sizes, modified times; folders first. Linux and macOS machines. |
| `copy_files` | Copies files and folders from one of the user's machines to another (this computer included), through Muxin: the machines don't need to reach each other. Folders come along whole, with permissions. Files with the same names in the destination folder are replaced, so check it with list_files first and confirm with the user before overwriting. Waits by default and returns the operation(s) with a log; with wait=false poll get_operation, and cancel_operation stops a copy. |
| `list_operations` | Operations this server has run (installs, starts, stops, logs, removals), newest first, without their output. |
| `github_scopes` | Watch or stop watching GitHub orgs/repos. Watching gives live runner status and job data. Each added scope is checked first; reading org runners needs an org admin token (admin:org), and failures come back with the command that fixes them. |
| `get_settings` | Current settings: watched orgs/repos, polling and check intervals, file locations. |
| `update_settings` | Change polling and check intervals or the GitHub API URL (for GitHub Enterprise Server). Only the fields given change. |
| `list_ssh_hosts` | Hosts to suggest for add_machine: aliases from the user's ~/.ssh/config; computers on their Tailscale network (`tailnet`, when Tailscale is on here: each with the `host` to add it by, whether Tailscale sees it online, and `machine_id` when it's already in Muxin; a machine added by its tailnet name is reachable from anywhere); and computers on this local network that aren't in Muxin yet (`network`: `ssh: false` means a Mac with Remote Login off, which the user turns on in System Settings › General › Sharing). Listens on the network for a few seconds. |
| `workspace_create` | Check out one of the user's own machines as your work environment, like a remote dev box: creates a persistent folder there (optionally cloning a git repo at a ref, or creating a new branch) and returns a workspace_id for the other workspace_* tools. Omit machine_id to get the least-loaded online machine that allows workspaces, filtered by `platform` (e.g. "linux" to run tests on Linux). Use one workspace per task and reuse it across steps. Nothing is deleted automatically: call workspace_destroy when done. Private GitHub repos are cloned with the user's gh login. To work on a repository that's already checked out on a machine (local_checkouts), pass its folder as `worktree_of`: the workspace is then a git worktree of it on its own branch, quicker than a clone, and several agents can work on that repository side by side; workspace_destroy removes the worktree and keeps the branch. |
| `workspace_list` | All workspaces (yours and other agents'): machine, folder, repo/ref, owner, status, last command, git (branch, changes, pull request: a merged one means a worktree's work is done), and which machines can take new workspaces. Check here before creating a new one; reuse a ready workspace for the same task. |
| `workspace_get` | One workspace with its recent command history (command, exit code, duration, output tail) and the shell command a human can use to open a terminal in it. |
| `workspace_exec` | Run a shell command in a workspace on the user's machine, with that user's login environment (their PATH, Homebrew, nvm, ...). Returns exit_code, stdout and stderr (long output is capped, keeping the start and end). A non-zero exit_code is a normal result, not a tool error. Commands default to a 600s timeout (max 3600). For servers and watchers pass background=true: it returns right away with a pid and a log_path to read later. Commands run as the user on a real machine; ask before anything destructive outside the workspace. |
| `workspace_read_file` | Read a text file from a workspace (default up to 256 KB; binary files are reported, not returned). Also use it to read the log of a background command. |
| `workspace_write_file` | Write (or append to) a text file in a workspace, creating parent folders. Replaces the whole file unless append=true. |
| `workspace_list_files` | List files and folders in a workspace (skips .git internals), with sizes. |
| `workspace_changes` | Review what changed in a workspace: `git status` plus the uncommitted diff (capped). Use it before reporting back or syncing changes to the user's checkout. |
| `workspace_sync` | Copy code between a folder on this computer (usually the user's local checkout) and a workspace with rsync over ssh. direction=push sends the local folder to the workspace (e.g. to test uncommitted local changes on another OS); direction=pull brings the workspace's files back. delete=true also removes files on the receiving side that the sender doesn't have; pulling with delete into the user's checkout can remove their files, so confirm first. |
| `workspace_destroy` | Done with a workspace: stops background commands it started and deletes its folder on the machine (keep_files=true keeps the folder and only forgets the workspace). Uncommitted work in it is lost; pull or push it first if it matters. |
| `workspace_allow` | Turn agent workspaces on or off for a machine (edits config.toml, comments preserved). Machines allow them by default; only change this when the user asks. |
| `session_list` | Agent sessions on the user's machines: terminals running Claude Code, Codex, OpenCode, Gemini CLI, Amp, Pi or a shell, kept alive by Muxin's agent on the machine. Each has an id (<machine-id>/<name>), agent, title, folder, status (running/exited), busy (printed output in the last few seconds, i.e. the agent is working) and the command a human can paste to attach. Also lists, per machine, the agent CLIs installed. Reads the machines over ssh, so it's always current (and sets up Muxin's agent on a machine the first time). |
| `session_start` | Start an agent CLI (or a shell) in a new session on a machine, in a folder there, optionally with a first prompt. It keeps running until stopped; the user can watch and type into it from the Muxin app. Returns the session. Only start sessions the user asked for. |
| `session_read` | The session's screen plus recent scrollback as plain text, with its status and whether it's busy. Use it to see what an agent is doing or waiting for. |
| `session_send` | Type text into a session, then press Enter (unless enter=false). Answers an agent's question or gives it a new instruction. Only send what the user asked you to. |
| `session_action` | Ask a session's agent to do one of Muxin's session actions, by typing that action's prompt (the user may have changed it in Settings) into it: commit (commit its changes, don't push), commit_push, pull_request (push and open or update the branch's pull request), review (review the branch against the default branch, no code changes), fix_comments (address the pull request's review comments), conflicts (bring the branch up to date and resolve conflicts) or recap (summarize the session). session_list's `git` says which fit: a branch, a pull request. Only when the user asked for it. |
| `session_question` | What a session's agent is waiting for, read off its screen: a choice ("Do you want to proceed?" with its options, each with a key, and the one selected), or, when it's simply done, the end of what it said (options empty). Read-only. |
| `session_answer` | Pick one of a waiting session's choices by its key from session_question ("1", "2", or "#1" for a menu without numbers). It approves or refuses what the agent asked: only when the user asked you to answer, and say which option. To reply with text, use session_send. |
| `session_stop` | End a session: closes it and whatever runs in it. Unsaved agent state is lost. Ask the user first unless they asked for exactly this. |
| `session_restart` | Run a session's agent again in the same session (for one that exited, or is stuck). Kills what's running in it first. |
| `session_rename` | Give a session a new title (what the app's sidebar and session_list show). The agent on its machine keeps it, so it sticks for every Muxin. An empty title goes back to the one it started with. Doesn't touch what runs in it. |
| `github_inbox` | What needs the user on GitHub: pull requests waiting for their review, open issues and PRs assigned to them, and their own open pull requests (repo, number, title, labels, updated time, URL). Uses their GitHub login. |
| `github_repos` | The user's GitHub repositories (owned, collaborator, and through their orgs): their favorites first (`favorite: true`, the ones they work on most), then most recently pushed, with visibility, default branch, language and open issue count. |
| `github_accounts` | The GitHub accounts Muxin can use: the ones the GitHub CLI (gh) here is signed in to, plus each other machine's gh login (`machines`, e.g. a work laptop's work account). `using` (and `machine_id` when it's a machine's login) says which one Muxin uses for GitHub data, managing runners, and cloning into workspaces. |
| `github_use_account` | Switch which GitHub account Muxin uses: one the GitHub CLI here is signed in to (login), or another machine's gh login (machine_id). Changes whose repositories, inbox and runners Muxin sees and manages; doesn't change the account gh uses in the user's terminal. Only when asked. To add an account the user runs `gh auth login`. |
| `github_favorite` | Add a repository to the user's favorites in Muxin (listed first in the app and in github_repos), or remove it with favorite: false. Only when asked. |
| `github_repo` | One repository: its open issues and pull requests, recent GitHub Actions runs, and where it's checked out on the user's machines (machine, folder, branch, ahead/behind, uncommitted changes). Use the checkouts to pick where to work on it. |
| `github_item` | An issue or pull request with its full description and comments; for pull requests also branches, size, check results (with failing check names) and reviews. Pass its URL or owner/name#123. |
| `local_checkouts` | Git checkouts on the user's machines (found in their code folders), each with the GitHub repo it tracks, branch, upstream, commits ahead/behind, and count of uncommitted files. Filter by machine or repo. |
| `machine_connection` | How Muxin reaches a machine right now: directly over QUIC (once paired), over a live ssh link, or not at all; with a measured round trip to its agent. |
| `direct_connection` | Turn direct (QUIC) connections to a machine on or off. On: pairs with its agent over ssh (each side trusts only the other's certificate) and runs the agent's listener as a user service there (systemd or launchd), on UDP port 47821. Off: removes that service. Changes the machine; only when asked. |
| `desktop_screenshot` | A picture of a machine's screen (Linux with an X11 desktop, or a Mac on a direct connection), or of one of its displays. Coordinates for desktop_input are in its pixels. |
| `desktop_input` | Use a machine's desktop like a person: events run in order. {type: move\|click\|scroll\|key\|text\|wait}: click {x, y, button?: left\|right\|middle, double?}; move {x, y}; scroll {x, y, dy} (positive scrolls down); key {key: "enter" \| "ctrl+c" \| "alt+tab" ...}; text {text}; wait {ms}. Coordinates in desktop_screenshot pixels; take a screenshot after to see what happened. It acts on the user's real screen: only when asked. |
| `desktop_displays` | A machine's monitors: real ones and virtual ones Muxin added (virtual: true), each with an id to pass as `display`, its name, size and place on the desktop. On Linux, "all" is the whole desktop. |
| `desktop_add_display` | Add a virtual monitor to a machine's desktop, like plugging in another screen (no screen needed): windows can move there, and you can screenshot and use it on its own (pass its id as `display`). Linux (X11) and macOS. On Linux it takes root: when sudo there wants a password, this fails and the user adds the monitor from the Muxin app, which asks for it. Stays until removed or the machine restarts. Only when asked. |
| `desktop_remove_display` | Remove a virtual monitor Muxin added (desktop_displays shows them as virtual: true). Windows on it move to the other monitors. |
| `desktop_set_display` | Change a monitor's resolution (one of its `modes` from desktop_displays) or how much the desktop scales things up on it (`scales`; 2: twice as big and as sharp, like a Retina screen; on Linux X11 one scale applies to every monitor). A 4K monitor at scale 2 reads like 1080p but stays sharp. Changes the user's real desktop: only when asked. |
| `log_sources` | What a machine has in the way of logs: the system log (journal on Linux, unified log on a Mac; kind system, and kernel on Linux), log files (with readable: false for root-only ones), Docker containers and running services (systemd units; user: true for the user's own). Pass an entry's kind and target to logs_read. |
| `logs_read` | A log's last lines on a machine: the system log, the kernel's, a service's, a file's or a container's (see log_sources). filter keeps the lines holding it (any case). Read-only. |
| `machine_services` | A machine's services and Docker: systemd services on Linux (the system's and the user's own, user: true; at_boot says whether one starts at boot), Homebrew's on a Mac (brew services), containers (status, ports), each with cpu (percent of one core) and memory (bytes) when running; Docker images and Compose projects (with their files). Failed ones come first. |
| `service_action` | Start, stop or restart a service or container on a machine; enable or disable whether a systemd service starts at boot. The system's services need root: when sudo there wants a password this fails, and the user does it from the Muxin app (which asks). Stopping ssh, tailscaled or the network can cut the machine off. It changes the user's machine: only when asked. |
| `docker_run` | Run a Docker container on a machine (docker run -d), pulling the image first if it isn't there. Waits for it (an operation; its log_tail has Docker's output) unless wait is false. It changes the user's machine: only when asked. |
| `docker_pull` | Pull a Docker image onto a machine. Waits for it unless wait is false. |
| `machine_processes` | A machine's processes, the busiest few hundred: pid, user, name, command line, cpu (percent of one core), memory (bytes), how long it has run, GPU memory when NVIDIA says, and yours (owned by the user Muxin runs as); and its GPUs (NVIDIA, AMD on Linux, Apple): name, busy percent, memory used and total, temperature and power when known. Read-only. |
| `process_signal` | Stop (SIGTERM, the default) or kill (SIGKILL) a process on a machine, by pid from machine_processes. Another user's process needs root: when sudo wants a password it fails with a hint to use the Muxin app. It ends something on the user's machine: only when asked, and say which process first. |
| `run_on_machines` | Run one shell command on several machines at once: in each one's login shell (sh-family; PowerShell on Windows), from the home folder, without a terminal (nothing interactive). One operation per machine; waits for all unless wait is false, and returns each one's exit_code and log_tail. It runs on the user's real machines as the user: only when asked, and show the command first. |
| `wake_machine` | Wake a machine that's asleep or shut down (Wake on LAN), when its network card is known (Muxin learns it while the machine is on) and it's set up to wake that way. The signal goes from this computer and from online machines on the same network. Muxin checks it again after 20 and 40 seconds. |
| `agent_usage` | What the agent CLIs (Claude Code, Codex) used over the last N days (default 7), from their own logs on each machine: tokens (input, output, cache reads and writes) and requests by model and machine, and the cost at Anthropic's and OpenAI's list prices (null for a model whose price isn't known). With a Claude or ChatGPT plan the user doesn't pay per token. Read-only. |
| `machine_ports` | What's listening on a machine: each TCP port (port, address it listens on, host to connect to) with its program or Docker container, and for web servers (web: true) the status and title of their front page. Read-only. The user opens them on their computer from the Muxin app (machine page › Servers), or with `muxin forward <machine> <port>`. |
| `compose_project` | A Docker Compose project's parts on a machine (machine_services lists the projects): each service's image (or build folder), the ports it opens on the machine, what it waits for (depends_on with its condition), its networks, volumes and the machine's files it mounts, whether it has a health check, and its containers' state and health; the project's networks and volumes. Environment values aren't included, only how many. Read-only. |
| `compose_action` | Bring a Docker Compose project up (detached), down, restart it or pull its images, on a machine (machine_services lists the projects). down stops and removes its containers. With service: start (with what it waits for), stop or restart just that service. Waits unless wait is false. Only when asked. |
| `git_status` | Whether git on one of the user's machines can commit and push to GitHub: its name and email (can_commit), how a push gets a GitHub login (push: gh, other, none), the machine's GitHub CLI account, and the identity git_setup would use. |
| `git_setup` | Set up git on a machine so commits and pushes work (for agents working there too). identity: true sets git's global name and email to the user's GitHub name and private noreply address (or pass name and email). credentials: true runs `gh auth setup-git`, so git uses the machine's GitHub CLI login for github.com. Changes the machine's global git config; only when asked. |
| `git_sync` | Run git fetch, pull (fast-forward only) or push in a checkout on one of the user's machines. Changes their checkout or the remote; only when asked. |

## Ids, files, errors

| Thing | Shape |
|---|---|
| Machine id | Slug of the display name, such as `mac-studio` |
| Runner id | `<machine-id>/<runner-name>`, or `github/<name>` |
| Session id | `<machine-id>/<name>`, such as `beelink/claude-3f9a` |
| Workspace id | `ws-` plus a short id, such as `ws-7f3a2c` |
| Config | `~/.config/muxin/config.toml` |
| Last check | `~/.config/muxin/state.json` |
| Agent on a machine | `~/.muxen/bin` (`muxen-agent`) |

`ApiError.code` (`src/api.rs`): `not_found`, `invalid`, `busy`, `github`, `unreachable`, `operation_failed`, `internal`. `hint` is optional. `message` is the sentence.

GitHub data older than 30 seconds is refreshed before an answer (`GITHUB_MAX_AGE_SECS`). Default log tail is 80 lines. Log reads return 200. An install waits up to 900 seconds. A runner action waits up to 180. A check waits 60 seconds.

`unreachable.kind`: `offline`, `refused`, `auth`, `unknown-host`, `host-key`, `tailscale-off`, `powered-off`, `host-down`, `stopped`, `installing`, `error`. The how-to spells out each one: [howto-see-what-needs-attention.md](howto-see-what-needs-attention.md).

## Examples

```sh
muxin status --json
muxin mcp
```

The second process speaks JSON-RPC on stdin and does not return. The first prints the overview and exits.

A safe MCP setup, from `print_config` in `src/mcp.rs` (the path is this binary, not the literal word):

```sh
claude mcp add muxin -- muxin mcp
```

## Related

- [How to see what needs attention](howto-see-what-needs-attention.md)
- [Cloud machines](reference-cloud.md)
- [How a machine is reached](explanation-connections.md)
