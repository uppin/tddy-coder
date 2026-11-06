# Sandbox Builder — explicit, cross-platform jail configuration

**Product Area**: Coder
**Status**: Implemented
**Updated**: 2026-11-03

> **`tddy-sandbox`** is generic (`SandboxBuilder`, `exec_reads`, `scratch_runner_env`, `cwd`,
> `extra_read_paths`). The generic recipe shapes (`Generic`, `Shell`, `RunnerPty`) and the lowering of
> tool definitions (`tool_grants`) live in **`tddy-sandbox-recipes`**, which names no coding agent. **`tddy-daemon`** `sandbox_plan_builder` assembles plans from
> `ActionSpec` sandbox metadata; sandbox crates do not import `tddy-actions`.

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
- The Linux backend runs under an unprivileged daemon (`User=tddy`): a functional userns probe
  detects a per-binary AppArmor grant, and the cgroup scope is created in the systemd-delegated
  subtree (`Delegate=yes`) derived at runtime — not the hardcoded cgroup root. Config-overridable via
  the daemon's `sandbox_cgroup:` section; see `packages/tddy-sandbox/docs/architecture.md`.

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
8. **Every agent jail is a tool plan.** No recipe knows a coding agent. A jailed agent's reads,
   copies, home, secrets, policy and egress are lowered from its tool definition, and its argv is
   rendered from its spec ([§ Jail sections](#jail-sections)). This holds for the daemon's sessions,
   for `tddy-sandbox-app --agent <id>` and for split sessions. The plan states the jail's guidance
   directory and transport itself (`SandboxPlan.context_dir`, `SandboxPlan.stdio`). No backend infers
   them from the runner's argv.
9. **Action sandbox plans.** `tddy-daemon` `sandbox_plan_builder` maps `ActionSpec` + `SandboxRequest`
   to process or runner-PTY plans: `SandboxRecipe::RunnerPty` for in-jail PTY (`--pty-command` on
   `tddy-sandbox-runner`); `generic_exec_policy` for confined Rust binaries; `cwd` and
   `extra_read_paths` for working directory and read-only artifact mounts.

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
| `claude_required_reads_include_the_dyld_root_literal` | `packages/tddy-sandbox-recipes` | Recipe contains the `(literal "/")` DyldRoot read. |
| `claude_required_copies_seed_only_the_credentials_file` | same | Only `.credentials.json`; no `settings.json`. |
| Action sandbox (tddy-coder, build RO mount, escape denial, PTY) | `packages/tddy-session-lifecycle/tests/action_sandbox_acceptance.rs` | Confined process and runner-PTY actions; unsupported platform returns `failed_precondition`. |
| `rendered_profile_omits_the_blanket_file_read_wildcard` | `packages/tddy-sandbox-darwin/src/profile.rs` | `(allow file-read*)\n` blanket absent. |
| `rendered_profile_emits_each_declared_read_as_an_explicit_rule` | same | Each declared read present as literal/subpath/regex. |
| `rendered_profile_emits_the_dyld_root_literal` | same | `(literal "/")` present. |
| `rendered_profile_emits_oauth_loopback_inbound_when_requested` | same | `network-inbound … localhost:*` when `allow_oauth_inbound`. |
| `maps_each_declared_read_to_a_readonly_bind_mount` | `packages/tddy-sandbox-cgroups/src/lib.rs` (`#[cfg(linux)]`) | RO bind-mount tuple per read. |
| `marks_non_exec_reads_with_the_noexec_flag` | same | `MS_NOEXEC` on non-exec reads. |
| `maps_plan_limits_onto_cgroup_values` | same | `ResourceLimits` → cgroup v2 values. |

Strong assertions: exact rule strings / tuples (no "contains > 0"), explicit absence of the
wildcard, and the negative argv assertion for the secret channel.

## Declared tools

Since `#agent-tools 1/12` ([#790](https://github.com/uppin/tddy-coder/pull/790)), a binary a jail runs
by name comes from a **tool definition**, not from a plan builder looking one binary up. A tool is
the Bazel-style "executable plus what it needs":

```yaml
# built-in: packages/tddy-actions/tools/rg.yaml
schema_version: 1
id: rg
summary: ripgrep — content search behind the Grep exec tool
binary:
  command: rg            # searched on the daemon's PATH, symlinks canonicalized
  env: TDDY_RG_BINARY    # optional: an absolute path that wins over the PATH search
deps: []                 # other tool ids; their grants join this tool's
reads: []                # extra host paths the tool needs readable
```

**Sources.** Built-ins are embedded from `packages/tddy-actions/tools/*.yaml`. Files in
`<tddyhome>/tools/*.yaml` override them: a file with a built-in's `id` replaces it **whole** (never
field by field), and a new `id` adds a tool. Loading refuses, naming the files and ids involved: an
unknown field, two files with one `id` in the same source, a dep on an unknown id, and a dep cycle.

**Resolution** against the host: the `env` override first (one that is not an executable file refuses
the tool, it never falls back to the search), then `command` on the daemon's `PATH` (absolute entries
only), then canonicalize. An unresolvable binary is an error naming the tool and what was searched,
e.g. `tool rg is unavailable: searched TDDY_RG_BINARY (unset), PATH (/usr/bin:/bin)`. A dep closure in
which two tools share one `binary.command` is refused naming both ids, since only one of them could
own the name on the jail's `PATH`.

**Lowering** (`tddy-sandbox-recipes::tool_grants`; generic `tddy-sandbox` does not learn tools):

- the binary is a **literal executable read** — the file, never its directory, so a Homebrew `rg`
  does not make the rest of `/opt/homebrew/bin` executable;
- its dynamic libraries (`otool -L` on macOS) and the nearest non-symlinked ancestor of each become
  reads;
- a jail-local `bin/` directory of links, one per tool, goes first on the jail's `PATH`, ahead of the
  system directories.

On macOS the Seatbelt renderer emits `process-exec*` for an exec literal as well as an exec subpath;
agent jails declare only exec subpaths, so their profiles are unchanged.

**First client:** the workspace tool jail (below) runs `Grep` through the declared `rg`.

**Running a tool.** A tool tddy spawns itself also declares `run:`. It holds an argv template, static
`env` and a `transport` (`pty` or `stdio`). The template is structured, not a string language:

- a literal is one argv entry, with `{name}` placeholders, and `{binary}` is always the resolved
  binary;
- `{ if: name, then: […], else: […] }` renders a segment when the parameter is bound to a non-empty
  value;
- `{ splice: name }` expands a list parameter in place.

An unbound placeholder or list is an error naming it, never an empty entry. Nothing is passed through
a shell.

**Extensions.** `extensions:` is an open map from an owner name to a section that owner validates. It
follows the same model as `BUILD.yaml`'s open `config`. `tddy-actions` never reads a section. A key
no registered owner claims, or a section its owner refuses, fails the load naming the file.
`tddy-coding-agent-specs` owns `coding_agent`: a coding agent is a tool whose `coding_agent` section
declares its mapping slots. See
[Claude CLI session § Launch](../daemon/claude-cli-session.md#launch-from-the-claude-tool-definition).

**Built-in support tools.** `tddy-tools.yaml` (the MCP relay and hook binary every coding agent lists
in `deps`) is a built-in. It sits apart from the jail tool set, so `ToolRegistry::builtin()` keeps
offering exactly the jail tools. A host that launches coding agents builds its registry from both
sets and the agent specs.

## Tool egress

Since `#agent-tools 3/12` ([#792](https://github.com/uppin/tddy-coder/pull/792)), a tool definition
declares which hosts it may reach, and the jail enforces it:

```yaml
network:
  egress: none            # the default when the section is absent: no shim at all
  # egress: any           # unrestricted (what every agent jail still has)
  # egress: { hosts: ["api.anthropic.com:443", "*.statsig.com", "127.0.0.1:8080"] }
  oauth_inbound: false    # loopback inbound for an OAuth callback
```

**Patterns** are `host[:port]` or `*.domain[:port]`; the port defaults to 443, hosts are matched
case-insensitively, and `*.domain` does not match the apex. A malformed pattern (a scheme, a path, an
inner `*`, port 0, an empty label or a character outside `[A-Za-z0-9-]`) is refused at registry load,
naming the tool. `hosts: []` is a policy that admits nothing — the shim starts and refuses every host.

**Union** (`tddy-sandbox-recipes::tool_grants::lower_network`): one tool declaring `any` makes the
jail `any`; host lists concatenate in tool order without repeats; `none` contributes nothing, so a set
of only `none` tools is `EgressPolicy::None`. `oauth_inbound` is true when any tool declares it. A
plan built from tools sets `NetworkSpec.egress` and `allow_oauth_inbound` from this; recipe-built
agent jails keep `EgressPolicy::Any` (the `NetworkSpec` default). The sandboxed `claude-cli` jail is
built from its tool definition ([§ Jail sections](#jail-sections)): its declared hosts, plus the host
of each `http(s)` URL in the session's provider route, which the launcher admits at plan time (a
`none` jail with a route reaches that host alone; `any` already does). `oauth_inbound` is honoured
only on an unrouted session.

**Enforcement, at two points, independently:**

- **The runner's egress shim** checks every `CONNECT` and plain-HTTP forward. An undeclared host is
  answered `403 Forbidden` whose body is the refusal line, and no `TunnelOpen` is sent. Under `none`
  the shim does not start and the PTY is given no `HTTPS_PROXY` / `TDDY_EGRESS_SHIM`.
- **The host relay's `TunnelOpen`** (`HostRelayConfig::with_egress`) refuses an undeclared host with a
  failed `TunnelOpenAck`, so a jailed process that speaks the relay protocol directly is refused too.
  A port no `u16` holds is refused under every policy.

The runner receives the policy as `--egress-policy <none|any|hosts:h:p,…>` (default `any`) plus one
`--egress-tool <id>` per tool. Every refusal reads
`egress to <host>:<port> refused for session <id>: not declared by tool set [<ids>]` — in the session's
`sandbox-runner.log` from the shim, and through the host logger (target
`tddy_sandbox_runner::egress`) from the relay.

The workspace tool jail lowers its tools' network: it starts a shim only when a tool declares `hosts`
or `any`, allows the jail that one loopback port, and passes the policy and tool ids. Its daemon side
does not yet serve tunnels, so a declared host is refused nothing but also reached nowhere there — see
[`docs/dev/todo/2026-11-02-workspace-jail-serves-no-egress-tunnels.md`](../../dev/todo/2026-11-02-workspace-jail-serves-no-egress-tunnels.md).

Out of scope: path or method filtering, DNS pinning, TLS inspection (the tunnel stays end-to-end TLS).

## Jail sections

Since `#agent-tools 5/12` ([#794](https://github.com/uppin/tddy-coder/pull/794)), a tool definition
also declares what it needs from a jail beyond its binary and network. The sections are generic — any
tool may carry them, and none names an agent:

```yaml
home: { kind: persistent, key: claude }   # or { kind: session } — the default
files:
  - { kind: seed-once, from_host: .claude/.credentials.json, to: .claude/.credentials.json }
  - { kind: mirror, from_host: .local/bin/claude, to: .local/bin/claude }
  - { kind: overlay, env: CLAUDE_CODE_TMPDIR, value: "{tmp}" }
secrets:
  - env: CLAUDE_CODE_OAUTH_TOKEN
    source: { kind: host-env, var: CLAUDE_CODE_OAUTH_TOKEN }   # or { kind: host-file, path: … }
    when: unrouted                                              # or routed; absent is always
policy: { jit: true, fork: true, sysctl_read: true, exec_paths: [/usr/bin, /bin] }
```

`from_host` is relative to the host user's home and `to` to the tool's home: a definition never names
an absolute path on either side. Every section is optional; an unknown field is refused at load.

**Lowering** (`tddy-sandbox-recipes::tool_grants`), over the tool and its dep closure:

| Section | Lowered by | Into the jail |
|---|---|---|
| `home` | `lower_home` | `persistent` → `HOME=<tddyhome>/homes/<key>` (`homes::persistent_home`), mounted read-write and shared by every session naming the key; `session` → a fresh home in the jail's scratch |
| `files` | `lower_files` | `seed-once` copies the host file into the home (mode `0600`) **only when the home lacks it**, before the jail exists — a home that refreshed its own token keeps it; `mirror` is a symlink at `to` to the canonical host path, plus the read (executable for a file) that makes it followable; `overlay` sets an env var, with `{home}` and `{tmp}` replaced. A host path that does not exist seeds or mirrors nothing. A directory at a mirror's `to` that holds only links (an older mirror) is replaced; any other directory is the tool's own and is left alone |
| `secrets` | `lower_secrets` | a `SecretSpec` per item the route guard admits — `when: unrouted` is dropped on a provider-routed launch, `when: routed` on an unrouted one. A `host-env` source unset or blank delivers nothing. Each is delivered as a `TDDY_SECRET_<NAME>` file the runner resolves on the child alone |
| `policy` | `lower_policy` | one `PolicySpec` for the set: an allow is granted when any tool asks for it, `exec_paths` concatenate without repeats. Mach lookup and the pseudo-terminal are the launcher's |

**Persistent homes** live under `<tddyhome>/homes/`. The first resolution of a key whose legacy,
tool-named home `<tddyhome>/sandbox-<key>-home` exists renames it into place once, keeping its
contents; when both exist the resolution is refused naming both, and neither is touched.

**The jail of a tool** (`tddy-daemon-sandbox::tool_jail`): `lower_tool_jail` combines the sections
above with `lower_tools` (the closure's `bin/` links and exec reads, plus the binary's own process
reads) and `lower_network` with the route's hosts; `build_tool_jail_plan` puts those grants on the
runner's plan, replacing the reads, policy and egress the runner's argv would imply, and adds each
secret with `SandboxPlan::add_secret`.

**The runner execs a plan** (`tddy-sandbox-runner --exec-plan <file>`). The file is an `ExecPlan`:
the rendered `argv`, the `env`, the `files` to write at their jail paths, and whether to point the
process at the egress shim. The runner writes the files and spawns the argv in its PTY with an
environment of its own minus every `TDDY_SANDBOX_*` control variable, plus the plan's env with each
`TDDY_SECRET_<NAME>` resolved to `<NAME>` (its file unlinked), plus `HTTPS_PROXY` / `HTTP_PROXY` in
both casings (`NO_PROXY` bypassing loopback) when the plan asks for the shim and the jail's policy started one. The
runner learns nothing about which tool it is running; the jail ends when that one process exits.
`--exec-plan` and `--workspace-tools` are exclusive.

**Clients:** every jailed coding agent. That covers the daemon's sandboxed sessions
([Claude CLI session § The jail](../daemon/claude-cli-session.md#the-jail-from-the-claude-tool-definition),
[Cursor CLI session](../daemon/cursor-cli-session.md)), `tddy-sandbox-app --agent <id>`, and split
sessions. The runner has no agent mode left to fall back to: started with neither `--exec-plan`, nor
`--pty-command`, nor `--workspace-tools`, it refuses to boot (`nothing to run`).
