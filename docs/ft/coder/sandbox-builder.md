# Sandbox Builder — explicit, cross-platform jail configuration

**Product Area**: Coder
**Status**: Draft
**Updated**: 2026-06-28

## Summary

The sandboxed Claude session is configured today by ad-hoc, implicit machinery that differs per
platform and trades away read confinement. The macOS Seatbelt profile carries a blanket
`(allow file-read*)` wildcard; read paths are auto-discovered by implicit detectors; files are
copied into the jail by a hard-coded `seed_claude_home_config`; env is built in `tddy-daemon` and
leaks on macOS via `/usr/bin/env -i`; and the Linux backend ignores the read allow-list entirely.

This feature introduces a single **`SandboxBuilder`** in the shared `tddy-sandbox` crate that
produces an explicit, auditable **`SandboxPlan`** consumed by both the macOS (Seatbelt) and Linux
(rootless cgroups) backends. **Nothing is read, copied, symlinked, or exposed unless a caller names
it.** The macOS read wildcard is removed in favour of an explicit read allow-list; the "what Claude
needs" set lives in one reviewable recipe; and a non-leaking secret channel delivers
`CLAUDE_CODE_OAUTH_TOKEN` to the inner Claude process without it ever touching `sandbox-exec` argv.

## Background

- macOS backend `tddy-sandbox-darwin` renders an SBPL profile from a template that statically
  contains `(allow file-read*)` — documented tech debt that lets the V8/Node `claude` binary boot
  but removes all read confinement.
- `detect_allow_read_paths` (xcode-select/node/brew) and `build_allow_read_paths` (`otool -L`) are
  duplicated across `tddy-daemon` and `tddy-sandbox-darwin` and run implicitly.
- `seed_claude_home_config` implicitly copies host `~/.claude/{.credentials.json,settings.json,
  settings.local.json}` into the jail. The copied `settings.json` drags personal hooks into the
  jail, which fail noisily (`node: command not found`, `notify.sh: No such file or directory`,
  `bad interpreter: Operation not permitted`).
- Linux backend `tddy-sandbox-cgroups` confines network + resources + uids but inherits the host
  filesystem (explicit `FIXME(fs-confinement)`); it ignores `SandboxSpec::allow_read_paths`.

## Requirements

### Functional

1. **Explicit builder, no implicit lists.** `SandboxBuilder::build()` is pure (no filesystem, no
   subprocess detection) and contains no default read/copy sets. Every `ReadSpec`, `CopySpec`,
   `SymlinkSpec`, env var, and secret is added by the caller.
2. **Strict reads on macOS.** The rendered SBPL profile no longer emits `(allow file-read*)`. The
   explicit read allow-list (always including the `(literal "/")` dyld-cache root) is the only read
   policy. `claude` must still boot under the strict profile.
3. **Copy vs allow-read are explicit and typed.** A resource is either an allow-read entry or a
   copy entry, declared by the caller. `seed_claude_home_config` is replaced by explicit
   `CopySpec`s that seed only `.credentials.json`.
4. **Centralized, non-leaking env + secrets.** Env is defined via the builder
   (`default_runner_env` moved to the shared crate). Secrets (`CLAUDE_CODE_OAUTH_TOKEN`) are written
   to a `0600` file under scratch and set on the inner Claude PTY child only, then unlinked — never
   placed in `sandbox-exec` argv or the broad env list.
5. **Explicit policies & resources.** Network rules, mach-lookup, process-exec paths, sysctl,
   pseudo-tty, and cgroup limits are explicit fields of the plan, not template-static blanket
   allows.
6. **Shared by both backends.** The same `SandboxPlan` drives `tddy-sandbox-darwin` (SBPL render)
   and `tddy-sandbox-cgroups` (read-only bind-mounts of each declared read, copies, symlinks, env,
   cgroup limits).
7. **Explicit symlinks.** Symlinks created inside the jail are declared as `SymlinkSpec` and
   materialized by each backend.
8. **Single Claude recipe.** `claude_required_reads()`, `claude_required_copies(host_home)`, and
   `claude_policy()` in `tddy-sandbox/src/claude_spawn.rs` are the one reviewable source of what a
   Claude jail needs; both the app and daemon call them.

### Non-functional

- `build()`, `render_plan()`, and the Linux `plan_to_bind_mounts()` mapping are pure and
  unit-testable without a kernel (cross-platform CI).
- No silent fallbacks: a missing required read surfaces as a fast, legible boot failure
  (`try_exit_diagnostic` already decodes the dyld SIGABRT/SIGTRAP signatures).

### Out of scope (follow-ups)

- Full minimal read-only root + `pivot_root` on Linux (this changeset lands RO bind-mounts of the
  declared reads; `pivot_root` is tracked in `docs/dev/TODO.md`).
- Config-driven cgroup limits surface (limits are plumbed through the plan; UI/config is separate).

## Testing Plan

**Test levels:** Unit (pure, cross-platform) + Acceptance (macOS Seatbelt + runner integration).

### Acceptance tests

| Test | File | Asserts |
|------|------|---------|
| `a_strict_profile_still_lets_the_claude_binary_report_its_version` | `packages/tddy-sandbox-darwin/tests/seatbelt_confinement_acceptance.rs` | Under a strict (no-wildcard) plan, `claude --version` exits 0 — the strict-reads validation gate. |
| `a_strict_profile_denies_reading_a_path_not_on_the_allow_list` | same | A read of an out-of-tree path not declared in the plan is denied (replaces the old "broad reads allowed" test). |
| `seatbelt_denies_writes_outside_project_tree` | same | (kept) write confinement unchanged. |
| `the_oauth_secret_is_passed_to_the_claude_child_and_never_appears_in_the_sandbox_exec_argv` | `packages/tddy-sandbox-runner/tests/secret_channel.rs` | The declared OAuth secret reaches the inner child env but never appears in the `sandbox-exec`/`env -i` argv. |

### Unit tests

| Test | File | Asserts |
|------|------|---------|
| `builds_a_plan_with_only_the_reads_the_caller_declared` | `packages/tddy-sandbox/src/builder.rs` | No implicit reads/copies added by `build()`. |
| `deduplicates_reads_with_the_same_host_and_kind` | same | Dedup by `(host, kind)`. |
| `drops_a_read_shadowed_by_an_enclosing_subpath` | same | Shadowed subpath removed. |
| `rejects_a_copy_whose_destination_is_outside_the_writable_jail_tree` | same | Validation error. |
| `rejects_a_symlink_whose_link_is_outside_the_jail_tree` | same | Validation error. |
| `records_a_declared_secret_without_placing_its_value_in_the_env_map` | same | Secret value absent from `env.vars`. |
| `claude_required_reads_include_the_dyld_root_literal` | `packages/tddy-sandbox/src/claude_spawn.rs` | Recipe contains the `(literal "/")` DyldRoot read. |
| `claude_required_copies_seed_only_the_credentials_file` | same | Only `.credentials.json`; no `settings.json`. |
| `rendered_profile_omits_the_blanket_file_read_wildcard` | `packages/tddy-sandbox-darwin/src/profile.rs` | `(allow file-read*)\n` blanket absent. |
| `rendered_profile_emits_each_declared_read_as_an_explicit_rule` | same | Each declared read present as literal/subpath/regex. |
| `rendered_profile_emits_the_dyld_root_literal` | same | `(literal "/")` present. |
| `rendered_profile_emits_oauth_loopback_inbound_when_requested` | same | `network-inbound … localhost:*` when `allow_oauth_inbound`. |
| `maps_each_declared_read_to_a_readonly_bind_mount` | `packages/tddy-sandbox-cgroups/src/lib.rs` (`#[cfg(linux)]`) | RO bind-mount tuple per read. |
| `marks_non_exec_reads_with_the_noexec_flag` | same | `MS_NOEXEC` on non-exec reads. |
| `maps_plan_limits_onto_cgroup_values` | same | `ResourceLimits` → cgroup v2 values. |

Strong assertions: exact rule strings / tuples (no "contains > 0"), explicit absence of the
wildcard, and the negative argv assertion for the secret channel.
