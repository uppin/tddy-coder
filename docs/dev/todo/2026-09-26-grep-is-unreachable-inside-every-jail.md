# 2026-09-26 — `Grep` is unreachable inside every jail, and fails one call at a time

**Category:** Defect — patched temporarily, real fix outstanding
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

## What closing it would take

Either:

1. **Ship `rg` beside `tddy-sandbox-runner`**, the way `tddy-tools` and `tddy-index-daemon` already
   are (`./install` ships them as siblings and the daemon resolves them as siblings). Then the
   grant is one known binary, not a directory, and it exists on every host by construction.
2. **Implement `Grep` in-process.** `tddy-discovery`'s `CodebaseAccess::Local` already has a
   `grep_file` using the `regex` crate for exactly this reason, so the capability exists in the
   tree; the engine's version is the one that shells out.

Either way, **a missing content-search tool should refuse at jail startup**, not once per call —
the per-call failure is what let this survive.
