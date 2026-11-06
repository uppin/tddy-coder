# Session resolution (`session_source`)

A `tddy-tools` process reports to one session: the toolcall socket `submit`, `ask`, `transition`
and the MCP server's relays talk to, with the session's directory, repository and remote tool
environment, and the operator-notify session `tddy-tools notify` reads. Neither a headless coding
agent's environment nor a daemon-launched CLI agent's (claude-cli, cursor-cli, any `*-cli` agent)
carries a `TDDY_*` variable, so the `tddy-tools` it runs — from its shell (`tddy-tools submit …`, the
command every workflow prompt names; `tddy-tools notify`) or as its MCP server, which runs in the
agent's working directory — cannot learn its session from the environment.

## Resolution order

`session_source::resolve_session(explicit_socket, cwd, env)` takes the first of three sources that
names a session (`resolve_current_session(socket)` applies it to this process's cwd and environment):

1. **`--socket <path>`** — an explicit socket on the command line.
2. **The session pointer** — the nearest `.tddy/session.json` at or above the cwd
   (`tddy_agent_backend::session_pointer::SessionPointer`, see
   [tddy-agent-backend architecture § Backend](../../tddy-agent-backend/docs/architecture.md#backend-backend), *Session pointer and turn files*).
   It holds the socket (optional: a session with no workflow listener has none), the repository, the
   session directory, the remote tool environment and a `notify` map — the operator-notify session
   (`TDDY_SESSION_ID`, `TDDY_OS_USER`, `TDDY_HOOK_TOKEN`, and `TDDY_DAEMON_URL` or
   `TDDY_DAEMON_SOCKET`). Two writers:
   - the headless workflow backend, in the agent's working directory before each turn, removed when
     the turn ends;
   - the daemon's generic CLI launch, in the session's worktree at every start and resume
     (`tddy-agent-launch`'s `cli_session_pointer`, see
     [module layout](../../tddy-agent-launch/docs/module-layout.md#the-cli-spawns)): the managed
     workflow's socket, session directory and repository, and the notify session. Owner-only, kept out
     of git through the checkout's `info/exclude`, replaced on resume, removed when the agent exits
     and when the session is deleted. A jailed session writes none.
3. **`TDDY_SOCKET`** (with `TDDY_SESSION_DIR`) — for the daemon's own callers and tests; nothing an
   agent inherits.

The answer is a `ResolvedSession { socket: Option<PathBuf>, repo_dir, session_dir, remote, notify, source: SessionSource }`
(`notify` is empty unless a pointer named it).
Errors are `SessionNotFound`:

- **`Nowhere { cwd }`** — none of the three names a session. Its message names all three: pass
  `--socket <path>`, run from a directory at or below a `.tddy/session.json` (searched upward from
  the cwd), or set `TDDY_SOCKET`.
- **`UnreadablePointer { path, reason }`** — a pointer file exists but does not parse. It is an
  error, never skipped for a pointer further up or for `TDDY_SOCKET`.

## Commands with no session

`submit`, `ask` and `transition` need a session. With none they fail with the `Nowhere` message
(exit 1); a validated submission that reached no session is not a success, and nothing is written
locally in its place (a `branch-review` submission's `review.md` included;
`tddy_workflow_recipes::review::persist_review_md_to_session_dir` stays a library function).

## Adopting the session

`tddy-tools-cli` resolves the session once, in `main`, **before** it builds its async runtime, and
`adopt_session` makes it the process's own environment — `TDDY_SOCKET` (when the session has one),
`TDDY_SESSION_DIR`, `TDDY_REPO_DIR`, the `TDDY_REMOTE_*` pair and the notify session — so the code
that reads those variables (the MCP server's permission relays and path rules, the session tool
client, `tddy-tools notify`) finds the session whichever source named it. `session_environment`
lists what is set: of a pointer's `notify` map only the five names `tddy-tools notify` reads
(`session_activity::notify_env`'s) are admitted, so a pointer found by walking up from the cwd cannot
set `PATH` or any other variable. Writing the environment happens while the process is still single-threaded. A
`Nowhere` result adopts nothing and leaves each command to report it; an unreadable pointer stops the
process at start-up (`tddy-tools: the session pointer <path> could not be read: …`, exit 1).

The agent that ran this `tddy-tools` never had those variables, and a command it runs through the
exec tools (`tddy-tool-engine`'s contained shell) starts with every `TDDY_*` variable removed —
inherited or passed — so an adopted session never leaks into the agent's commands. A `tddy-tools`
such a command runs finds the session through the pointer, as the agent's own does.

## Tests

`tests/session_source_unit.rs` (`--socket` beats the pointer and the environment, the pointer beats
the environment, the environment last, nothing anywhere names all three sources, an unparsable
pointer is an error;
`a_pointers_notify_map_sets_only_the_names_tddy_tools_notify_reads`); `tddy-tools-cli/tests/session_pointer_submit.rs` (a shell-run
`tddy-tools submit` with no `TDDY_*` variable reaches its session through the pointer);
`tddy-tools-cli/tests/notify_cli_acceptance.rs` (`notify_reads_its_session_from_the_pointer_when_its_environment_names_none`);
`tddy-tools-cli/tests/cli_integration.rs` (`ask_and_submit_with_no_session_anywhere_fail_naming_all_three_sources`)
and `tddy-tools-cli/tests/branch_review_persist_red.rs` (a `branch-review` submit with no session fails and writes
nothing); `tddy-tool-engine/tests/exec_shell_no_tddy_env.rs` (an exec-tool command sees no `TDDY_*`
variable, inherited or passed).
