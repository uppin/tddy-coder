# 2026-08-30 — `system_baseline_reads` no longer grants the host's per-user temp base

**Type:** Fix

its `/private/var/folders` cache entry reached every recipe-built plan through `plan.reads`, so narrowing the Seatbelt profile's own rules left every jail still able to read the tree. Removed, with a note recording why it must not come back. (tddy-sandbox)
