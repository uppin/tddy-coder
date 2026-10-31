# `install` is 1,503 production lines — split the account/identity handling out

**Detected:** 2026-11-01, `/pr-wrap` step 3.5 file-length gate on
[PR #803](https://github.com/uppin/tddy-coder/pull/803): the root `./install` script measured
**1,408 → 1,503** production lines across the PR (budget 500; it was already ~3× over before the PR,
which added the `read_daemon_service_key` extraction and the daemon-identity derivation block).
The gate's extension filter does not match an extensionless script, so the number below is the
honest measurement, not the gate's output.

**Deferred by:** developer consent, 2026-11-01 — the `code-restructuring` engine is Rust-only and
hand-splitting a root bash script mid-PR would delay review of an otherwise-small change. Recorded
per the file-length gate's deferral contract.

**What would close it:** `install` under 500 production lines. The natural seams, both already
self-contained regions: (1) the account/identity handling (`read_daemon_service_key`, the
derivation block, the useradd/usermod/chown branch — the part PR #803 grew), sourced as a helper
file the way the script already sources nothing today; (2) the desktop install section, which is
already documented as self-contained ("shares this script's helpers"). A bash split needs a
sourcing convention plus the `install_contract` static checks updated to follow the files.
