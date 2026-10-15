# 2026-10-15 — Local gh as a project's GitHub source

- **A project can use the host's `gh` login.** A **Local gh** toggle on the project card makes its
  sessions commit as the GitHub login the host's GitHub CLI is logged in as, for the project's GitHub
  host (GitHub Enterprise included), with no vault account and no device-flow link.
- **No token is handed out.** The agent runs `gh` itself; tddy's PR tools, workflow steps and the
  re-pointing of a planned PR read the token from `gh` where they run, as the session's OS user.
- **One source per project.** Turning Local gh on removes the GitHub account assignment; a project with
  both is refused, naming both.
- **Forwarded starts carry the source.** A session started on another host uses `gh` even if that
  host's copy of the project was never set, and its resume does too.
- **Telegram-started sessions** of a Local gh project act as the host's `gh` login and are no longer
  told to start from the dashboard.
- **`gh` cannot answer: the session still starts**, under the checkout's identity, with the reason
  logged. Sandboxed sessions work when the daemon runs as the session's OS user; on a multi-user daemon
  they start with no identity and a logged-out `gh`.
- **Exported tokens reach nothing.** `GH_TOKEN`, `GITHUB_TOKEN` and their Enterprise counterparts are
  removed from every process a session runs and from every `gh` tddy runs.
- **Known limits.** Child sessions read the host's own project setting, not the parent's forwarded
  source, and the setting still reaches only the host it was written to. See
  [the open TODOs](../../../dev/todo/2026-10-15-local-gh-child-sessions-and-telegram-gh.md).

See [Per-project GitHub identity](../github-identity.md#local-gh) and
[Project concept](../project-concept.md#local-gh).
