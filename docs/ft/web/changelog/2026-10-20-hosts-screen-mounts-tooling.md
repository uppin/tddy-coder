# 2026-10-20 — Hosts screen shows each online host's tooling and remote desktop

Each online row on the Hosts screen asks its host for its tooling and renders it, including the
remote-desktop section with its Connect action. An offline row asks nothing, and a host that does not
answer reads "Tooling could not be read" instead of a blank cell. The daemon keeps each answer for 30
seconds and shares one probe between concurrent callers, so a screen listing many hosts does not
fan out subprocesses on every visit. See [Hosts screen tooling](../hosts-screen-tooling.md).
