# 2026-10-04 — Projects use the accounts you assign them

- **Assign accounts to a project.** On the Projects screen each project shows, for every provider
  you have an account at, which account the project uses. Choose one, or leave it unassigned. A
  project uses one account per provider, and the assignment applies on every host that has the
  project.
- **Unassigned means no account.** A project with no account assigned for a provider has no
  credentials for it. Nothing falls back to your own login or to the only account in your vault.
- **Three clear states.** Assigned, no account assigned, and unavailable on this host (assigned, but
  this host's vault does not hold that account).

See [project-concept.md](../project-concept.md#account-assignment) and
[projects-screen-multi-host.md](../../web/projects-screen-multi-host.md#accounts).
