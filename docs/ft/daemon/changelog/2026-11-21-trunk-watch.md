# 2026-11-21 — New trunk commits reach every live orchestrator as activities

The daemon watches each project's remote trunk for its orchestrators: every orchestrator-role session
is subscribed when it starts or resumes, one conditional poll per project serves all of them, and the
polling stops when none is live. Each new commit — or a force-push rewriting the trunk — becomes a
`trunk_commit` activity of every subscribed orchestrator's own session, which it reads through a
watch of its own session of the kind `trunk_commit` (the one self-watch allowed) and is woken by when
idle. A Local gh project is read through the host's `gh`, any other through the project's GitHub
account. Operator surfaces never show these records. See
[Project concept § Watching the trunk](../project-concept.md#watching-the-trunk).
