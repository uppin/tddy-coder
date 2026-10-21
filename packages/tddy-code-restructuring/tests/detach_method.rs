//! `detach_method` turns the members of one inherent `impl` into free functions over a named
//! parameter and re-points their callers — `#reshape` 15/24, rules R1–R6.
//!
//! Library level: the operation is resolved against **recorded server answers** rather than a live
//! rust-analyzer. A test hands it the outline of the file (`textDocument/documentSymbol`, read off
//! the fixture text in the shape the server answers), the reference set of each member
//! (`textDocument/references`, the needles the test names) and the type of each receiver
//! (`textDocument/hover`), so every rule is decided in milliseconds. The two live tests in
//! `detach_method_acceptance.rs` prove the same edits compile.

use std::collections::BTreeMap;

use serde_json::{json, Value};
use tddy_code_restructuring::backends::rust::detach_method::{self, DetachServer};
use tddy_code_restructuring::crate_move::Reference;
use tddy_code_restructuring::registry::Workspace;
use tddy_code_restructuring::{
    Anchor, FileEdit, Overlay, Plan, Position, Range, RefactorOp, Resolution, RestructureError,
};

const HOST: &str = "src/host.rs";
const TICK: &str = "src/tick.rs";
const LIB: &str = "src/lib.rs";

// ---------------------------------------------------------------------------------------------
// The workspace and the recorded server
// ---------------------------------------------------------------------------------------------

/// An `app` crate on disk, read through an empty overlay as a run reads it.
struct AnApp {
    root: tempfile::TempDir,
    overlay: Overlay,
}

/// An `app` crate holding exactly `files`.
fn an_app_holding(files: &[(&str, &str)]) -> AnApp {
    let app = AnApp {
        root: tempfile::tempdir().expect("a temporary directory"),
        overlay: Overlay::default(),
    };
    for (path, text) in files {
        let absolute = app.root.path().join(path);
        std::fs::create_dir_all(absolute.parent().expect("a parent")).expect("directories");
        std::fs::write(absolute, text).expect("the file is written");
    }
    app
}

impl AnApp {
    fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.root.path().join(path)).expect("the file is read")
    }

    fn workspace(&self) -> Workspace<'_> {
        Workspace {
            root: self.root.path(),
            overlay: &self.overlay,
        }
    }
}

/// The answers a language server would give about one fixture, recorded by the test.
#[derive(Default)]
struct ARecordedServer {
    /// Per member name, every place that names it: `(file, needle)`, the member's name being the
    /// needle's last occurrence of it.
    references: BTreeMap<String, Vec<(String, String)>>,
    /// Per `(file, receiver)`, the type the hover states for the receiver.
    types: BTreeMap<(String, String), String>,
}

/// A server that reports no caller of anything.
fn a_server() -> ARecordedServer {
    ARecordedServer::default()
}

impl ARecordedServer {
    /// `member` is named in `file` where `needle` is written (once per call of this).
    fn naming(mut self, member: &str, file: &str, needle: &str) -> Self {
        self.references
            .entry(member.to_string())
            .or_default()
            .push((file.to_string(), needle.to_string()));
        self
    }

    /// The receiver written `receiver` in `file` has the type `ty`.
    fn typing(mut self, file: &str, receiver: &str, ty: &str) -> Self {
        self.types
            .insert((file.to_string(), receiver.to_string()), ty.to_string());
        self
    }
}

impl DetachServer for ARecordedServer {
    fn span(
        &mut self,
        op: &RefactorOp,
        workspace: &Workspace<'_>,
    ) -> Result<Range, RestructureError> {
        let Anchor::Items { file, items, .. } = &op.anchor else {
            panic!("these tests anchor by `items`: {:?}", op.anchor);
        };
        let text = workspace.read(file)?;
        let lines: Vec<(u32, u32)> = items
            .iter()
            .map(|item| {
                let written = item.to_string();
                let name = written.rsplit("::").next().expect("a last segment");
                member_lines(&text, name)
            })
            .collect();
        let start = lines
            .iter()
            .map(|(first, _)| *first)
            .min()
            .expect("a member");
        let end = lines.iter().map(|(_, last)| *last).max().expect("a member");
        let end_col = text.lines().nth(end as usize - 1).map_or(0, str::len) as u32 + 1;
        Ok(Range {
            start: Position {
                line: start,
                col: 1,
            },
            end: Position {
                line: end,
                col: end_col,
            },
        })
    }

    fn outline(
        &mut self,
        workspace: &Workspace<'_>,
        file: &str,
    ) -> Result<Value, RestructureError> {
        Ok(the_outline_of(&workspace.read(file)?))
    }

    fn references(
        &mut self,
        workspace: &Workspace<'_>,
        file: &str,
        at: Position,
    ) -> Result<Vec<Reference>, RestructureError> {
        let name = word_at(&workspace.read(file)?, at);
        let mut found = Vec::new();
        for (path, needle) in self.references.get(&name).into_iter().flatten() {
            let text = workspace.read(path)?;
            let offset = text
                .find(needle.as_str())
                .unwrap_or_else(|| panic!("`{needle}` is not written in {path}"));
            let within = needle
                .rfind(name.as_str())
                .expect("the needle names the member");
            found.push(Reference {
                path: path.clone(),
                at: position_of(&text, offset + within),
            });
        }
        Ok(found)
    }

    fn hover(
        &mut self,
        workspace: &Workspace<'_>,
        file: &str,
        at: Position,
    ) -> Result<Value, RestructureError> {
        let receiver = word_at(&workspace.read(file)?, at);
        let ty = self
            .types
            .get(&(file.to_string(), receiver.clone()))
            .unwrap_or_else(|| panic!("no type recorded for `{receiver}` in {file}"));
        Ok(json!({
            "contents": { "kind": "markdown", "value": format!("```rust\n{receiver}: {ty}\n```") }
        }))
    }
}

/// The `documentSymbol` answer rust-analyzer gives for `text`: every top-level `impl` (kind 19,
/// labelled as its header reads) with its members as children — a method (6), an associated
/// function (12), a const (14), a type (26) — zero-based, as the server reports them.
fn the_outline_of(text: &str) -> Value {
    let lines: Vec<&str> = text.lines().collect();
    let mut blocks = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        let header = lines[at];
        if !header.starts_with("impl") {
            at += 1;
            continue;
        }
        // A `where` clause puts the block's `{` on a line of its own.
        let open = (at..lines.len())
            .find(|&line| lines[line].ends_with('{'))
            .expect("the block opens");
        let close = (open..lines.len())
            .find(|&line| lines[line] == "}")
            .expect("the block closes");
        let children: Vec<Value> = (open + 1..close)
            .filter_map(|line| a_member_at(&lines, line))
            .collect();
        blocks.push(json!({
            "name": header.trim_end_matches('{').trim(),
            "kind": 19,
            "range": a_range(at, 0, close, 1),
            "selectionRange": a_range(at, 0, at, header.len()),
            "children": children,
        }));
        at = close + 1;
    }
    Value::Array(blocks)
}

/// The outline child for a member declared on `line`, if one is.
fn a_member_at(lines: &[&str], line: usize) -> Option<Value> {
    let written = lines[line];
    if !written.starts_with("    ") || written.starts_with("     ") {
        return None;
    }
    let (keyword, kind) = [("fn ", 0), ("const ", 14), ("type ", 26)]
        .into_iter()
        .find(|(keyword, _)| written.contains(keyword))?;
    let name_at = written.find(keyword)? + keyword.len();
    let name: String = written[name_at..]
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    let kind = match kind {
        0 if written.contains("self") => 6,
        0 => 12,
        other => other,
    };
    let last = if written.ends_with(';') {
        line
    } else {
        (line..lines.len())
            .find(|&end| lines[end] == "    }")
            .expect("the member closes")
    };
    Some(json!({
        "name": name,
        "kind": kind,
        "range": a_range(line, 4, last, lines[last].len()),
        "selectionRange": a_range(line, name_at, line, name_at + name.len()),
    }))
}

fn a_range(start_line: usize, start_col: usize, end_line: usize, end_col: usize) -> Value {
    json!({
        "start": { "line": start_line, "character": start_col },
        "end": { "line": end_line, "character": end_col },
    })
}

/// The one-based first line (attached doc comments and attributes included) and last line of the
/// member `name` of `text`.
fn member_lines(text: &str, name: &str) -> (u32, u32) {
    let outline = the_outline_of(text);
    let member = outline
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|block| block["children"].as_array().cloned().unwrap_or_default())
        .find(|child| child["name"] == name)
        .unwrap_or_else(|| panic!("no member `{name}` in the outline"));
    let declared = member["range"]["start"]["line"].as_u64().expect("a line") as usize;
    let last = member["range"]["end"]["line"].as_u64().expect("a line") as u32 + 1;
    let lines: Vec<&str> = text.lines().collect();
    let mut first = declared;
    while first > 0 {
        let above = lines[first - 1].trim_start();
        if above.starts_with("///") || above.starts_with("#[") {
            first -= 1;
        } else {
            break;
        }
    }
    (first as u32 + 1, last)
}

/// The identifier written at the one-based `at` of `text`.
fn word_at(text: &str, at: Position) -> String {
    let line = text
        .lines()
        .nth(at.line as usize - 1)
        .expect("the line exists");
    line.chars()
        .skip(at.col as usize - 1)
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect()
}

/// The one-based position of a byte offset, as the server reports a reference.
fn position_of(text: &str, offset: usize) -> Position {
    let before = &text[..offset];
    Position {
        line: before.matches('\n').count() as u32 + 1,
        col: before
            .rsplit('\n')
            .next()
            .map_or(0, |line| line.chars().count()) as u32
            + 1,
    }
}

// ---------------------------------------------------------------------------------------------
// The operation, and what it leaves
// ---------------------------------------------------------------------------------------------

/// A `detach_method` of `members` of `src/host.rs` (each `Host::bump`, as the `app::host` module
/// names it) over the parameter `name`.
fn a_detach_of(members: &[&str], name: &str) -> RefactorOp {
    let items: Vec<String> = members
        .iter()
        .map(|member| format!("app::host::{member}"))
        .collect();
    let fingerprints: Vec<&str> = members.iter().map(|_| "sha256:a").collect();
    serde_json::from_value(json!({
        "op": "detach_method",
        "anchor": { "kind": "items", "file": HOST, "items": items, "fingerprints": fingerprints },
        "name": name,
    }))
    .expect("a `detach_method` operation deserializes")
}

/// Resolve the detach of `members` over `session`, failing the test when it is refused.
fn detaching(app: &AnApp, server: &mut ARecordedServer, members: &[&str]) -> Resolution {
    detach_method::resolve(server, &a_detach_of(members, "session"), &app.workspace())
        .unwrap_or_else(|refusal| panic!("the detach was refused: {refusal}"))
}

/// What `path` reads as once `resolution`'s edits to it are applied.
fn after(app: &AnApp, resolution: &Resolution, path: &str) -> String {
    let edits: Vec<_> = resolution
        .edit
        .changes
        .iter()
        .filter_map(|change| match change {
            FileEdit::Change {
                path: changed,
                edits,
            } if changed == path => Some(edits.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    tddy_code_restructuring::apply::edited(app.read(path), &edits).expect("the edits apply")
}

/// What `check --deep` reports for the detach of `members` over `session`, and what `resolve`
/// refuses with — the two must agree, and a refused resolve builds no edit.
fn the_refusals_of_detaching(
    app: &AnApp,
    server: &mut ARecordedServer,
    members: &[&str],
) -> (Vec<String>, String) {
    let op = a_detach_of(members, "session");
    let findings =
        detach_method::check(server, &op, &app.workspace()).expect("the deep check runs");
    let refusal = match detach_method::resolve(server, &op, &app.workspace()) {
        Err(refusal) => refusal.to_string(),
        Ok(resolution) => panic!("the detach was resolved: {resolution:?}"),
    };
    (findings, refusal)
}

/// Assert the deep check and the resolve both name `location` (`file:line`) and `reason`.
fn assert_refused_at(refused: &(Vec<String>, String), location: &str, reason: &str) {
    let (findings, refusal) = refused;
    assert!(
        findings
            .iter()
            .any(|finding| finding.contains(location) && finding.contains(reason)),
        "the deep check does not name {location} and `{reason}`: {findings:?}"
    );
    assert!(
        refusal.contains(location) && refusal.contains(reason),
        "the resolve does not refuse naming {location} and `{reason}`: {refusal}"
    );
}

// ---------------------------------------------------------------------------------------------
// The fixture most tests detach
// ---------------------------------------------------------------------------------------------

const A_HOST_WITH_TWO_MEMBERS: &str = concat!(
    "pub struct Host {\n",                                   // 1
    "    pub(crate) n: u32,\n",                              // 2
    "}\n",                                                   // 3
    "\n",                                                    // 4
    "impl Host {\n",                                         // 5
    "    pub(crate) fn bump(&mut self, by: u32) -> u32 {\n", // 6
    "        self.n += by;\n",                               // 7
    "        self.n\n",                                      // 8
    "    }\n",                                               // 9
    "\n",                                                    // 10
    "    pub(crate) fn total(&self) -> u32 {\n",             // 11
    "        self.n\n",                                      // 12
    "    }\n",                                               // 13
    "}\n",                                                   // 14
);

const THE_HOST_WITH_TWO_FUNCTIONS: &str = concat!(
    "pub struct Host {\n",
    "    pub(crate) n: u32,\n",
    "}\n",
    "\n",
    "pub(crate) fn bump(session: &mut Host, by: u32) -> u32 {\n",
    "    session.n += by;\n",
    "    session.n\n",
    "}\n",
    "\n",
    "pub(crate) fn total(session: &Host) -> u32 {\n",
    "    session.n\n",
    "}\n",
);

/// The crate root calls `bump` through the module it already declares.
const A_ROOT_THAT_BUMPS: &str = concat!(
    "pub mod host;\n",
    "pub mod tick;\n",
    "\n",
    "pub fn run(h: &mut host::Host) -> u32 {\n",
    "    h.bump(1)\n",
    "}\n",
);

/// A sibling module calls both members on a reference, and binds only the type.
const A_TICK_THAT_CALLS_BOTH: &str = concat!(
    "use crate::host::Host;\n",
    "\n",
    "pub fn tick(h: &mut Host) -> u32 {\n",
    "    h.bump(2) + h.total()\n",
    "}\n",
);

/// The app of [`A_HOST_WITH_TWO_MEMBERS`], called from the root and from `tick`.
fn an_app_calling_both_members() -> AnApp {
    an_app_holding(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        (LIB, A_ROOT_THAT_BUMPS),
        (HOST, A_HOST_WITH_TWO_MEMBERS),
        (TICK, A_TICK_THAT_CALLS_BOTH),
    ])
}

/// What the server reports about [`an_app_calling_both_members`]: three calls, every receiver a
/// `&mut Host`.
fn the_server_of_both_members() -> ARecordedServer {
    a_server()
        .naming("bump", LIB, "h.bump(1)")
        .naming("bump", TICK, "h.bump(2)")
        .naming("total", TICK, "h.total()")
        .typing(LIB, "h", "&mut host::Host")
        .typing(TICK, "h", "&mut Host")
}

/// The app of `host` and `tick`, with a root that declares both modules.
fn an_app_of(host: &str, tick: &str) -> AnApp {
    an_app_holding(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        (LIB, "pub mod host;\npub mod tick;\n"),
        (HOST, host),
        (TICK, tick),
    ])
}

// ---------------------------------------------------------------------------------------------
// R1–R4: the functions and the callers
// ---------------------------------------------------------------------------------------------

#[test]
fn detaches_two_members_and_removes_the_emptied_block() {
    // Given a block of two members, `&mut self` and `&self`, called from two files
    let app = an_app_calling_both_members();
    let mut server = the_server_of_both_members();

    // When both are detached over `session`
    let resolution = detaching(&app, &mut server, &["Host::bump", "Host::total"]);

    // Then each is a function over `session` in member order, with its visibility, and the block
    // it emptied is gone
    assert_eq!(after(&app, &resolution, HOST), THE_HOST_WITH_TWO_FUNCTIONS);
}

#[test]
fn re_points_a_call_in_another_module_and_adds_the_use() {
    // Given `tick`, which binds `Host` but not the `host` module, and the root, which declares it
    let app = an_app_calling_both_members();
    let mut server = the_server_of_both_members();

    // When both members are detached
    let resolution = detaching(&app, &mut server, &["Host::bump", "Host::total"]);

    // Then `tick` calls them through `host` with a `use` of it added, and the root, which already
    // binds `host`, gains no `use`
    let tick = after(&app, &resolution, TICK);
    assert!(
        tick.contains("use crate::host;\n")
            && tick.contains("    host::bump(h, 2) + host::total(h)\n"),
        "`tick` does not call the functions through their module:\n{tick}"
    );
    assert_eq!(
        after(&app, &resolution, LIB),
        A_ROOT_THAT_BUMPS.replace("    h.bump(1)\n", "    host::bump(h, 1)\n")
    );
}

#[test]
fn borrows_a_place_receiver() {
    // Given both members called on an owned `Host`
    let app = an_app_of(
        A_HOST_WITH_TWO_MEMBERS,
        concat!(
            "use crate::host::Host;\n",
            "\n",
            "pub fn fresh(n: u32) -> u32 {\n",
            "    let mut h = Host { n };\n",
            "    h.bump(1) + h.total()\n",
            "}\n",
        ),
    );
    let mut server = a_server()
        .naming("bump", TICK, "h.bump(1)")
        .naming("total", TICK, "h.total()")
        .typing(TICK, "h", "Host");

    // When both are detached
    let resolution = detaching(&app, &mut server, &["Host::bump", "Host::total"]);

    // Then the place is lent at each call as the member's receiver was: mutably, then shared
    let tick = after(&app, &resolution, TICK);
    assert!(
        tick.contains("    host::bump(&mut h, 1) + host::total(&h)\n"),
        "the owned receiver was not borrowed at the calls:\n{tick}"
    );
}

#[test]
fn passes_a_reference_receiver_as_it_is() {
    // Given calls on a `&mut Host` and on a `&Host` with a named lifetime
    let app = an_app_of(
        A_HOST_WITH_TWO_MEMBERS,
        concat!(
            "use crate::host::Host;\n",
            "\n",
            "pub fn tick<'a>(h: &mut Host, r: &'a Host) -> u32 {\n",
            "    h.bump(3) + r.total()\n",
            "}\n",
        ),
    );
    let mut server = a_server()
        .naming("bump", TICK, "h.bump(3)")
        .naming("total", TICK, "r.total()")
        .typing(TICK, "h", "&mut Host")
        .typing(TICK, "r", "&'a Host");

    // When both are detached
    let resolution = detaching(&app, &mut server, &["Host::bump", "Host::total"]);

    // Then each reference is the first argument as written, borrowed no further
    let tick = after(&app, &resolution, TICK);
    assert!(
        tick.contains("    host::bump(h, 3) + host::total(r)\n"),
        "a reference receiver was re-borrowed or dropped:\n{tick}"
    );
}

#[test]
fn rewrites_self_and_capital_self_in_the_body() {
    // Given a documented member reading `self`, `Self::LIMIT` and `Self::step`, beside a const that
    // stays on the type
    let host = concat!(
        "pub struct Host {\n",                            // 1
        "    pub(crate) n: u32,\n",                       // 2
        "}\n",                                            // 3
        "\n",                                             // 4
        "impl Host {\n",                                  // 5
        "    pub(crate) const LIMIT: u32 = 3;\n",         // 6
        "\n",                                             // 7
        "    /// Capped at the limit.\n",                 // 8
        "    #[inline]\n",                                // 9
        "    pub(crate) fn capped(&mut self) -> u32 {\n", // 10
        "        // self.n is the count\n",               // 11
        "        self.n = Self::step(self.n);\n",         // 12
        "        self.n.min(Self::LIMIT)\n",              // 13
        "    }\n",                                        // 14
        "\n",                                             // 15
        "    fn step(by: u32) -> u32 {\n",                // 16
        "        by + 1\n",                               // 17
        "    }\n",                                        // 18
        "}\n",                                            // 19
    );
    let app = an_app_of(host, "pub fn tick() {}\n");
    let mut server = a_server().naming("step", HOST, "Self::step(self.n)");

    // When `capped` and `step` are detached together
    let resolution = detaching(&app, &mut server, &["Host::capped", "Host::step"]);

    // Then `self` is `session` and `Self` is `Host` in code, the comment is untouched, the trivia
    // travels, `Self::step` is a plain call, and the block keeps the const
    assert_eq!(
        after(&app, &resolution, HOST),
        concat!(
            "pub struct Host {\n",
            "    pub(crate) n: u32,\n",
            "}\n",
            "\n",
            "impl Host {\n",
            "    pub(crate) const LIMIT: u32 = 3;\n",
            "}\n",
            "\n",
            "/// Capped at the limit.\n",
            "#[inline]\n",
            "pub(crate) fn capped(session: &mut Host) -> u32 {\n",
            "    // self.n is the count\n",
            "    session.n = step(session.n);\n",
            "    session.n.min(Host::LIMIT)\n",
            "}\n",
            "\n",
            "fn step(by: u32) -> u32 {\n",
            "    by + 1\n",
            "}\n",
        )
    );
}

// ---------------------------------------------------------------------------------------------
// R5: what cannot be detached, refused before any edit, each naming its file and line
// ---------------------------------------------------------------------------------------------

const A_HOST_WITH_MEMBERS_THAT_CANNOT_LEAVE: &str = concat!(
    "pub struct Host {\n",                                  // 1
    "    pub(crate) n: u32,\n",                             // 2
    "}\n",                                                  // 3
    "\n",                                                   // 4
    "pub struct Wrap<T> {\n",                               // 5
    "    pub(crate) inner: T,\n",                           // 6
    "}\n",                                                  // 7
    "\n",                                                   // 8
    "pub trait Tally {\n",                                  // 9
    "    fn count(&self) -> u32;\n",                        // 10
    "}\n",                                                  // 11
    "\n",                                                   // 12
    "impl Host {\n",                                        // 13
    "    pub(crate) const LIMIT: u32 = 3;\n",               // 14
    "\n",                                                   // 15
    "    pub(crate) fn with(mut self, n: u32) -> Self {\n", // 16
    "        self.n = n;\n",                                // 17
    "        self\n",                                       // 18
    "    }\n",                                              // 19
    "\n",                                                   // 20
    "    pub fn shown(&self) -> u32 {\n",                   // 21
    "        self.n\n",                                     // 22
    "    }\n",                                              // 23
    "}\n",                                                  // 24
    "\n",                                                   // 25
    "impl<T> Wrap<T> {\n",                                  // 26
    "    pub(crate) fn get(&self) -> &T {\n",               // 27
    "        &self.inner\n",                                // 28
    "    }\n",                                              // 29
    "}\n",                                                  // 30
    "\n",                                                   // 31
    "impl Wrap<u32>\n",                                     // 32
    "where\n",                                              // 33
    "    u32: Copy,\n",                                     // 34
    "{\n",                                                  // 35
    "    pub(crate) fn copied(&self) -> u32 {\n",           // 36
    "        self.inner\n",                                 // 37
    "    }\n",                                              // 38
    "}\n",                                                  // 39
    "\n",                                                   // 40
    "impl Tally for Host {\n",                              // 41
    "    fn count(&self) -> u32 {\n",                       // 42
    "        self.n\n",                                     // 43
    "    }\n",                                              // 44
    "}\n",                                                  // 45
);

fn an_app_whose_members_cannot_leave() -> AnApp {
    an_app_of(A_HOST_WITH_MEMBERS_THAT_CANNOT_LEAVE, "pub fn tick() {}\n")
}

#[test]
fn refuses_a_member_that_takes_self_by_value() {
    // Given a builder that consumes its receiver
    let app = an_app_whose_members_cannot_leave();

    // When it is detached
    let refused = the_refusals_of_detaching(&app, &mut a_server(), &["Host::with"]);

    // Then it is refused at its line: a function over a reference cannot take it
    assert_refused_at(
        &refused,
        "src/host.rs:16",
        "`Host::with` takes `self` by value: a function over a reference cannot take it",
    );
}

#[test]
fn refuses_a_member_of_a_generic_impl_or_of_one_with_a_where_clause() {
    // Given a member of `impl<T> Wrap<T>` and one of an `impl` with a `where` clause
    let app = an_app_whose_members_cannot_leave();

    // When each is detached
    let generic = the_refusals_of_detaching(&app, &mut a_server(), &["Wrap::get"]);
    let bounded = the_refusals_of_detaching(&app, &mut a_server(), &["Wrap::copied"]);

    // Then each is refused at its block's header, which a free function would have to restate
    let reason = "carries generic parameters or a `where` clause";
    assert_refused_at(&generic, "src/host.rs:26", reason);
    assert_refused_at(&bounded, "src/host.rs:32", reason);
}

#[test]
fn refuses_a_member_of_a_trait_impl() {
    // Given a member of `impl Tally for Host`
    let app = an_app_whose_members_cannot_leave();

    // When it is detached
    let refused = the_refusals_of_detaching(&app, &mut a_server(), &["<Host as Tally>::count"]);

    // Then it is refused: trait members stay methods
    assert_refused_at(
        &refused,
        "src/host.rs:42",
        "`count` is a member of `impl Tally for Host`: trait members stay methods",
    );
}

#[test]
fn refuses_a_pub_member() {
    // Given a `pub` member, which callers outside the workspace may name
    let app = an_app_whose_members_cannot_leave();

    // When it is detached
    let refused = the_refusals_of_detaching(&app, &mut a_server(), &["Host::shown"]);

    // Then it is refused at its line
    assert_refused_at(
        &refused,
        "src/host.rs:21",
        "`Host::shown` is `pub` and may have callers outside the workspace",
    );
}

#[test]
fn refuses_an_associated_const() {
    // Given an associated const in the anchored run
    let app = an_app_whose_members_cannot_leave();

    // When it is detached
    let refused = the_refusals_of_detaching(&app, &mut a_server(), &["Host::LIMIT"]);

    // Then it is refused: there is nothing to detach
    assert_refused_at(
        &refused,
        "src/host.rs:14",
        "`Host::LIMIT` is an associated const or type: there is nothing to detach",
    );
}

const A_HOST_WHOSE_NAMES_COLLIDE: &str = concat!(
    "pub struct Host {\n",                           // 1
    "    pub(crate) n: u32,\n",                      // 2
    "}\n",                                           // 3
    "\n",                                            // 4
    "impl Host {\n",                                 // 5
    "    pub(crate) fn shadowing(&self) -> u32 {\n", // 6
    "        let session = self.n;\n",               // 7
    "        session + 1\n",                         // 8
    "    }\n",                                       // 9
    "\n",                                            // 10
    "    pub(crate) fn taken(&self) -> u32 {\n",     // 11
    "        self.n\n",                              // 12
    "    }\n",                                       // 13
    "}\n",                                           // 14
    "\n",                                            // 15
    "pub(crate) fn taken() -> u32 {\n",              // 16
    "    0\n",                                       // 17
    "}\n",                                           // 18
);

#[test]
fn refuses_a_parameter_name_the_body_already_writes() {
    // Given a member whose body already binds `session`
    let app = an_app_of(A_HOST_WHOSE_NAMES_COLLIDE, "pub fn tick() {}\n");

    // When it is detached over `session`
    let refused = the_refusals_of_detaching(&app, &mut a_server(), &["Host::shadowing"]);

    // Then it is refused where the body writes it: the parameter would shadow it
    assert_refused_at(
        &refused,
        "src/host.rs:7",
        "`session` is already written at line 7 of `shadowing`: the parameter would shadow it",
    );
}

#[test]
fn refuses_a_function_name_the_module_already_declares() {
    // Given a member named like a function its module already declares
    let app = an_app_of(A_HOST_WHOSE_NAMES_COLLIDE, "pub fn tick() {}\n");

    // When it is detached
    let refused = the_refusals_of_detaching(&app, &mut a_server(), &["Host::taken"]);

    // Then it is refused at the member: the function would be E0428
    assert_refused_at(
        &refused,
        "src/host.rs:11",
        "already declares or imports `taken`",
    );
}

const A_HOST_OF_ONE_TOTAL: &str = concat!(
    "pub struct Host {\n",
    "    pub(crate) n: u32,\n",
    "}\n",
    "\n",
    "impl Host {\n",
    "    pub(crate) fn total(&self) -> u32 {\n",
    "        self.n\n",
    "    }\n",
    "}\n",
);

#[test]
fn refuses_a_call_whose_receiver_is_a_box_naming_its_type() {
    // Given a call of the member on a `Box<Host>`
    let app = an_app_of(
        A_HOST_OF_ONE_TOTAL,
        concat!(
            "use crate::host::Host;\n",             // 1
            "\n",                                   // 2
            "pub fn tick(b: Box<Host>) -> u32 {\n", // 3
            "    b.total()\n",                      // 4
            "}\n",                                  // 5
        ),
    );
    let mut server = a_server()
        .naming("total", TICK, "b.total()")
        .typing(TICK, "b", "Box<Host>");

    // When the member is detached
    let refused = the_refusals_of_detaching(&app, &mut server, &["Host::total"]);

    // Then the call is refused naming the receiver and its type
    assert_refused_at(
        &refused,
        "src/tick.rs:4",
        "has receiver `b` of type `Box<Host>`, which is neither `Host` nor a reference to it",
    );
}

#[test]
fn refuses_a_call_inside_a_macro_invocation() {
    // Given a call of the member written inside `format!`
    let app = an_app_of(
        A_HOST_OF_ONE_TOTAL,
        concat!(
            "use crate::host::Host;\n",             // 1
            "\n",                                   // 2
            "pub fn shown(h: &Host) -> String {\n", // 3
            "    format!(\"{}\", h.total())\n",     // 4
            "}\n",                                  // 5
        ),
    );
    let mut server = a_server()
        .naming("total", TICK, "h.total()")
        .typing(TICK, "h", "&Host");

    // When the member is detached
    let refused = the_refusals_of_detaching(&app, &mut server, &["Host::total"]);

    // Then the call is refused: this run does not rewrite inside a macro
    assert_refused_at(
        &refused,
        "src/tick.rs:4",
        "is inside a macro invocation, which this run does not rewrite",
    );
}

// ---------------------------------------------------------------------------------------------
// R5 (plain `check`): the plan line alone, no server
// ---------------------------------------------------------------------------------------------

const A_HEADER: &str = "{\"v\":1,\"snapshot\":{}}";
const THE_ITEMS: &str = r#"{"kind":"items","file":"src/host.rs","items":["app::host::Host::bump"],"fingerprints":["sha256:a"]}"#;

/// What reading the one-line plan `{"op":"detach_method","anchor":<anchor><extra>}` refuses with.
fn the_refusal_of_reading(anchor: &str, extra: &str) -> String {
    let plan = format!("{A_HEADER}\n{{\"op\":\"detach_method\",\"anchor\":{anchor}{extra}}}\n");
    match Plan::parse(&plan) {
        Err(refusal) => refusal.to_string(),
        Ok(_) => panic!("the plan was expected to be refused, and parsed:\n{plan}"),
    }
}

#[test]
fn plain_check_reports_a_missing_name_without_a_server() {
    // Given a detach with no `name`, and one naming `self`
    // When each plan line is read, before any server exists
    let missing = the_refusal_of_reading(THE_ITEMS, "");
    let receiver = the_refusal_of_reading(THE_ITEMS, ",\"name\":\"self\"");

    // Then the missing parameter and the reserved one are each refused as a malformed plan
    assert!(
        missing.starts_with("plan is malformed:")
            && missing.contains("`detach_method` needs `name`: the parameter the receiver becomes"),
        "{missing}"
    );
    assert!(
        receiver.starts_with("plan is malformed:")
            && receiver.contains("`name` must be one identifier other than `self`; `self` is not"),
        "{receiver}"
    );
}

#[test]
fn plain_check_refuses_an_anchor_that_names_no_members_and_a_field_it_does_not_define() {
    // Given a detach anchored on a symbol, and one carrying `to`
    // When each plan line is read
    let a_symbol = the_refusal_of_reading(
        r#"{"kind":"symbol","file":"src/host.rs","path":"Host"}"#,
        ",\"name\":\"session\"",
    );
    let a_destination =
        the_refusal_of_reading(THE_ITEMS, ",\"name\":\"session\",\"to\":\"app::tick\"");

    // Then the anchor and the field are each refused by name
    assert!(
        a_symbol.contains("`detach_method` anchors on members of one inherent `impl`"),
        "{a_symbol}"
    );
    assert!(
        a_destination.contains("`to` is not a field of `detach_method`"),
        "{a_destination}"
    );
}

// ---------------------------------------------------------------------------------------------
// R6: the note
// ---------------------------------------------------------------------------------------------

#[test]
fn the_note_counts_calls_files_and_uses() {
    // Given two members called three times from two files, one of which needs a `use`
    let app = an_app_calling_both_members();
    let mut server = the_server_of_both_members();

    // When both are detached
    let resolution = detaching(&app, &mut server, &["Host::bump", "Host::total"]);

    // Then the note names the members and the parameter, counts the calls, files and `use`s, and
    // says the block was removed
    assert_eq!(
        resolution.notes,
        [
            "detach_method: Host::bump, Host::total became functions over `session`; 3 calls in 2 \
          files re-pointed (1 `use` added); the block was emptied and removed"
        ]
    );
}
