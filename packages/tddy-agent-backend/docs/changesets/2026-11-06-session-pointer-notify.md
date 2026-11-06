# 2026-11-06 — `SessionPointer` gains `notify`; its socket is optional

**Type:** Feature (format extension)

`#agent-tools` 17/19 ([#823](https://github.com/uppin/tddy-coder/pull/823)).

- `SessionPointer.socket: Option<PathBuf>` (omitted when `None`), and a new `notify: BTreeMap<String, String>` (omitted when empty): the operator-notify session `tddy-tools notify` reads. A pointer written before this change still parses.
- The headless backend writes `socket: Some(..)` and an empty `notify`, so its behaviour is unchanged. The daemon's generic CLI launch (`tddy-agent-launch::cli_session_pointer`) writes both.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-06-agent-tools-cli-dispatch-collapse.md).
