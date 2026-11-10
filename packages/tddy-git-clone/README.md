# tddy-git-clone

The clone helper a supervised daemon runs as the session user.

```text
tddy-git-clone --git <git> -- <url> <destination>
```

On a host run by `tddy-supervisor` the daemon cannot become another user, and the supervisor knows
nothing about git. So a clone is one `SpawnStreamed` of this binary as the session user, and
everything the clone does to the filesystem is done by that user:

- runs `<git> clone --progress -- <url> <destination>.partial-<random>`, a sibling of the
  destination so the rename stays on one filesystem;
- on success, renames the partial directory to `<destination>`;
- on failure or a stop, removes the partial directory, so a destination either does not exist or
  holds a complete clone;
- refuses a destination that already exists, and leaves it alone.

Git's stderr (its progress and its own explanation of a failure) passes through on this process's
stderr. The supervisor streams it line by line, and the daemon parses the progress. The helper exits
with git's exit code, or non-zero when it refused or could not rename. It has no dependencies
outside `std`.

## Who runs it

Not a human: `tddy-spawn`'s `plan_supervised_clone` names it, resolved beside the daemon. The helper
must sit under the supervisor's `exec_roots`. It ships with `./release`, `./install`, `./publish.sh`
and `scripts/dev-runtime-binaries.sh` next to `tddy-daemon`. It is not part of `./install --desktop`,
because Tddy Desktop runs no supervisor.

The daemon's side: [`tddy-spawn/docs/clone-run.md`](../tddy-spawn/docs/clone-run.md#supervised-clones).
Supervisor: [`docs/ft/supervisor/tddy-supervisor.md`](../../docs/ft/supervisor/tddy-supervisor.md#streamed-spawns-and-clones).
