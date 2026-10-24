# main is set up with the New-session form - PRD

**Date**: 2026-10-24
**PRD Type**: Bug Fix + Requirement Update

## Affected Features

- **Primary Feature**: [Session drawer § Pinned main entry](../session-drawer.md#pinned-main-entry) — the
  setup pane becomes the New-session form in a *main* variant; the entry gains **Replace main**.
- **Related Feature**: [Project concept § Home agent / Project-home session](../../daemon/project-concept.md#project-home-session)
  — `home_agent` is removed; the project records its **home session id**; `EnsureProjectHomeSession`
  carries the start spec.
- **Related Feature**: [Projects screen § Home agent](../projects-screen-multi-host.md#home-agent) — the
  picker is removed; the card links to main's setup instead.
- **Related Feature**: [Session drawer § Create Session](../session-drawer.md#create-session) — the form
  gains a main variant (fixed project, sections hidden, types disabled with a reason).

## Summary

Setting up a project's **main** session uses the same form as **New session**: session type
(Claude CLI, Cursor CLI, Tool, OpenCode, Muse Code), model, provider account, permission mode and
initial prompt. main stays **read-only**, so only types the daemon can hold read-only are
selectable; the others are shown disabled with the reason. The project records which session is its
main, replacing the `home_agent` setting and its picker. A **Replace main** action sets it up again.

## Background

Today the setup pane offers every agent `ListAgents` lists and `SetProjectHomeAgent` stores any
listed id, but `EnsureProjectHomeSession` accepts only a **harness profile** whose harness is
`claude-cli`. Picking the catalog's own `claude` ("Claude (opus)") is stored and then fails:

> `[failed_precondition] home agent "claude" is not a harness profile, so it names no harness to run`

With no profile file written by hand, **no** option in the picker can start main. The picker also
offers none of what New session offers — no session type, no permission mode, no initial prompt.

## Proposed Changes

### What's Changing

**Setup (web).**
- The setup pane at `#/sessions/project-home/<id>` is the New-session form in a *main* variant:
  - titled **Set up main for `<project>`** (or **Replace main for `<project>`**), with the existing
    explanation of what main is;
  - the project is fixed and shown, the host is the selected daemon;
  - **hidden**: host and project selects, branch mode, PR-stack parent, attachments, and the
    placement controls (sandbox, managed codebase, sandboxed codebase, split) — main always runs
    managed on its home worktree;
  - **shown**: the session-type toggle, model, provider account, permission mode, initial prompt —
    the same fields New session shows for the chosen type;
  - the session-type toggle shows every type. A type the daemon cannot hold read-only is
    **disabled** and its reason is shown beside the toggle:
    - **Cursor CLI** — has no tool allowlist, so it could still use its own file tools;
    - **Tool**, **OpenCode**, **Muse Code** — cannot run in managed-codebase mode.
    Claude CLI is selected by default;
  - the submit button reads **Start main session**; it sends `EnsureProjectHomeSession` with the
    form's start spec, then selects and attaches the new session as New session does. A refusal is
    shown in the form with the daemon's reason.
- The pinned **main** entry:
  - is *configured* when the project records a home session id (was: a home agent);
  - a click on a configured entry ensures main (reach / resume). When the daemon answers that main
    no longer exists, the setup pane opens;
  - a click on an unconfigured entry opens the setup pane (unchanged);
  - a configured entry has a **Replace main** action, which opens the setup pane in replace mode.
    Its text says the current main is stopped and stays in the list as an ordinary session.
  - its right-hand label is the home session's model (was: the home agent's label).
- The Projects card's **Home agent** picker is removed. In its place: **Set up main** (unconfigured)
  or **Open main** (configured), both linking to the drawer. The `#/projects?project=<id>` picker focus
  goes with the picker.

**Daemon.**
- `ProjectData.home_agent` and `ProjectEntry.home_agent` are removed; `ProjectData.home_session_id`
  and `ProjectEntry.home_session_id` are added. Older rows' `home_agent` is ignored on read and not
  written back.
- `SetProjectHomeAgent` is removed.
- `EnsureProjectHomeSessionRequest` gains an optional `start` (a `StartSessionRequest`):
  - **No `start`**: reach the recorded home session — running → `running`; stopped → fast-forward
    the home worktree and resume → `resumed`. No recorded id → `NOT_FOUND` ("set up main"), nothing
    starts. A recorded id that no longer names a session is **cleared** and answered the same way.
  - **With `start`**: set up (or replace) main from that spec.
    - `session_type` other than `claude-cli` → `FAILED_PRECONDITION` naming the type and why it
      cannot be held read-only.
    - `sandbox`, `sandboxed_codebase` or a `codebase_daemon_instance_id` → `INVALID_ARGUMENT`
      (main runs on the host's home worktree).
    - `session_role` set → `PERMISSION_DENIED` (as `StartSession`).
    - The daemon owns `project_id`, `repo_path` (home worktree), branch fields, `managed_codebase`
      (true) and `session_role` (project-home); the spec's model, provider account, permission mode,
      initial prompt and the rest are used as given.
    - A recorded home session that is running is stopped first. It keeps its record and role (and so
      stays read-only), and is no longer the project's main. Outcome `replaced`; otherwise `started`.
    - The new session's id is recorded on the project.
- The home session is found by the recorded id, not by scanning for the role.

### What's Staying the Same

- main is read-only: the daemon refuses every write tool for a project-home session; only
  `claude-cli` can start it.
- Home worktree, fast-forward-on-resume, project-home tools, git tools, idle auto-stop, serialized
  ensures, per-host home sessions and peer forwarding.
- New session itself — its fields and behaviour are unchanged outside the main variant.

## Impact Analysis

### Technical Impact

- Proto: `session.proto` (Ensure `start = 4`, outcome `replaced`), `project.proto` (remove the RPC,
  reserve `home_agent = 10`, add `home_session_id = 11`); both generated TS trees.
- `tddy-agent-launch` (ensure), `tddy-projects` (storage, handler, service), `tddy-daemon-rpc`
  (remove handler, entry mapping), `tddy-daemon-kernel` (remove peer forward), `tddy-service`
  (`RESIDUAL_METHODS`), ~60 `ProjectData` literals across crates (mechanical rename).
- `tddy-web`: `CreateSessionPane` variant (new modules — the pane is at 483/500 lines), setup pane,
  drawer entry and hooks, Projects card, routing.

### User Impact

- Setting up main works without a hand-written profile file, and offers the New-session options.
- **Migration:** a stored `home_agent` is dropped. A project that already has a main session from
  before this change shows **unconfigured**; its old session stays in the list as an ordinary
  (still read-only) session. Set main up again once.
- Running main under a harness profile on a provider account is still possible: choose Claude CLI
  and the account in the form.

## Implementation Plan

1. Daemon: storage field, proto, ensure-with-spec, removal of `SetProjectHomeAgent`.
2. Web: type-availability rule, form variant, setup pane, drawer entry + Replace main, Projects card.
3. Tests per the changeset; docs via the changeset.

## Acceptance Criteria

- [ ] The setup pane is the New-session form with project fixed and branch/stack/attachments/placement hidden ([Session drawer](../session-drawer.md#pinned-main-entry))
- [ ] Every session type is shown; only Claude CLI is selectable; each other type shows its reason
- [ ] Start main session sends `EnsureProjectHomeSession` with the form's model, provider, permission mode and initial prompt, and selects the new session
- [ ] The daemon starts main from the spec on the home worktree, managed, with the project-home role, and records its id on the project ([Project concept](../../daemon/project-concept.md#project-home-session))
- [ ] The daemon refuses a non-claude-cli type, a sandbox placement and a caller-set role
- [ ] Ensure without a spec reaches / resumes the recorded session; with none recorded, or a stale one (cleared), it answers `NOT_FOUND` and the web opens setup
- [ ] Replace main stops the running main, starts the new one, records it, and the old one is listed as an ordinary session
- [ ] `home_agent`, `SetProjectHomeAgent` and the Projects-card picker are gone; the card links to Set up / Open main ([Projects screen](../projects-screen-multi-host.md#home-agent))
- [ ] Tests passing for the touched packages

## References

### Affected Features (Complete List)
- [session-drawer.md](../session-drawer.md) — Pinned main entry; Create Session main variant
- [project-concept.md](../../daemon/project-concept.md) — Home agent removed; project-home session from a spec
- [projects-screen-multi-host.md](../projects-screen-multi-host.md) — Home agent picker removed
- [url-state-routing.md](../url-state-routing.md) — setup route gains replace mode; `?project=` focus removed

### Related Documentation
- [TODO: cursor-cli cannot enforce managed-codebase mode](../../../dev/todo/2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md)
- Changeset: [2026-10-24-project-home-from-new-session-form.md](../../../dev/1-WIP/2026-10-24-project-home-from-new-session-form.md)
