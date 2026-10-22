# 2026-10-22 — an extension language that several declared servers qualify for: the tie-break is undecided

**Category:** Open design question
**Found in:** `#reshape` 22/24 (`extension-language-server`, PR #681), open since its contract (rule R1)
**Owner:** the developer's decision; then a follow-up PR in `tddy-zed-extensions`

## What happens

`ServerChoice::for_language` (`packages/tddy-zed-extensions/src/server.rs`) takes the **first** manifest
`[language_servers.<id>]` entry, by id, whose `language` / `languages` names the language (and, for an opt-in
language, that `languages.<Name>.language_servers` in `<extensions dir>/settings.json` names). A second qualifying
server is never started. No test pins the choice, and no fixture declares two servers for one language.

## Why it was not decided in #681

The developer left R1's tie-break open at the contract (2026-10-20). Zed itself runs every qualifying server, ordered
by `languages.<Name>.language_servers` (with `...` standing for "the rest"); tddy runs one server per language and
root (`LspKey { root, language }`), so adopting Zed's rule means choosing which one answers a rename.

## Wanted

A decision — (a) the first server `languages.<Name>.language_servers` lists, else the first by id (Zed's order, one
server); (b) refuse a language several servers qualify for until a setting names one; (c) keep first-by-id — then a
`server::tests` case with two qualifying entries that pins it, and the rule written into
`docs/ft/coder/zed-extensions.md` § Language servers.
