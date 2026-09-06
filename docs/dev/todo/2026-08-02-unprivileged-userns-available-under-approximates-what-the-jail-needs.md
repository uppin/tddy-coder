# 2026-08-02 — `unprivileged_userns_available()` under-approximates what the jail needs

**Category:** Future enhancement

`packages/tddy-daemon/tests/action_sandbox_acceptance.rs` →
`sandboxed_bash_action_writes_to_output_dir` **fails instead of skipping** on a host with
`kernel.apparmor_restrict_unprivileged_userns=1`:

```
sandbox I/O error: spawn sandbox runner in cgroups jail failed:
Operation not permitted (os error 1) (the host may forbid unprivileged user namespaces)
```

The test has the correct self-skip guard, and the guard *passes* — `unprivileged_userns_available()`
returns true. The probe (`probe_unprivileged_userns`) only performs `unshare(CLONE_NEWUSER)` plus the
uid/gid-map writes, whereas `enter_rootless_jail` additionally does
`unshare(CLONE_NEWNS|CLONE_NEWNET)`, `mount(/, MS_REC|MS_PRIVATE)` and the cgroup scope write. So the
probe answers a strictly easier question than the one the caller is asking, and the self-skip
contract silently fails to fire.

Two things to fix, and they are separable:
1. The probe should exercise the same steps the jail does (or the jail's extra steps need their own
   probe), so "available" means available.
2. The error message's parenthetical is a guess appended by the caller; it named userns when userns
   was fine. It should report which syscall actually returned `EPERM`.

Verified pre-existing by inspection: the whole `tddy-daemon` diff on this branch is 20 added lines
(one `pub mod`, one `Option` config field defaulting to `None`) with zero deletions, and
`tddy-sandbox-cgroups`, `tddy-sandbox`, `tddy-actions`, `sandbox_session.rs`, `spawner.rs` and
`spawn_worker.rs` are untouched.
