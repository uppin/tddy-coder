# 2026-11-18 — A Coder one-shot's stdin and stdout cross the workspace's pty intact

**Type:** Fix
**Packages:** `tddy-remote-shell`, `tddy-tool-engine`

Root cause of every Claude start on the Coder workspace `mantasi/remote-dev` failing at the agent
step with `coder target mantasi/remote-dev: no answer within 60s; the command was stopped`. It was
found by the smoke test of 2026-11-18 (orchestrator session `01a1ecee-e942-7982-abfc-104c6c4baa98`)
and confirmed on the VM by its diagnostics session `01a1ecfc-0fac-77a0-bb0d-ac20a90593ae`: argv, the
remote fds, `wchan`, and the bytes of the file.

- `RemoteShell::write_file` piped `.claude/settings.local.json` into `cat > <path>` on the command's
  stdin. `coder ssh` runs the command in a **pty**, and closing the CLI's stdin never reaches the
  workspace as an EOF.
- The file's last line, `}`, had no newline, so the tty's canonical mode held it back. `cat` never
  returned and left 1816 bytes of invalid JSON. The start hit its 60 s deadline, and the remote
  `sh`/`cat` pair outlived the local `coder ssh`.
- A run against the real workspace showed the return direction broken too. The pty's `onlcr` turned
  every `\n` a command wrote into `\r\n`, so `read_file` and any parsed stdout came back altered.

**`CoderShell::exec` wraps every one-shot so the command never relies on the pty**
(`tddy-remote-shell`): `stty -opost` first, `/dev/null` for a command given no stdin, and given
stdin crossing as counted base64 lines.

- The workspace reads exactly that many bytes with `head -c` and decodes them into an owner-only
  `mktemp` file. It checks the file holds the original length, makes it the shell's stdin, and
  unlinks it before the command runs.
- A stdin that did not arrive whole runs nothing and exits 125.
- Echo is off before the start marker, and the lines are written only once the marker has arrived.
- The bytes stay off the command line, because the session pointer written this way carries the hook
  token.
- `base64 = "0.22"`, the version six workspace crates already use, is a dependency of
  `tddy-remote-shell`.

**`StartSession`'s `new_branch_name` description** (`tddy-tool-engine`) states that an agent whose
spec declares `branch_name_required` (the built-in `claude-cli`) refuses an empty name, and that an
empty `branch_worktree_intent` generates one for every agent. The smoke test read the old text as a
promise and was refused. The refusal itself is by design.

**Pinned by** five tests in `tddy-remote-shell/tests/coder_shell_acceptance.rs`, over stand-ins whose
pty never ends the command's input and whose pty ends every line with `\r\n` until `stty -opost`:

- a settings file without a trailing newline is written whole, its token on no command line;
- control bytes reach `cat` exactly;
- a command given no stdin reads an empty one;
- a file reads back without added `\r`;
- printed lines end as printed.

The first three hung against the previous `coder.rs`, and the last two failed on the added `\r`. The
schema description is pinned by `new_branch_name_says_an_agent_requiring_one_refuses_it_empty`.

**Run against the real deployment** (`coder.ireland-devex.wixprod.net`, CLI v2.33.5,
`mantasi/remote-dev`):

- `write_file` returns in about 7 s, and the file is valid JSON, mode 600.
- Newlines come back as `\n`, and `\0 \377 ^D ^C \r` round-trip through `cat`.
- A qape-hq worktree was cut through `tddy_git::setup_remote_worktree` in 39 s, and its Claude
  settings file was written in 6 s.
- No temp file was left behind.

**Backlog.** Narrowed, not deleted:
`2026-11-16-coder-hosts-never-run-against-a-real-deployment` — the workspace argument form and the
`CODER_URL` / `CODER_SESSION_TOKEN` variables are verified against a real deployment, and the other
facts stay open. Recorded as open work, each with why it is deferred:

- `2026-11-18-cursor-chat-is-minted-on-the-daemon-host-for-a-target`
- `2026-11-18-a-target-launch-runs-the-daemon-hosts-agent-path`
- `2026-11-18-claude-on-a-target-will-meet-the-folder-trust-prompt`
- `2026-11-18-coder-exec-streamed-crosses-the-pty-raw`
- `2026-11-18-coder-one-shot-deadline-leaves-the-remote-process`
- `2026-11-18-coder-one-shots-start-pagers`
- `2026-11-18-coder-start-visibility-gaps`
- `2026-11-18-idle-session-reported-as-waiting-on-a-permission`

The smoke test's other finding, a second Tddy Desktop instance on the same `~/.tddy` breaking the
running app's agent-tool socket, belongs to the separate investigation of that socket and is not
recorded here.

**Code issue measured:** `complexity-start-session-args-schema-schema-offering` is unchanged at 192
lines, with only a description string edited, and stays open.
