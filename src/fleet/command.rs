//! Running one command on several machines at once (the Run page, `muxin
//! run`): an operation per machine, in the user's shell from their home
//! folder, its output streamed into the operation's log. Aliases and
//! functions from `.zshrc` or `.bashrc` apply, as they do in a terminal.

use std::sync::Arc;
use std::time::Duration;

use super::Engine;
use super::workspaces::transport::{self, Limits};
use crate::ops::remote::{self, sh_quote};
use crate::probe::Shell;

/// How long a command may run unless asked otherwise.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// The longest it may run.
pub const MAX_TIMEOUT: Duration = Duration::from_secs(60 * 60);

/// The command, from the home folder, in the user's own shell.
///
/// The script itself runs in a login shell, so the profile's PATH is
/// exported. zsh and bash then run the command interactively: that is what
/// reads `.zshrc` / `.bashrc` and expands aliases. A prompt theme started
/// with no terminal is not part of the command's output.
fn sh_script(command: &str) -> String {
    format!(
        r#"OR_CMD={cmd}
cd ~ 2>/dev/null
export NO_COLOR=1 GIT_TERMINAL_PROMPT=0 GIT_PAGER=cat PAGER=cat
or_shell=sh
case "$SHELL" in
  */bash|*/zsh|*/ksh) or_shell=$SHELL ;;
esac
or_status=0
case "$or_shell" in
  */zsh)
    or_dir=$(mktemp -d 2>/dev/null) || or_dir=
    if [ -n "$or_dir" ]; then
      printf '%s\n' \
        'exec 3>&1 4>&2' \
        'exec >/dev/null 2>&1' \
        '[ -f "$HOME/.zshenv" ] && . "$HOME/.zshenv"' \
        > "$or_dir/.zshenv"
      printf '%s\n' \
        '[ -f "$HOME/.zshrc" ] && . "$HOME/.zshrc"' \
        'exec >&3 2>&4' \
        'exec 3>&- 4>&-' \
        > "$or_dir/.zshrc"
      ZDOTDIR="$or_dir" "$or_shell" -ic "$OR_CMD" < /dev/null
      or_status=$?
      rm -rf "$or_dir"
    else
      "$or_shell" -ic "$OR_CMD" < /dev/null
      or_status=$?
    fi
    ;;
  */bash)
    or_rc=$(mktemp 2>/dev/null) || or_rc=
    or_fifo=$(mktemp 2>/dev/null) || or_fifo=
    if [ -n "$or_rc" ] && [ -n "$or_fifo" ]; then
      rm -f "$or_fifo"
      printf '%s\n' \
        'exec 3>&1 4>&2' \
        'exec >/dev/null 2>&1' \
        '[ -f "$HOME/.bashrc" ] && . "$HOME/.bashrc"' \
        'exec >&3 2>&4' \
        'exec 3>&- 4>&-' \
        > "$or_rc"
      if mkfifo "$or_fifo" 2>/dev/null; then
        while IFS= read -r or_line; do
          case "$or_line" in
            "bash: cannot set terminal process group "*|"bash: no job control in this shell") ;;
            *) printf '%s\n' "$or_line" >&2 ;;
          esac
        done < "$or_fifo" &
        or_reader=$!
        "$or_shell" --rcfile "$or_rc" -ic "$OR_CMD" < /dev/null 2>"$or_fifo"
        or_status=$?
        wait "$or_reader"
      else
        "$or_shell" --rcfile "$or_rc" -ic "$OR_CMD" < /dev/null
        or_status=$?
      fi
      rm -f "$or_rc" "$or_fifo"
    else
      "$or_shell" -ic "$OR_CMD" < /dev/null
      or_status=$?
    fi
    ;;
  *)
    "$or_shell" -ic "$OR_CMD" < /dev/null
    or_status=$?
    ;;
esac
printf '%s:exit=%s\n' "$OR_MARK" "$or_status" >&2
"#,
        cmd = sh_quote(command)
    )
}

/// PowerShell, for Windows machines: the command in a script file of its
/// own, run by its own PowerShell from the home folder, as in a terminal (its
/// output formatted as text, its `exit` its exit status).
fn ps_script(command: &str) -> String {
    format!(
        "$OR_CMD = '{}'\n\
         $f = Join-Path ([IO.Path]::GetTempPath()) (\"muxen-run-$PID.ps1\")\n\
         Set-Content -LiteralPath $f -Value $OR_CMD -Encoding UTF8\n\
         Set-Location $env:USERPROFILE\n\
         & powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $f\n\
         $c = $LASTEXITCODE\n\
         Remove-Item -LiteralPath $f -Force -ErrorAction SilentlyContinue\n\
         [Console]::Error.WriteLine(\"${{OR_MARK}}:exit=$c\")\n",
        command.replace('\'', "''")
    )
}

impl Engine {
    /// Runs `command` on a machine as an operation (kind "command"); its id.
    pub fn run_command(
        self: &Arc<Self>,
        machine_id: &str,
        command: &str,
        timeout: Duration,
    ) -> Result<u64, String> {
        let command = command.trim();
        if command.is_empty() {
            return Err("Which command?".into());
        }
        let (spec, probe) = self
            .machine(machine_id)
            .ok_or_else(|| format!("No machine called {machine_id}."))?;
        if self.demo_world().is_some() {
            return Ok(self.demo_command(
                &spec.id,
                &spec.name,
                command,
                probe
                    .as_ref()
                    .and_then(|p| p.os.clone())
                    .unwrap_or_default(),
            ));
        }
        spec.refuse_if_disconnected()?;
        let windows = remote::shell_for(&spec, probe.as_ref()) == Shell::Powershell;
        let timeout = timeout.clamp(Duration::from_secs(1), MAX_TIMEOUT);
        let short: String = command
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(60)
            .collect();
        let op = self.begin_op(
            "command",
            format!("{short} on {}", spec.name),
            Some(spec.id.clone()),
            None,
        );
        let script = if windows {
            ps_script(command)
        } else {
            sh_script(command)
        };
        let me = self.clone();
        self.runtime().spawn(async move {
            let marker = transport::new_marker();
            let log = me.clone();
            let on_line = move |l: &str| log.op_log(op, l.to_string());
            let out = transport::run(
                &spec,
                windows,
                &marker,
                transport::with_marker(&marker, windows, &script),
                timeout,
                Limits::OUTPUT,
                &[],
                Some(&on_line),
            )
            .await;
            let result = match out {
                Err(e) => Err(e),
                Ok(o) if o.timed_out => Err(format!(
                    "Still running after {}s: stopped waiting.",
                    timeout.as_secs()
                )),
                Ok(o) if !o.started => Err(format!("{} is unreachable: {}", spec.name, o.reason())),
                Ok(o) => match o.exit {
                    Some(0) => Ok(()),
                    Some(c) => Err(format!("Exited with status {c}.")),
                    None => Err(o.reason()),
                },
            };
            if let Err(e) = &result {
                me.op_log(op, format!("error: {e}"));
            }
            me.op_finish(op, result);
        });
        Ok(op)
    }

    /// The demo's run: made-up output after a moment.
    fn demo_command(self: &Arc<Self>, id: &str, name: &str, command: &str, os: String) -> u64 {
        let op = self.begin_op(
            "command",
            format!("{command} on {name}"),
            Some(id.to_string()),
            None,
        );
        let mac = os.starts_with("darwin") || os.contains("mac");
        let lines: Vec<String> = if command.starts_with("df") {
            let (dev, size, used, free, pct) = if mac {
                ("/dev/disk3s1s1", "926Gi", "512Gi", "382Gi", "58%")
            } else {
                ("/dev/nvme0n1p2", "1.8T", "412G", "1.3T", "24%")
            };
            vec![
                "Filesystem       Size   Used  Avail Capacity  Mounted on".into(),
                format!("{dev:<16} {size:>5}  {used:>5}  {free:>5}    {pct:>4}   /"),
            ]
        } else if command.starts_with("uptime") {
            vec![format!(
                " 13:45  up {} days,  4:02, 2 users, load averages: 0.42 0.38 0.31",
                id.len() % 9 + 2
            )]
        } else if command.starts_with("ls") {
            [
                "Applications",
                "Desktop",
                "Documents",
                "Downloads",
                "Library",
                "Movies",
                "Music",
                "Pictures",
                "Public",
                "code",
                "compose",
                "dotfiles",
                "go",
                "models",
                "notes",
                "runners",
                "scratch",
                "snap",
                "Templates",
                "Videos",
                "work",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect()
        } else if command.starts_with("uname") {
            vec![if mac {
                "Darwin 25.1.0 arm64".into()
            } else {
                "Linux 6.17.0-8-generic x86_64".into()
            }]
        } else {
            vec!["done".into()]
        };
        let me = self.clone();
        let wait = Duration::from_millis(400 + (id.len() as u64 % 5) * 250);
        self.runtime().spawn(async move {
            tokio::time::sleep(wait).await;
            for l in lines {
                me.op_log(op, l);
            }
            me.op_finish(op, Ok(()));
        });
        op
    }

    /// Runs `command` on each machine; each one's operation id, or why it didn't start.
    pub fn run_on_machines(
        self: &Arc<Self>,
        machine_ids: &[String],
        command: &str,
        timeout: Duration,
    ) -> Vec<(String, Result<u64, String>)> {
        machine_ids
            .iter()
            .map(|id| (id.clone(), self.run_command(id, command, timeout)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_the_command() {
        let s = sh_script("echo 'hi' && uptime");
        assert!(
            s.starts_with("OR_CMD='echo '\\''hi'\\'' && uptime'\n"),
            "{s}"
        );
        assert!(
            s.contains("ZDOTDIR=\"$or_dir\" \"$or_shell\" -ic \"$OR_CMD\""),
            "{s}"
        );
        assert!(s.contains(".zshrc"), "{s}");
        assert!(s.contains("--rcfile \"$or_rc\" -ic \"$OR_CMD\""), "{s}");
        let mut n = std::process::Command::new("sh")
            .arg("-n")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("sh");
        std::io::Write::write_all(n.stdin.as_mut().unwrap(), s.as_bytes()).unwrap();
        let out = n.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "sh -n: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(ps_script("Write-Output 'hi'").starts_with("$OR_CMD = 'Write-Output ''hi'''\n"));
    }
}
