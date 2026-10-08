# 2026-10-08 — `move_cluster_to_crate` moves a module's file but not its directory children, even when each child is named in `also`; their crates are missing from the manifest

**Category:** Engine failure plus manual fixes (a hand move and manifest lines), developer-consented 2026-10-08
**Source:** #carve 21/21 (PR #536), R7: `cli_session_manager` (anchor) with nine directory children (`argv`, `control_lease`, `launch`,
`livekit_bridge`, `livekit_terminals`, `pty_handle`, `pty_spawn`, `relaunch`, `terminals`) and `session_toolcall` → new crate `tddy-cli-sessions`.
Engine causes already filed:
[2026-10-04-restructure-move-cluster-to-crate-leaves-a-modules-directory-children-behind](2026-10-04-restructure-move-cluster-to-crate-leaves-a-modules-directory-children-behind.md)
(children and the manifest's missing crates)

## What the engine did

1. `check --deep` over the cluster with the parent only: `no findings`. `apply`: 2 files moved, nine children stranded (`E0583` ×9) and 39 further errors.
2. Same, with all nine children in `also` (a partial subset is refused as "names `tddy-session-lifecycle`"): `check --deep` `no findings`,
   `apply --dry-run` `resolved … 8 file(s)`, `apply` `8 file(s) applied` — **still only the parent and `session_toolcall` moved**; the children named in
   `also` stayed in `cli_session_manager/`. (In R3 and R4 a named child *was* moved, flattened to a root sibling; here none was.) The moved parent kept its
   nine `mod x;` lines.
3. Because the children were never read, the destination's manifest lacked every crate only they name.

## Manual fixes (build corrections)

- **Hand move:** `git mv packages/tddy-session-lifecycle/src/cli_session_manager packages/tddy-cli-sessions/src/cli_session_manager` — the directory the
  parent's `mod` lines already expect, keeping the nesting (no flattening, no path edits needed in the children). In its own commit.
- **Manifest lines the engine missed** (`packages/tddy-cli-sessions/Cargo.toml`, versions copied from lifecycle's): `anyhow`, `async-trait`, `bytes`, `libc`,
  `log`, `portable-pty`, `prost`, `uuid`, and the approved path edges `tddy-livekit`, `tddy-service`, `tddy-session-activity`.
- **New crate skeleton by hand** (`Cargo.toml`, `src/lib.rs`, a `members` line in the root `Cargo.toml`): a plan whose `to` is not a crate is refused
  (`is not a crate: …/Cargo.toml could not be read`). The engine gains the dependency edges once the crate exists.

## What the engine should do

Carry a module's directory children with it when it moves (keeping nesting), or move the `also` members it is given; read the children's paths for the
manifest; and offer a create-crate step so a destination need not exist. Delete this file with those fixes.
