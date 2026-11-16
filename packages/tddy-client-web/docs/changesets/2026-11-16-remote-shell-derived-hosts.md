# 2026-11-16 — The derived host-directory source

**Type:** Feature · `#remote-shell` 5/7, PR [#919](https://github.com/uppin/tddy-coder/pull/919)

The host directory gains a `derived` source (`src/hostDirectory/derivedSource.ts`): every connected daemon is asked (`ListDerivedHosts`) for the hosts it reaches through a remote shell, and each answer becomes a `HostDescriptor` carrying `parent` and `target`, carried through `DaemonHost` (`daemonHost.ts`) and the clients port. Only a connected parent is asked; one parent's failed listing is recorded against that parent and does not hide another's hosts. The web app registers the source inside its connection providers (`tddy-web`'s `DerivedHostDirectorySources`).

Verification: `derivedSource.test.ts` 7/7 (each connected daemon's hosts with their parent, nothing for an offline parent, one parent's failure keeping another's hosts).
