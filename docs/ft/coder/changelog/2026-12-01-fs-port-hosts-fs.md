# 2026-12-01 — Hosts, VMs, git and projects go through the Fs port

The production disk I/O of `tddy-vm`, `tddy-host-service`, `tddy-git`, `tddy-projects`,
`tddy-discovery`, `tddy-github`, `tddy-git-clone` and `tddy-credential-sync` goes
through the `tddy-fs` port (refactor, no behaviour change): the VM library, host
stores, project registry, discovery executors and the async build pipelines run behind
`<fn>_on` seams, held handles and an `AsyncFs` with `DiskFs` only at composition
roots. No `Fs` trait method was added; descriptor acts and streamed image reads stay
on `libc` and are recorded in the changeset.
