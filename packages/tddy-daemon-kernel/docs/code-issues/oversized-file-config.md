# oversized-file: config.rs

**Location:** `packages/tddy-daemon-kernel/src/config.rs`
**Category:** oversized-file
**Detected:** 2026-09-19 by structural audit
**Metrics:** **1,474 production lines** (2026-09-24, #510 HEAD; 1,447 at detection, 2,511 total and first `#[cfg(test)]` at `:1448` then) — **2.9× the 500-line budget** · 19 structs · 11 `resolve_*` functions · 5 env-var consts · `DaemonConfig` carries 33 fields (struct/field counts not re-derived 2026-09-24)
**Restructure:** required — `extract_module`, `/code-restructuring` territory
**Status:** Open — regressed 2026-09-24 (1,470 → 1,472 in #509, `#keyring` 2/9; 1,472 → 1,474 in #510, `#keyring` 3/9; split deferred with consent both times) — **unclaimed**
**Verified:** ✅ hand-verified 2026-09-19 — see *Verified by hand*

## Measurement history

| Run | Production lines | Total | Structs | Note |
|---|---|---|---|---|
| 2026-09-19 | 1,447 | 2,511 | 19 | first detection |
| 2026-09-23 | 1,470 | — | — | master 1,467 → 1,470 after #508 (`#keyring` 1/9; two doc comments) — grown by #508; split deferred to a follow-up after #keyring lands because dependents #509–#513 touch it |
| 2026-09-24 | 1,472 | — | — | 1,470 on the merge-base with `origin/master` (`4e7157d2`) → 1,472 after #509 (`#keyring` 2/9): `users:` becomes the shared `LiveUsers` holder. Grown; the split is deferred with the developer's consent — #509's `## Boundaries` rules it out inside the stack and #510–#513 touch this file (`docs/dev/todo/2026-09-24-keyring-desktop-login-grew-thirteen-over-budget-files.md`). First `#[cfg(test)]` now at L1473 |
| 2026-09-24 | 1,474 | 2,538 | — | 1,472 on `origin/master` `35cf2913` → 1,474 after #510 (`#keyring` 3/9): `GitHubConfig.pending_login_ttl_seconds`, the field and its one-line `#[serde(default)]` pointer — the type, its default (600 s) and its validation live in the new `pending_login_ttl.rs`, so `config.rs` carries only the field. The `auth_storage` doc comment is reworded in place (the vault, not `github-tokens.json`), no net lines. Grown; the split is deferred with the developer's consent to a follow-up after `#keyring` lands (`docs/dev/todo/2026-09-24-keyring-store-deferred-oversized-file-splits.md`). First `#[cfg(test)]` now at L1475 |

## What the tool found

Production lines are counted before the first `#[cfg(test)]`, per the category definition:

```
$ grep -n "^#\[cfg(test)\]" packages/tddy-daemon-kernel/src/config.rs | head -1
1448:#[cfg(test)]
```

1,447 production lines against a 500-line budget. It is the largest file in the crate by a wide
margin — the next is `spawn_as_user.rs` at 426 — and it holds 1,447 of the crate's 3,957 production
lines on its own.

**There is one clean seam, and it is measurable.** Lines **545–970 (426 lines, 29% of the file)** are
not configuration *shape* at all. They are binary- and path-*resolution*: eleven functions that take
a `&DaemonConfig` and an environment and answer where something lives.

```
545  resolve_vnc_binary_path            852  CURSOR_HOME_ENV
567  resolve_rdp_binary_path            855  resolve_cursor_home_dir
593  CLAUDE_BINARY_ENV                  873  CLAUDE_HOME_ENV
682  resolve_claude_binary_path         884  resolve_claude_home_dir
744  CURSOR_BINARY_ENV                  902  SANDBOX_CONFIG_ENV
807  resolve_cursor_binary_path         906  SANDBOX_CONFIG_BASENAME
824  resolve_cursor_cli_tddy_tools_path 913  sandbox_config_os_token
838  resolve_cursor_cli_daemon_url      937  resolve_sandbox_config_path
```

The rest of the file is serde structs and their defaults, which is what a `config` module is for.
This block is a different job: it reads `TDDY_*` environment variables, applies precedence, and
returns paths. It is contiguous, it is a single concern, and it is the only part of the file with
non-trivial branching.

**Not measured:** no CRAP score and no per-function complexity. Coverage was not collected for this
crate, so this record is a size and cohesion finding only. The crate's 78 passing unit tests are
concentrated in this file's own 1,063 test lines, so the file is **not** untested — do not infer a
CRAP problem from the size.

## Why it matters here

The size alone is the weaker half of this. The stronger half is that the file's name stops
predicting its contents: a reader looking for "how does the daemon decide which `claude` binary to
run" has no reason to open `config.rs`, and a reader changing a config struct has 426 lines of
unrelated path logic between them and the next struct.

It also sits under two other open findings on this crate. `heavy-dependency-livekit-peer-forwarding`
wants `peer_forwarding` extracted; this file is the other large thing in the same crate, and both
extractions want the same warm index and the same `restructure check --deep` pass. Whoever does one
should cost the other at the same time.

**For `#keyring`.** `#keyring` 1/9 changes what this file *means* rather than its shape:
`LiveKitConfig::api_secret` (`:994`) stops being the daemon's signing key, and `build_auth_entries`
stops requiring a `livekit` block at all. That is an edit of a few lines in `DaemonConfig` and its
tests. **It is not a reason to split the file inside that node** — see below.

## What would close it

`extract_module` the 545–970 block into a sibling — `binary_paths.rs` or `resolved_paths.rs` — and
re-export from `config.rs` so no caller's path changes. That alone takes the file to roughly 1,020
production lines: still over budget, but the seam that exists is the only one that is both
contiguous and single-concern. Splitting the remaining serde structs by domain (sandbox, agents,
transport) is a second, larger judgement call and should not be bundled with the first.

Anchor with `tddy-tools restructure anchors` rather than by hand, and prove the seam with
`restructure check --deep` against a warm index (`./run-index-daemon`) — a cold crate-graph load on
this workspace costs six to ten minutes per invocation.

**Do not absorb this into a feature node.** Two backlog entries and this repo's own planning policy
say the same thing: a mechanical 426-line move inside a feature PR buries the reviewable diff. It
belongs on a follow-up branch of its own.

## Verified by hand

**2026-09-19.** Checked:

- Counted production lines by locating the first `#[cfg(test)]` (`:1448`), not by `wc -l`, so the
  1,063 test lines are excluded. The 2,511 total is `wc -l`.
- Read the 545–970 region's function and const declarations and confirmed all eleven `resolve_*`
  functions and all five `TDDY_*` env consts fall inside it, with no serde struct interleaved. This
  is what makes it a *contiguous* seam rather than a scattered concern — an important distinction,
  since prior `#carve` nodes found that interleaved definitions split seams and panic the assist.
- Counted `DaemonConfig`'s fields (33) by reading `:310-445`.

**What I deliberately did not record.** 33 fields on `DaemonConfig` would meet a naive `god-object`
threshold, and I am **not** opening that record. `DaemonConfig` is a configuration aggregate root
deserialized from one YAML document; its fields are unrelated to each other *by design*, because the
YAML's top-level keys are. Counting them as a god-object would be a true metric and a false finding.
If a later pass disagrees, it should argue from the *methods* on the type, not the field count.
