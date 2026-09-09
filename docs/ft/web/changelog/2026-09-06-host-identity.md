# 2026-09-06 — Each host's git identity and GitHub CLI status

Every Hosts row reports what that host has **installed and configured**: the git `user.name` /
`user.email` its commits would carry, and whether the GitHub CLI there is installed, logged out, or
authenticated — and as which login. Until now nothing in tddy said either, so a host committing
under the wrong identity, or one whose `gh` is logged out, announced itself only through the
failure it caused.

Six states are kept distinguishable rather than collapsed. **"Could not check" is never rendered as
"not configured"**: one is a host to go and fix, the other is a probe to go and fix, and an empty
string in place of either is a fabricated fact an operator acts on. `gh auth status` has no stable
output contract, so output the daemon does not recognise is reported as a probe failure — telling an
operator that an authenticated host is logged out is the worse of the two errors.

The `gh` login shown is **the host's**. tddy carries three GitHub identities that can disagree — the
web session's user in the avatar, a `GITHUB_TOKEN` in some environment, and the host's own `gh`
login — so the cell says which one it is showing rather than leaving a bare login next to the
operator's own name.

The screen reads and changes nothing: two fixed probes, no "configure git" action, and no general
"run this on host X" primitive.

See [`hosts-screen-tooling.md`](../hosts-screen-tooling.md) and
[`hosts-screen.md`](../hosts-screen.md).
