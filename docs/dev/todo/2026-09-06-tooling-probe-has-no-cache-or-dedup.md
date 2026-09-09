# 2026-09-06 — The tooling probe has no cache and no in-flight dedup

**Category:** Host tooling probe
**Source:** `#hosts-screen` 4/8, PR #456

- The product model is the Hosts screen probing **every host it lists**, i.e. a poll. Overlapping
  calls stack rather than coalescing, and each one spawns up to three subprocesses on the target
  host.
- The neighbouring `list_agent_models` caches per `(os_user, daemon, agent)` for exactly this
  reason, and is the shape to copy.
- **Latent** only while the Hosts row does not mount the tooling section. Worth resolving before it
  does, not after.
