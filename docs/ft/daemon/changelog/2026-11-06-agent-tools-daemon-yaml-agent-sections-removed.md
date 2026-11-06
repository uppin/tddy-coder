# 2026-11-06 — The daemon.yaml agent sections and their readers are deleted

`#agent-tools` 19/19 ([#825](https://github.com/uppin/tddy-coder/pull/825)).

- **Breaking: no agent setting is read from daemon.yaml.** `claude_cli:`, `cursor_cli:` and
  `cli_harnesses:` were refused at start-up since 12/19; the typed `DaemonConfig` fields, their
  types and every reader are now gone too. Each setting lives in the agent's tool file,
  `<tddyhome>/tools/<id>.yaml`:

  | removed | replacement |
  |---|---|
  | `binary_path` | `binary.command`, or the spec's `binary.env` (`TDDY_CLAUDE_BINARY`, `TDDY_CURSOR_AGENT`, `TDDY_OPENCODE_CLI`, `TDDY_TOOLS_BINARY`, ...) |
  | `claude_home_dir` | `home: { kind: path, path: <absolute dir> }` — mounted read-write in the jail |
  | `fullscreen` | `run.env: { CLAUDE_CODE_NO_FLICKER: "1" }` (declared by `claude.yaml`; an override without it turns fullscreen off) |
  | `answer_wait_secs` | `coding_agent.answer_wait_secs` |
  | `tddy_tools_path` | `tools/tddy-tools.yaml` `binary.command` / `TDDY_TOOLS_BINARY` |
  | `daemon_url` | `listen.advertise_url` (one hook url for every agent) |

- **A binary found nowhere is refused**, naming the agent, its `binary.env` and every place searched
  (`tool claude is unavailable: searched TDDY_CLAUDE_BINARY (unset), PATH (...)`). There is no
  "configured value as-is" last step, so a bare `claude` is never handed to a spawn.
- **The answer wait is per session.** A start records the wait it wrote into the agent's hooks file
  (`answer-wait-ms`, beside `.session.yaml`); `AwaitQuestionAnswer` holds for that value, so editing
  `answer_wait_secs` changes new sessions only, and the hold always equals the agent's hook timeout.
- **`CLAUDE_CODE_NO_FLICKER` comes only from the spec's `run.env`**, on a host, in a jail and on resume.
- **`home: { kind: path }`** is a new form of a tool's home: an operator's own absolute directory.
  A relative path is refused when the tool file loads.
- **A Telegram delete removes an agent session's worktree again**, unless the agent's spec keeps it
  (`keep_worktree_on_delete`).
- **Every launcher holds the coding agents.** A host that installed none holds an empty registry, so
  every agent session is refused for naming no spec; the hand-built ACP launch path is deleted.
- **Tool-file route shapes use the body-format words** (from `#agent-tools` 13/19, #819, listed here
  for operators migrating overrides in one pass): `route.shape: args` is now `toml_args` and
  `config_file` is `json_config_file`; a tool file still writing the old word is refused when it
  loads, naming the new one.
- **The catalogue's model probe resolves an agent's binary as its launch does** — through the host's
  coding agents, never a separate load of the tool files.

**Migrating a daemon.yaml:** delete the `claude_cli:`, `cursor_cli:` and `cli_harnesses:` sections and
write each field you set into `<tddyhome>/tools/<id>.yaml` per the table above (`claude.yaml`,
`cursor.yaml`, `opencode.yaml`, …). The daemon refuses to start while a section is present and names
each field's replacement.
