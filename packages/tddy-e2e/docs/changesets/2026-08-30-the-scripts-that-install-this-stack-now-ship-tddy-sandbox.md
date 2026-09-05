# 2026-08-30 — the scripts that install this stack now ship `tddy-sandbox-runner`

**Type:** Fix

every jail spawns it, and `./release` built five other binaries while `./install` and `publish.sh` shipped it nowhere. A developer checkout always has it in `target/debug` and the daemon resolves it as a sibling of its own executable, so only a freshly installed host ever saw the gap — where the first sandboxed session dies on a missing executable. It is now required in the same array that already gates `tddy-daemon`, so the install fails loudly instead. New fast suite plus a VM-backed one under `./vm-tests`. (tddy-e2e)
