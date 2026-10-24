# 2026-10-24 — main is set up with the New-session form

- Setting up a project's **main** session opens the **New-session form** in a main variant: the
  project and host are fixed, and session type, model, provider account, permission mode and initial
  prompt are offered as for a new session. Branch, PR-stack parent, attachments and placement are not
  shown, because main always runs managed on its project's home worktree. See
  [session drawer § Main variant](../session-drawer.md#main-variant).
- main stays read-only, so only **Claude CLI** can be chosen. Cursor CLI, Tool, OpenCode and Muse
  Code are shown disabled, each with the reason it cannot be held read-only.
- **Start main session** starts main from the form. Setting up main works without a hand-written
  harness profile, and the reported failure (`home agent "claude" is not a harness profile`) cannot
  occur: there is no home-agent setting to store an unstartable agent in.
- The pinned **main** entry is labelled with main's model, and a set-up main offers **Replace main**
  (`#/sessions/project-home/<id>/replace`). Replacing stops the current main; it stays in the list as
  an ordinary, still read-only session. A main the daemon no longer has opens the setup form. See
  [session drawer § Pinned main entry](../session-drawer.md#pinned-main-entry) and
  [URL state routing](../url-state-routing.md).
- The Projects card's **Home agent** picker is gone. The card links to **Set up main** or **Open
  main** instead, and `#/projects?project=<id>` no longer focuses anything. See
  [Projects § Main session](../projects-screen-multi-host.md#main-session).
- **Migration:** a stored home agent is dropped. A project that had a main before shows as not set
  up, and its old session stays in the list as an ordinary session. Set main up again once.
- The daemon side is described in
  [project concept § Project-home session](../../daemon/project-concept.md#project-home-session).

- Code issues: The branch-wide re-measure (`/analyze-code-issues` on tddy-agent-launch, tddy-daemon-rpc and tddy-projects at `4348f160a` against `c1df8e142`, commit `f3f1c93ac`) attributes no finding to this change; `start_session_on_request` in tddy-agent-launch improved from 393 to 384 lines. No record closed.
