# 2026-06-28 — tddy-sandbox-cgroups

**Category:** Future enhancement
**Source:** sandbox-builder changeset, 2026-06-28

- **Minimal RO-root `pivot_root`** — the sandbox-builder changeset lands read-only bind-mounts of each declared `ReadSpec` inside the rootless jail, but the jail still shares the host filesystem root. Build a minimal tmpfs root, bind only the plan's reads + writable project/scratch/egress, then `pivot_root` into it for full filesystem write-confinement.
