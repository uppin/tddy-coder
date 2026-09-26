# missing-tests: resolve_pty_os_user

**Location:** `packages/tddy-daemon-kernel/src/privilege_drop.rs:75` — `resolve_pty_os_user`
**Category:** missing-tests
**Detected:** 2026-09-19 by structural audit
**Metrics:** **0 tests enter it**, in this crate or any other · 3 production call sites in 3 crates · the only one of `privilege_drop`'s 5 public symbols with no test
**Coverage:** **not measured** — `tddy-tools analyze coverage` was not run; "0 tests enter it" is a call-graph check across all 40 packages, not a coverage tier
**Restructure:** not required — ordinary work
**Status:** Open — **unclaimed**
**Verified:** ✅ hand-verified 2026-09-19 — see *Verified by hand*

## Measurement history

| Run | Tests entering | Production call sites | Note |
|---|---|---|---|
| 2026-09-19 | 0 | 3 | first detection |
| 2026-09-26 | 0 | 3 | #526 (`#carve` 15/21) moved `pty_runtime` to `tddy-terminal-rpc`: the call sites are `tddy-terminal-rpc/src/pty_runtime.rs:128`, `tddy-host-service/src/ssh_agent.rs:452` and `tddy-worktree-service/src/remote_git_service.rs:222`. The four tested symbols' tests moved with it, unchanged |

## What the tool found

`privilege_drop` exports five symbols. Four are exercised; this one is not.

| Symbol | Tests | Where |
|---|---|---|
| `pty_requires_privilege_drop` | 2 | `tddy-session-lifecycle/src/pty_runtime.rs:219,229` |
| `wrap_argv_for_privilege_drop` | 1 | `tddy-session-lifecycle/src/pty_runtime.rs:245` |
| `pty_user_env_overrides` | 2 | `tddy-session-lifecycle/src/pty_runtime.rs:180,198` |
| `ResolvedPtyUser` | — | constructed by the function below |
| **`resolve_pty_os_user`** | **0** | — |

Its three references are all production:

```
packages/tddy-session-lifecycle/src/pty_runtime.rs:128
packages/tddy-host-service/src/ssh_agent.rs:452
packages/tddy-worktree-service/src/remote_git_service.rs:221
```

**Why it is untested is the finding.** The function is a `getpwnam_r` passwd lookup. It answers from
the host's real user database, so a test asserting anything specific about a named user passes or
fails according to the machine it runs on. There is no seam — no injected resolver, no fixture
passwd source — so the function cannot be tested without either changing it or pinning the test to
whatever users happen to exist on a CI runner.

## Why it matters here

This is the function that decides **which uid a process drops to**. Its three callers each use the
answer to become another OS user: `pty_runtime` front-loads a `setpriv` with it, `remote_git_service`
does the same for the pipe path, and `ssh_agent` uses `.uid` to locate another user's agent socket.

The uid it returns is the whole of the impersonation boundary, and nothing asserts it. The specific
untested behaviour that matters is **failure**: `ssh_agent.rs:449` carries a comment saying "A passwd
lookup that failed is not a user without an agent", which is a distinction the caller draws
deliberately and no test pins. A change to this function's error shape would not break a single test
in the workspace, and the callers that depend on the distinction would start silently disagreeing.

The neighbouring three symbols being well tested is what makes this asymmetric rather than merely
thin: someone reading `privilege_drop`'s test story sees four green symbols and a module that looks
covered.

## What would close it

Ordinary work. Two honest options, and the choice is a judgement call worth stating in whatever
changeset takes it:

1. **Give it a seam.** Take the passwd lookup as an injected function (default: the real
   `getpwnam_r`), then test the mapping and every error path against a fixture. Costs one parameter
   or one trait on a function with three callers.
2. **Test it against the running user only.** `resolve_pty_os_user(&whoami)` must return the current
   process's own uid/gid/home, and a name that cannot exist must return `Err`. That is machine
   independent, needs no production change, and pins the two things the callers actually branch on.

Option 2 is the smaller change and closes the failure-path gap that matters most; option 1 is what to
do if the error shape ever needs to carry more than a `String`. **Do not do both at once.**

## Verified by hand

**2026-09-19.** Checked:

- Listed every reference to each of the five public symbols across `packages/`, then opened
  `pty_runtime.rs` and confirmed which call sites sit after its `#[cfg(test)]` at `:166`. Lines 180
  and 198 are inside test bodies; 128 and 132 are production.
- Confirmed `privilege_drop.rs` itself has no `#[cfg(test)]` block at all.
- Read `resolve_pty_os_user` at `:75` and confirmed the passwd lookup has no injection point.

**A grep artifact this record had to route around, recorded so a re-run does not repeat it.** My
first pass filtered references by lines matching `test|assert` and concluded that
`wrap_argv_for_privilege_drop` was untested. It is tested — the call sits on a plain `let wrapped =
…` line inside a test function, and the test-ness is three lines up at the `#[test]` attribute.
Deciding coverage from the matched line's own text is wrong for any symbol whose result is bound
before it is asserted, which is most of them. Decide it from the enclosing item instead, which is
what this record did.
