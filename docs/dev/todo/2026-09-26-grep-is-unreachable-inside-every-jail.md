# 2026-09-26 — `Grep` is unreachable inside every jail, and fails one call at a time

**Category:** Defect — patched temporarily; `ToolSpec[]` is the designed fix
**Source:** found live while testing `./desktop-dev` session
`01a0deca-5f5f-78a3-8c7d-b1e20272b4d9`, 2026-09-26

`tool_grep` (`packages/tddy-tool-engine/src/lib.rs`) shells out to `ripgrep`. A workspace jail's
PATH is `/usr/bin:/bin:/usr/sbin:/sbin` (`packages/tddy-sandbox/src/runner_env.rs:12`), and
Homebrew installs `rg` under `/opt/homebrew/bin` (Apple Silicon) or `/usr/local/bin` (Intel).
Neither is on that PATH, and neither is in `shell_interactive_policy`'s `exec_paths`
(`packages/tddy-sandbox-recipes/src/plan.rs`), so **`Grep` has never worked inside a sandboxed
session on a Mac**. Every call returns

```
Grep: spawn failed: No such file or directory (os error 2)
```

## How it went unnoticed

It presents as one refused tool among many, per call, with no startup signal. In the session that
found it, the subagent's single `Grep` failed, it fell back to `Glob`, globbed a path prefix that
does not exist in that repo (`web/**/*` — the web package is `packages/tddy-web`) **thirty times**,
got a correct empty answer each time, and then produced a confident summary. Losing the only
content-search tool is what pushed it onto a path where it could find nothing.

## The temporary patch

`build_workspace_tool_plan` (`packages/tddy-daemon-sandbox/src/workspace_tool_sandbox.rs`) now
resolves the host's own `rg` at plan time (`host_ripgrep_dir`) and, when there is one, prepends its
directory to the jail's PATH **and** adds an `.executable()` subpath read for it. Both halves are
required: `exec_paths` grants `process-exec*` and no read, and a binary the loader cannot read is a
binary it cannot run.

Resolved rather than hardcoded, so it is correct on Intel, Apple Silicon and Linux, and grants
nothing on a host with no `rg`.

## Why it is still the wrong shape

- It widens a jail's exec surface to **a whole directory of unrelated Homebrew binaries** because
  of one tool's dependency. The jail's entire claim is an explicit allow-list.
- It is silent when `rg` is missing: `Grep` goes back to failing per call.
- It makes a jail's capabilities depend on what the operator happened to `brew install`.

## What closing it looks like — `ToolSpec[]`

Decided 2026-09-26: the jail accepts a **declared list of external tools**, rather than the plan
builder hardcoding a lookup for one named binary.

A `ToolSpec` names a tool the jail needs and states what that costs the sandbox: which paths
become readable, which become executable, and what joins the jail's `PATH`. `build_workspace_tool_plan`
then takes `&[ToolSpec]` and renders the grants from it, instead of `host_ripgrep_dir()` splicing
one directory in by hand.

What that buys over the monkeypatch:

- **The grant is the binary, not its directory.** Today `rg` drags the whole of
  `/opt/homebrew/bin` into the jail's exec surface.
- **A tool the host cannot satisfy refuses at jail startup**, with the tool named — not a
  `spawn failed: No such file or directory` once per call, which is exactly how this survived.
- **The list is the statement of what a jail may run**, reviewable in one place, in a sandbox whose
  whole claim is an explicit allow-list. Right now that claim has an undeclared exception in it.
- **It generalises.** `rg` is the one that bit, but any tool the engine shells out to has the same
  problem waiting.

Still worth weighing while designing it: implementing `Grep` in-process would remove this
particular dependency altogether — `tddy-discovery`'s `CodebaseAccess::Local` already has a
`regex`-based `grep_file`, so the capability is in the tree and it is the *engine* copy that
shells out. `ToolSpec` is the right general answer either way; `Grep` may simply not need to be one
of its clients.
