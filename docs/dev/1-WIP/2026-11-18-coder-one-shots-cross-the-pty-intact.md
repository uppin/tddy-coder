# A Coder one-shot's stdin and stdout cross the workspace's pty intact

**Type:** Fix
**Packages:** `tddy-remote-shell`, `tddy-tool-engine`

## Problem

Every Claude start on the Coder workspace `mantasi/remote-dev` failed at the agent step with
`coder target mantasi/remote-dev: no answer within 60s; the command was stopped`. The cause was
confirmed on the VM: argv, the remote fds, `wchan`, and the bytes of the file. The evidence is in the
diagnostics session `01a1ecfc-0fac-77a0-bb0d-ac20a90593ae`, under `tmp/coder-agent-start-diagnostics.md`
in its worktree. The orchestrator that found it was session `01a1ecee-e942-7982-abfc-104c6c4baa98`.

- `RemoteShell::write_file` pipes the file into `cat > <path>` on the command's stdin. `coder ssh`
  runs the command in a **pty**, and closing the CLI's stdin never reaches the workspace as an EOF.
- The settings file's last line, `}`, has no newline, so the tty's canonical mode held it back.
  `cat` never returned and left 1816 bytes of invalid JSON behind. The start then hit its 60 s
  deadline, and the remote `sh`/`cat` pair stayed alive after the local `coder ssh` was killed.
- A run against the real workspace showed the return direction is broken too. The pty's `onlcr`
  setting turns every `\n` the command writes into `\r\n`, so `read_file` and any command's stdout
  came back altered.

## Change

`CoderShell::exec` (`tddy-remote-shell/src/coder.rs`) wraps every one-shot so the command never
relies on the pty:

- **Output processing off:** `stty -opost` runs first, so the command's bytes arrive as written.
- **No stdin:** the shell's stdin is `/dev/null`, so a command that reads stdin gets an empty one
  instead of blocking on the pty.
- **Stdin given:**
  - The bytes are sent as **base64 lines of 76 characters**, each ending in `\n`. Base64 holds no
    control character a tty acts on (`^D`, `^C`, `\r`), and every line is far inside `MAX_CANON`.
  - The workspace reads exactly that many bytes with `head -c`, so it needs no EOF. It decodes them
    into a `mktemp` file and checks that the file holds the original length.
  - It then makes that file the shell's stdin and unlinks it before the command runs. A stdin that
    did not arrive whole runs nothing and exits 125, saying so.
  - Echo is turned off before the start marker is printed. The lines are written only once the
    marker has arrived, so none is echoed into the command's output.
  - The bytes still travel on stdin, never on a command line. The session pointer written this way
    carries the hook token, and argv is readable by other processes.

`StartSession`'s `new_branch_name` description (`tddy-tool-engine`) no longer promises a generated
name for every agent. An agent whose spec declares `branch_name_required` (the built-in
`claude-cli`) refuses an empty name. An empty `branch_worktree_intent` generates one for every agent.
The refusal itself is by design and stays as it is.

`base64 = "0.22"`, the version six workspace crates already use, is added to `tddy-remote-shell`.

## Tests

- `tddy-remote-shell/tests/coder_shell_acceptance.rs` gets two new stand-ins in
  `tests/support/coder.rs`:
  - `a_coder_whose_workspaces_never_see_the_end_of_their_input`: the command's stdin is a FIFO it
    holds open, so input never ends, and stderr is merged into stdout;
  - `a_coder_whose_workspaces_end_their_lines_with_carriage_returns`: `onlcr` stays on until a
    stand-in `stty -opost` turns it off.
- Five tests pin the behaviour:
  - a settings file without a trailing newline is written whole, and its token is on no command line;
  - control bytes reach `cat` exactly;
  - a command given no stdin reads an empty one;
  - a file reads back without added `\r`;
  - printed lines end as printed.
- The first three hung against the previous `coder.rs` and the last two failed on the added `\r`.

**Real-deployment run (2026-11-18, `mantasi/remote-dev`, coder v2.33.5):**

- `write_file` returns in about 7 s, and the file is valid JSON, mode 600.
- `read_file` and stdout return `\n`.
- `\0 \377 ^D ^C \r` round-trip through `cat`, and `cat` with no stdin returns at once.
- A qape-hq worktree was cut through `tddy_git::setup_remote_worktree` in 39 s, and the Claude
  settings file was written into it in 6 s. Both were driven through the patched shell.
- No temp file was left behind.

## Not changed (still open)

- **`exec_streamed` over Coder.** It still streams stdin and stdout through the pty, so its EOF and
  its binary bytes (git pack verbs) are exposed in the same way.
- **A remote process is not stopped at the deadline.** A timed-out one-shot kills the local
  `coder ssh` only.
- **Pagers run.** `git log` and similar commands start a pager because stdout is a tty.
- **Cursor `create-chat` mints on this host** with the target's worktree as its cwd. That gives
  ENOENT on a Coder target.
- **A target launch execs this host's resolved agent path** (`{{binary}}`) and this host's agent
  environment on the target.
