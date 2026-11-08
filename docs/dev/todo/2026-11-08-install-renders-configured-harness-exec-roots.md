# 2026-11-08 — `./install` does not render the configured harnesses into `exec_roots`

**Category:** Gap (install / supervisor policy)
**Source:** `#agent-tools` 32/32 (`supervisor-agent-agnostic`, #865), pass-2 user decision 2026-11-08.

## What is there

`./install` renders `spawn_policy.exec_roots` with the install bin dir plus the file `git` resolves to (and
`/nix/store` as an operator-style trusted prefix when git lives there). A coding-agent harness installed
**outside** the bin dir — `opencode` or `claude-agent-acp` under `/usr/local/bin`, for instance — is not
rendered: the operator adds it as an `exec_roots` entry by hand, or every relayed launch of that agent is
refused at run time (the reason only in the supervisor's log).

## What to decide

Whether `./install` should resolve each harness binary the daemon's spec YAML names (as the daemon user,
following symlinks to their targets) and render each as a single-file `exec_roots` entry — and how it learns
which agents are configured without the supervisor naming any of them.
