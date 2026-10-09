//! A cross-crate move widens what the origin still reaches — `#reshape` 7/19.
//!
//! Across a crate boundary the only visibility that compiles is `pub`, so the question a move
//! answers is **which** declarations of the files it carries to widen: what a file it does not
//! carry, outside the destination, still references; what the moved module's parent re-exports by
//! glob; and what the signature of either names. `#carve` 21 answered it by hand, 275 times, with a
//! `cargo check` loop that also widened same-named parameters and wrote `pub` on a trait method.
//!
//! These tests decide what the move writes from a reference set the test hands it, so they need no
//! server. Each declaration in the set carries the position its name is written at, which is the
//! only thing an edit is addressed by.

use std::collections::BTreeMap;

use tddy_code_restructuring::crate_move::{
    self, DeclarationKind, ItemReferences, ModuleReferences, Reference,
};
use tddy_code_restructuring::registry::Workspace;
use tddy_code_restructuring::{
    Destination, FileEdit, ModuleHome, MovingCluster, Overlay, Position, Reexport,
    RestructureError, VisibilityChange, WorkspaceEdit,
};

const ORIGIN: &str = "crates/origin";
const ORIGIN_LIB: &str = "crates/origin/src/lib.rs";
const ROSTER: &str = "crates/origin/src/roster.rs";
const LEDGER: &str = "crates/origin/src/ledger.rs";
const FACTORY: &str = "crates/origin/src/factory.rs";
const SPLIT: &str = "crates/origin/src/split.rs";
const RUNTIME: &str = "crates/origin/src/runtime.rs";
const CONNECTION_SERVICE: &str = "crates/origin/src/connection_service.rs";
const ATTACHMENT_PROGRESS: &str = "crates/origin/src/connection_service/attachment_progress.rs";
const SIBLING: &str = "crates/origin/src/connection_service/sibling.rs";
const DESTINATION_USER: &str = "crates/destination/src/user.rs";

/// The module most tests move. Line numbers are read off this list, one-based.
const THE_ROSTER: &[&str] = &[
    "//! The roster.",                                     // 1
    "",                                                    // 2
    "pub(crate) struct AgentRoster {",                     // 3
    "    pub(crate) rev: u64,",                            // 4
    "    secret: u64,",                                    // 5
    "}",                                                   // 6
    "",                                                    // 7
    "impl AgentRoster {",                                  // 8
    "    pub(crate) fn broadcast(&self) -> u64 {",         // 9
    "        self.rev + self.internal()",                  // 10
    "    }",                                               // 11
    "",                                                    // 12
    "    fn internal(&self) -> u64 {",                     // 13
    "        self.secret",                                 // 14
    "    }",                                               // 15
    "}",                                                   // 16
    "",                                                    // 17
    "impl Default for AgentRoster {",                      // 18
    "    fn default() -> Self {",                          // 19
    "        Self { rev: 0, secret: 0 }",                  // 20
    "    }",                                               // 21
    "}",                                                   // 22
    "",                                                    // 23
    "pub(crate) fn mint_first_admission_token() -> u64 {", // 24
    "    0",                                               // 25
    "}",                                                   // 26
    "",                                                    // 27
    "pub(crate) trait Rostered {",                         // 28
    "    fn rostered(&self) -> bool;",                     // 29
    "}",                                                   // 30
    "",                                                    // 31
    "pub(crate) enum Phase {",                             // 32
    "    Idle,",                                           // 33
    "}",                                                   // 34
    "",                                                    // 35
    "pub(crate) struct Tally {",                           // 36
    "    pub(crate) rev: u64,",                            // 37
    "}",                                                   // 38
    "",                                                    // 39
    "pub(crate) mod inner {",                              // 40
    "    pub(crate) struct Probe;",                        // 41
    "}",                                                   // 42
    "",                                                    // 43
    "pub(super) fn handed_up() -> u64 {",                  // 44
    "    1",                                               // 45
    "}",                                                   // 46
    "",                                                    // 47
    "pub(in crate::roster) fn kept_in() -> u64 {",         // 48
    "    2",                                               // 49
    "}",                                                   // 50
    "",                                                    // 51
    "pub fn already_public() -> u64 {",                    // 52
    "    3",                                               // 53
    "}",                                                   // 54
    "",                                                    // 55
    "mod pty_handle;",                                     // 56
];

/// `runtime` stays behind and names most of the roster. Its `rev` parameter and its own
/// `broadcast` share names with roster declarations, and must never be what gets rewritten.
const THE_RUNTIME: &[&str] = &[
    "use crate::roster::{already_public, handed_up, mint_first_admission_token, AgentRoster};", // 1
    "use crate::roster::{inner::Probe, pty_handle::PtyHandle, Phase, Rostered, Tally};",        // 2
    "use crate::factory::make;",                                                                // 3
    "use crate::ledger::ledger_rev;",                                                           // 4
    "use crate::split::split_declared;",                                                        // 5
    "",                                                                                         // 6
    "pub fn boot(rev: u64, roster: &AgentRoster) -> u64 {",                                     // 7
    "    roster.broadcast() + roster.rev + rev + make().size() + ledger_rev(roster)",           // 8
    "}",                                                                                        // 9
    "",                                                                              // 10
    "fn broadcast() -> u64 {",                                                       // 11
    "    let _ = (Probe, PtyHandle, Phase::Idle, Tally { rev: 0 });",                // 12
    "    AgentRoster::default().rostered() as u64 + handed_up() + already_public()", // 13
    "}",                                                                             // 14
];

/// `ledger` names the roster's `Tally`, and nothing else does; runtime calls `ledger_rev`.
const THE_LEDGER: &[&str] = &[
    "use crate::roster::Tally;",                        // 1
    "",                                                 // 2
    "pub(crate) fn ledger_rev(tally: &Tally) -> u64 {", // 3
    "    tally.rev",                                    // 4
    "}",                                                // 5
];

/// `factory`'s `make` returns a `Widget` no path outside names.
const THE_FACTORY: &[&str] = &[
    "pub(crate) struct Widget {",             // 1
    "    size: u64,",                         // 2
    "}",                                      // 3
    "",                                       // 4
    "impl Widget {",                          // 5
    "    pub(crate) fn size(&self) -> u64 {", // 6
    "        self.size",                      // 7
    "    }",                                  // 8
    "}",                                      // 9
    "",                                       // 10
    "pub(crate) fn make() -> Widget {",       // 11
    "    Widget { size: 3 }",                 // 12
    "}",                                      // 13
];

/// `attachment_progress`, which its parent re-exports with `pub(crate) use attachment_progress::*;`.
const THE_ATTACHMENT_PROGRESS: &[&str] = &[
    "pub(crate) struct AttachmentProgressSink {", // 1
    "    pub(crate) sent: u64,",                  // 2
    "}",                                          // 3
    "",                                           // 4
    "pub(crate) trait Progressing {",             // 5
    "    fn tick(&self) -> u64;",                 // 6
    "}",                                          // 7
    "",                                           // 8
    "fn private_helper() -> u64 {",               // 9
    "    0",                                      // 10
    "}",                                          // 11
];

/// Source text from its lines, one per entry.
fn source(lines: &[&str]) -> String {
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

struct AWorkspace {
    root: tempfile::TempDir,
    overlay: Overlay,
}

/// An `origin` crate holding every module above, and a `destination` holding one file that names
/// the roster from inside the crate the roster moves into.
fn a_workspace_whose_modules_the_origin_still_reaches() -> AWorkspace {
    AWorkspace {
        root: tempfile::tempdir().expect("a temporary directory"),
        overlay: Overlay::default(),
    }
    .with(
        "Cargo.toml",
        "[workspace]\nmembers = [\n    \"crates/origin\",\n    \"crates/destination\",\n]\n",
    )
    .with(
        "crates/origin/Cargo.toml",
        "[package]\nname = \"origin\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
         [dependencies]\ndestination = { path = \"../destination\" }\n",
    )
    .with(
        ORIGIN_LIB,
        "//! The origin.\n\npub mod connection_service;\npub mod factory;\npub mod ledger;\n\
         pub mod roster;\npub mod runtime;\npub mod split;\n",
    )
    .with(ROSTER, &source(THE_ROSTER))
    .with("crates/origin/src/roster/pty_handle.rs", "pub struct PtyHandle;\n")
    .with(RUNTIME, &source(THE_RUNTIME))
    .with(LEDGER, &source(THE_LEDGER))
    .with(FACTORY, &source(THE_FACTORY))
    .with(
        SPLIT,
        "pub(crate) fn\nsplit_declared() -> u64 {\n    0\n}\n",
    )
    .with(
        CONNECTION_SERVICE,
        "//! The service.\n\nmod attachment_progress;\npub(crate) use attachment_progress::*;\n\n\
         pub mod sibling;\n",
    )
    .with(ATTACHMENT_PROGRESS, &source(THE_ATTACHMENT_PROGRESS))
    .with(
        SIBLING,
        "use super::*;\n\npub fn progress() -> u64 {\n    let sink = AttachmentProgressSink { sent: 1 };\n    \
         sink.tick() + sink.sent\n}\n",
    )
    .with(
        "crates/destination/Cargo.toml",
        "[package]\nname = \"destination\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .with("crates/destination/src/lib.rs", "//! The destination.\n\npub mod user;\n")
    .with(
        DESTINATION_USER,
        "pub fn mint() -> u64 {\n    crate::roster::mint_first_admission_token()\n}\n",
    )
}

impl AWorkspace {
    fn with(self, path: &str, text: &str) -> Self {
        let absolute = self.root.path().join(path);
        std::fs::create_dir_all(absolute.parent().expect("a parent")).expect("directories");
        std::fs::write(absolute, text).expect("the file is written");
        self
    }

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

/// The one-based character position of the first whole-word `name` on `line` of `path`.
fn where_named(workspace: &AWorkspace, path: &str, line: u32, name: &str) -> Position {
    let text = workspace.read(path);
    let written = text
        .split('\n')
        .nth(line as usize - 1)
        .unwrap_or_else(|| panic!("{path} has no line {line}"));
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let col = written
        .match_indices(name)
        .find(|(at, _)| {
            let before = written[..*at].chars().next_back();
            let after = written[at + name.len()..].chars().next();
            !before.is_some_and(is_word) && !after.is_some_and(is_word)
        })
        .map(|(at, _)| written[..at].chars().count() as u32 + 1)
        .unwrap_or_else(|| panic!("`{name}` is not on line {line} of {path}"));
    Position { line, col }
}

/// A reference set standing in for the Rust backend's `documentSymbol` + `textDocument/references`:
/// every declaration of each moving file, at its position, with the places outside that name it.
#[derive(Default)]
struct AKnownReferenceSet {
    by_file: BTreeMap<String, Vec<ItemReferences>>,
}

/// One declaration the reference set reports, before anything reaches it.
struct Declaring<'a> {
    file: &'a str,
    line: u32,
    name: &'a str,
    within: &'a [&'a str],
    kind: DeclarationKind,
}

fn an_item<'a>(file: &'a str, line: u32, name: &'a str) -> Declaring<'a> {
    Declaring {
        file,
        line,
        name,
        within: &[],
        kind: DeclarationKind::Item,
    }
}

fn a_member<'a>(
    file: &'a str,
    line: u32,
    name: &'a str,
    within: &'a [&'a str],
    kind: DeclarationKind,
) -> Declaring<'a> {
    Declaring {
        file,
        line,
        name,
        within,
        kind,
    }
}

impl AKnownReferenceSet {
    /// `declaring` is declared and named from each `(path, line)` in `from`.
    fn reaching(
        mut self,
        workspace: &AWorkspace,
        declaring: Declaring<'_>,
        from: &[(&str, u32)],
    ) -> Self {
        let referenced_at = from
            .iter()
            .map(|(path, line)| Reference {
                path: (*path).to_string(),
                at: where_named(workspace, path, *line, declaring.name),
            })
            .collect();
        self.by_file
            .entry(declaring.file.to_string())
            .or_default()
            .push(ItemReferences {
                item: declaring.name.to_string(),
                referenced_at,
                declared_at: where_named(workspace, declaring.file, declaring.line, declaring.name),
                within: declaring.within.iter().map(|c| (*c).to_string()).collect(),
                kind: declaring.kind,
            });
        self
    }

    /// `declaring` is declared and nothing outside names it.
    fn declaring(self, workspace: &AWorkspace, declaring: Declaring<'_>) -> Self {
        self.reaching(workspace, declaring, &[])
    }
}

impl ModuleReferences for AKnownReferenceSet {
    fn outside_references(
        &mut self,
        _workspace: &Workspace<'_>,
        file: &str,
    ) -> Result<Vec<ItemReferences>, RestructureError> {
        Ok(self.by_file.get(file).cloned().unwrap_or_default())
    }
}

/// Every declaration of the roster, reached the way `runtime` (and `destination::user`, and
/// `ledger`) name them.
fn the_roster_as_the_origin_reaches_it(workspace: &AWorkspace) -> AKnownReferenceSet {
    AKnownReferenceSet::default()
        .reaching(
            workspace,
            an_item(ROSTER, 3, "AgentRoster"),
            &[(RUNTIME, 1), (RUNTIME, 7)],
        )
        .reaching(
            workspace,
            a_member(ROSTER, 4, "rev", &["AgentRoster"], DeclarationKind::Field),
            &[(RUNTIME, 8)],
        )
        .declaring(
            workspace,
            a_member(
                ROSTER,
                5,
                "secret",
                &["AgentRoster"],
                DeclarationKind::Field,
            ),
        )
        .reaching(
            workspace,
            a_member(
                ROSTER,
                9,
                "broadcast",
                &["impl AgentRoster"],
                DeclarationKind::InherentMember,
            ),
            &[(RUNTIME, 8)],
        )
        .declaring(
            workspace,
            a_member(
                ROSTER,
                13,
                "internal",
                &["impl AgentRoster"],
                DeclarationKind::InherentMember,
            ),
        )
        .reaching(
            workspace,
            a_member(
                ROSTER,
                19,
                "default",
                &["impl Default for AgentRoster"],
                DeclarationKind::NoVisibility,
            ),
            &[(RUNTIME, 13)],
        )
        .reaching(
            workspace,
            an_item(ROSTER, 24, "mint_first_admission_token"),
            &[(RUNTIME, 1), (DESTINATION_USER, 2)],
        )
        .reaching(workspace, an_item(ROSTER, 28, "Rostered"), &[(RUNTIME, 2)])
        .reaching(
            workspace,
            a_member(
                ROSTER,
                29,
                "rostered",
                &["Rostered"],
                DeclarationKind::NoVisibility,
            ),
            &[(RUNTIME, 13)],
        )
        .reaching(workspace, an_item(ROSTER, 32, "Phase"), &[(RUNTIME, 2)])
        .reaching(
            workspace,
            a_member(
                ROSTER,
                33,
                "Idle",
                &["Phase"],
                DeclarationKind::NoVisibility,
            ),
            &[(RUNTIME, 12)],
        )
        .reaching(
            workspace,
            an_item(ROSTER, 36, "Tally"),
            &[(RUNTIME, 2), (LEDGER, 1)],
        )
        .reaching(
            workspace,
            a_member(ROSTER, 37, "rev", &["Tally"], DeclarationKind::Field),
            &[(LEDGER, 4)],
        )
        .declaring(workspace, an_item(ROSTER, 40, "inner"))
        .reaching(
            workspace,
            Declaring {
                within: &["inner"],
                ..an_item(ROSTER, 41, "Probe")
            },
            &[(RUNTIME, 2)],
        )
        .reaching(workspace, an_item(ROSTER, 44, "handed_up"), &[(RUNTIME, 1)])
        .declaring(workspace, an_item(ROSTER, 48, "kept_in"))
        .reaching(
            workspace,
            an_item(ROSTER, 52, "already_public"),
            &[(RUNTIME, 1)],
        )
        .reaching(
            workspace,
            an_item(ROSTER, 56, "pty_handle"),
            &[(RUNTIME, 2)],
        )
}

fn the_destination() -> Destination {
    Destination {
        dir: "crates/destination".to_string(),
        package: "destination".to_string(),
        extern_name: "destination".to_string(),
    }
}

fn a_module(path: &[&str], declared_in: &str) -> ModuleHome {
    ModuleHome {
        crate_dir: ORIGIN.to_string(),
        declared_in: declared_in.to_string(),
        path: path.iter().map(|segment| (*segment).to_string()).collect(),
    }
}

fn a_cluster_of(members: Vec<ModuleHome>, reexport: Reexport) -> MovingCluster {
    MovingCluster {
        members,
        destination: the_destination(),
        reexport,
        creates: None,
    }
}

fn the_roster_moving_alone() -> MovingCluster {
    a_cluster_of(vec![a_module(&["roster"], ORIGIN_LIB)], Reexport::Glob)
}

fn resolving(
    workspace: &AWorkspace,
    engine: &mut AKnownReferenceSet,
    cluster: &MovingCluster,
) -> WorkspaceEdit {
    crate_move::resolve_cluster(engine, &workspace.workspace(), cluster).expect("the move resolves")
}

/// What `path` reads as once the edit's text changes for it are applied.
fn after(workspace: &AWorkspace, edit: &WorkspaceEdit, path: &str) -> String {
    let edits: Vec<_> = edit
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
    tddy_code_restructuring::apply::edited(workspace.read(path), &edits).expect("the edits apply")
}

/// Line `n` (one-based) of `text`.
fn line(text: &str, n: usize) -> &str {
    text.split('\n').nth(n - 1).expect("the line exists")
}

/// The roster's text after `edit`, as the move lands it.
fn the_moved_roster(workspace: &AWorkspace, edit: &WorkspaceEdit) -> String {
    after(workspace, edit, ROSTER)
}

/// Test 1 — the `#carve` 21 R2 shape: a `pub(crate)` function the origin still calls lands `pub`.
#[test]
fn a_pub_crate_fn_the_origin_still_calls_lands_pub_and_is_reported() {
    // Given a roster whose `mint_first_admission_token` runtime calls
    let workspace = a_workspace_whose_modules_the_origin_still_reaches();
    let mut engine = the_roster_as_the_origin_reaches_it(&workspace);

    // When the roster moves alone
    let edit = resolving(&workspace, &mut engine, &the_roster_moving_alone());

    // Then the function is `pub` where it lands
    assert_eq!(
        line(&the_moved_roster(&workspace, &edit), 24),
        "pub fn mint_first_admission_token() -> u64 {"
    );
}

/// Test 2 — the fields and inherent methods the origin names are widened with their type; the
/// field and the method it does not name keep the visibility they were written with.
#[test]
fn a_struct_its_fields_and_inherent_methods_the_origin_names_land_pub_and_the_rest_keep_their_visibility(
) {
    // Given runtime naming `AgentRoster`, its `rev` and its `broadcast`
    let workspace = a_workspace_whose_modules_the_origin_still_reaches();
    let mut engine = the_roster_as_the_origin_reaches_it(&workspace);

    // When the roster moves alone
    let moved = the_moved_roster(
        &workspace,
        &resolving(&workspace, &mut engine, &the_roster_moving_alone()),
    );

    // Then lines 3–16 read with exactly those three widened
    assert_eq!(
        moved.split('\n').skip(2).take(14).collect::<Vec<_>>(),
        [
            "pub struct AgentRoster {",
            "    pub rev: u64,",
            "    secret: u64,",
            "}",
            "",
            "impl AgentRoster {",
            "    pub fn broadcast(&self) -> u64 {",
            "        self.rev + self.internal()",
            "    }",
            "",
            "    fn internal(&self) -> u64 {",
            "        self.secret",
            "    }",
            "}",
        ]
    );
}

/// Test 3 — a declaration only a co-moving member names needs nothing: both land in one crate.
#[test]
fn a_declaration_only_a_co_moving_member_reaches_keeps_its_visibility() {
    // Given `Tally`'s field, named by `ledger` alone, with `ledger` moving beside the roster
    let workspace = a_workspace_whose_modules_the_origin_still_reaches();
    let mut engine = the_roster_as_the_origin_reaches_it(&workspace).reaching(
        &workspace,
        an_item(LEDGER, 3, "ledger_rev"),
        &[(RUNTIME, 4)],
    );
    let cluster = a_cluster_of(
        vec![
            a_module(&["roster"], ORIGIN_LIB),
            a_module(&["ledger"], ORIGIN_LIB),
        ],
        Reexport::Glob,
    );

    // When the two move together
    let moved = the_moved_roster(&workspace, &resolving(&workspace, &mut engine, &cluster));

    // Then the field keeps `pub(crate)`, while the type runtime names beside it is widened
    assert_eq!(
        [line(&moved, 36), line(&moved, 37)],
        ["pub struct Tally {", "    pub(crate) rev: u64,"]
    );
}

/// Test 4 — a reference from inside the destination crate was already crossing a crate boundary,
/// so it asks for nothing more.
#[test]
fn a_reference_from_inside_the_destination_crate_widens_nothing() {
    // Given `mint_first_admission_token` named only by `destination::user`, and `AgentRoster`
    // named by runtime
    let workspace = a_workspace_whose_modules_the_origin_still_reaches();
    let mut engine = AKnownReferenceSet::default()
        .reaching(
            &workspace,
            an_item(ROSTER, 3, "AgentRoster"),
            &[(RUNTIME, 7)],
        )
        .reaching(
            &workspace,
            an_item(ROSTER, 24, "mint_first_admission_token"),
            &[(DESTINATION_USER, 2)],
        );

    // When the roster moves alone
    let moved = the_moved_roster(
        &workspace,
        &resolving(&workspace, &mut engine, &the_roster_moving_alone()),
    );

    // Then the function the destination names reads as written; the type runtime names does not
    assert_eq!(
        [line(&moved, 3), line(&moved, 24)],
        [
            "pub struct AgentRoster {",
            "pub(crate) fn mint_first_admission_token() -> u64 {"
        ]
    );
}

/// Test 5 — a trait-impl member, a trait item and an enum variant take no visibility (`E0449`),
/// however much the origin uses them.
#[test]
fn a_member_of_a_trait_impl_a_trait_item_and_an_enum_variant_are_never_given_pub() {
    // Given runtime calling `default`, `rostered` and naming `Phase::Idle`
    let workspace = a_workspace_whose_modules_the_origin_still_reaches();
    let mut engine = the_roster_as_the_origin_reaches_it(&workspace);

    // When the roster moves alone
    let moved = the_moved_roster(
        &workspace,
        &resolving(&workspace, &mut engine, &the_roster_moving_alone()),
    );

    // Then those three lines read as written, while their trait and enum are widened
    assert_eq!(
        [
            line(&moved, 19),
            line(&moved, 28),
            line(&moved, 29),
            line(&moved, 32),
            line(&moved, 33)
        ],
        [
            "    fn default() -> Self {",
            "pub trait Rostered {",
            "    fn rostered(&self) -> bool;",
            "pub enum Phase {",
            "    Idle,"
        ]
    );
}

/// Test 6 — the edit is addressed by the declaration's position, so a same-named field of another
/// struct, a parameter and a function elsewhere are never the ones rewritten.
#[test]
fn a_same_named_parameter_field_and_function_elsewhere_are_byte_identical() {
    // Given `AgentRoster::rev` reached, beside an unreached `Tally::rev`, runtime's `rev` parameter
    // and runtime's own `broadcast`
    let workspace = a_workspace_whose_modules_the_origin_still_reaches();
    let mut engine = AKnownReferenceSet::default()
        .reaching(
            &workspace,
            a_member(ROSTER, 4, "rev", &["AgentRoster"], DeclarationKind::Field),
            &[(RUNTIME, 8)],
        )
        .declaring(
            &workspace,
            a_member(ROSTER, 37, "rev", &["Tally"], DeclarationKind::Field),
        )
        .reaching(
            &workspace,
            a_member(
                ROSTER,
                9,
                "broadcast",
                &["impl AgentRoster"],
                DeclarationKind::InherentMember,
            ),
            &[(RUNTIME, 8)],
        );

    // When the roster moves alone
    let edit = resolving(&workspace, &mut engine, &the_roster_moving_alone());

    // Then only the reached `rev` changed, and runtime is untouched
    let moved = the_moved_roster(&workspace, &edit);
    assert_eq!(
        [line(&moved, 4), line(&moved, 37)],
        ["    pub rev: u64,", "    pub(crate) rev: u64,"]
    );
    assert_eq!(after(&workspace, &edit, RUNTIME), source(THE_RUNTIME));
}

/// Test 7 — an item of an inline module is reached through the module, so both are widened.
#[test]
fn an_item_of_an_inline_module_and_the_mod_declarations_on_its_path_land_pub() {
    // Given runtime naming `roster::inner::Probe`
    let workspace = a_workspace_whose_modules_the_origin_still_reaches();
    let mut engine = the_roster_as_the_origin_reaches_it(&workspace);

    // When the roster moves alone
    let moved = the_moved_roster(
        &workspace,
        &resolving(&workspace, &mut engine, &the_roster_moving_alone()),
    );

    // Then the inline module and its struct are both `pub`
    assert_eq!(
        [line(&moved, 40), line(&moved, 41)],
        ["pub mod inner {", "    pub struct Probe;"]
    );
}

/// Test 8 — the `#carve` 21 R8 shape: a child module's declaration the origin names through.
#[test]
fn a_child_mod_declaration_the_origin_names_lands_pub_mod() {
    // Given runtime naming `roster::pty_handle::PtyHandle`
    let workspace = a_workspace_whose_modules_the_origin_still_reaches();
    let mut engine = the_roster_as_the_origin_reaches_it(&workspace);

    // When the roster moves alone
    let moved = the_moved_roster(
        &workspace,
        &resolving(&workspace, &mut engine, &the_roster_moving_alone()),
    );

    // Then its declaration is `pub mod`
    assert_eq!(line(&moved, 56), "pub mod pty_handle;");
}

/// The `attachment_progress` declarations, none of them reached by a path the reference set lists:
/// the parent's glob is the only thing that makes them visible.
fn attachment_progress_reached_by_its_parents_glob_alone(
    workspace: &AWorkspace,
) -> AKnownReferenceSet {
    AKnownReferenceSet::default()
        .declaring(
            workspace,
            an_item(ATTACHMENT_PROGRESS, 1, "AttachmentProgressSink"),
        )
        .reaching(
            workspace,
            a_member(
                ATTACHMENT_PROGRESS,
                2,
                "sent",
                &["AttachmentProgressSink"],
                DeclarationKind::Field,
            ),
            &[(SIBLING, 5)],
        )
        .declaring(workspace, an_item(ATTACHMENT_PROGRESS, 5, "Progressing"))
        .declaring(
            workspace,
            a_member(
                ATTACHMENT_PROGRESS,
                6,
                "tick",
                &["Progressing"],
                DeclarationKind::NoVisibility,
            ),
        )
        .declaring(workspace, an_item(ATTACHMENT_PROGRESS, 9, "private_helper"))
}

fn attachment_progress_moving(reexport: Reexport) -> MovingCluster {
    a_cluster_of(
        vec![a_module(
            &["connection_service", "attachment_progress"],
            CONNECTION_SERVICE,
        )],
        reexport,
    )
}

/// Test 9 — the 09-25 shape: every non-private item a parent's glob re-exports is reached, and the
/// report says the glob is why.
#[test]
fn every_non_private_item_of_a_module_its_parent_re_exports_by_glob_lands_pub_with_the_glob_as_its_reason(
) {
    // Given `attachment_progress`, re-exported by `pub(crate) use attachment_progress::*;`
    let workspace = a_workspace_whose_modules_the_origin_still_reaches();
    let mut engine = attachment_progress_reached_by_its_parents_glob_alone(&workspace);

    // When it moves with no facade
    let resolution = crate_move::cluster_resolution(
        &mut engine,
        &workspace.workspace(),
        &attachment_progress_moving(Reexport::None),
    )
    .expect("the move resolves");

    // Then the struct and the trait are `pub` through the glob, and the field by its reference
    let moved = after(&workspace, &resolution.edit, ATTACHMENT_PROGRESS);
    assert_eq!(
        [line(&moved, 1), line(&moved, 2), line(&moved, 5)],
        [
            "pub struct AttachmentProgressSink {",
            "    pub sent: u64,",
            "pub trait Progressing {"
        ]
    );
    let through_the_glob =
        Some("through `crates/origin/src/connection_service.rs`'s glob".to_string());
    assert_eq!(
        resolution.report,
        vec![
            VisibilityChange {
                item: "AttachmentProgressSink".to_string(),
                from: "pub(crate)".to_string(),
                to: "pub".to_string(),
                reason: through_the_glob.clone(),
            },
            VisibilityChange {
                item: "AttachmentProgressSink::sent".to_string(),
                from: "pub(crate)".to_string(),
                to: "pub".to_string(),
                reason: None,
            },
            VisibilityChange {
                item: "Progressing".to_string(),
                from: "pub(crate)".to_string(),
                to: "pub".to_string(),
                reason: through_the_glob,
            },
        ]
    );
}

/// Test 10 — behind a facade the old glob still resolves, through the module, and still imports
/// only what is `pub`: the same items are widened.
#[test]
fn a_glob_re_exported_module_moved_behind_a_facade_widens_the_same_items() {
    // Given the same child, moving behind a facade
    let workspace = a_workspace_whose_modules_the_origin_still_reaches();
    let mut engine = attachment_progress_reached_by_its_parents_glob_alone(&workspace);

    // When it moves
    let edit = resolving(
        &workspace,
        &mut engine,
        &attachment_progress_moving(Reexport::Glob),
    );

    // Then the struct and the trait are `pub`
    let moved = after(&workspace, &edit, ATTACHMENT_PROGRESS);
    assert_eq!(
        [line(&moved, 1), line(&moved, 5)],
        [
            "pub struct AttachmentProgressSink {",
            "pub trait Progressing {"
        ]
    );
}

/// Test 11 — a private item was never visible to the parent's glob, so it stays private.
#[test]
fn a_private_item_the_parent_glob_cannot_see_keeps_its_visibility() {
    // Given the child's private helper
    let workspace = a_workspace_whose_modules_the_origin_still_reaches();
    let mut engine = attachment_progress_reached_by_its_parents_glob_alone(&workspace);

    // When it moves with no facade
    let edit = resolving(
        &workspace,
        &mut engine,
        &attachment_progress_moving(Reexport::None),
    );

    // Then the helper reads as written, while the struct the glob made visible is widened
    let moved = after(&workspace, &edit, ATTACHMENT_PROGRESS);
    assert_eq!(
        [line(&moved, 1), line(&moved, 9)],
        [
            "pub struct AttachmentProgressSink {",
            "fn private_helper() -> u64 {"
        ]
    );
}

/// Test 12 — a type no path names but a widened signature returns is widened too
/// (`private_interfaces`), and the report names the signature.
#[test]
fn a_type_a_widened_signature_names_lands_pub_with_the_signature_as_its_reason() {
    // Given runtime calling `make().size()`, which never names `Widget`
    let workspace = a_workspace_whose_modules_the_origin_still_reaches();
    let mut engine = AKnownReferenceSet::default()
        .declaring(&workspace, an_item(FACTORY, 1, "Widget"))
        .declaring(
            &workspace,
            a_member(FACTORY, 2, "size", &["Widget"], DeclarationKind::Field),
        )
        .reaching(
            &workspace,
            a_member(
                FACTORY,
                6,
                "size",
                &["impl Widget"],
                DeclarationKind::InherentMember,
            ),
            &[(RUNTIME, 8)],
        )
        .reaching(
            &workspace,
            an_item(FACTORY, 11, "make"),
            &[(RUNTIME, 3), (RUNTIME, 8)],
        );

    // When the factory moves
    let resolution = crate_move::cluster_resolution(
        &mut engine,
        &workspace.workspace(),
        &a_cluster_of(vec![a_module(&["factory"], ORIGIN_LIB)], Reexport::Glob),
    )
    .expect("the move resolves");

    // Then `Widget` is `pub` beside `make` and `size`, its field is not, and the report says why
    let moved = after(&workspace, &resolution.edit, FACTORY);
    assert_eq!(
        [
            line(&moved, 1),
            line(&moved, 2),
            line(&moved, 6),
            line(&moved, 11)
        ],
        [
            "pub struct Widget {",
            "    size: u64,",
            "    pub fn size(&self) -> u64 {",
            "pub fn make() -> Widget {"
        ]
    );
    assert!(
        resolution.report.contains(&VisibilityChange {
            item: "Widget".to_string(),
            from: "pub(crate)".to_string(),
            to: "pub".to_string(),
            reason: Some("named by the signature of `make`".to_string()),
        }),
        "the report does not name the signature: {:?}",
        resolution.report
    );
}

/// Test 13 — a reached restricted visibility becomes `pub` like any other; an unreached one is the
/// crate move's own `pub(in …)` rewrite's to make (`#reshape` 8/19), and this pass leaves it.
#[test]
fn a_reached_declaration_written_pub_super_or_pub_in_lands_pub_and_an_unreached_one_is_byte_identical(
) {
    // Given runtime calling `handed_up` (`pub(super)`), and nothing reaching `kept_in` (`pub(in …)`)
    let workspace = a_workspace_whose_modules_the_origin_still_reaches();
    let mut engine = the_roster_as_the_origin_reaches_it(&workspace);

    // When the roster moves alone
    let moved = the_moved_roster(
        &workspace,
        &resolving(&workspace, &mut engine, &the_roster_moving_alone()),
    );

    // Then
    assert_eq!(
        [line(&moved, 44), line(&moved, 48)],
        [
            "pub fn handed_up() -> u64 {",
            "pub(in crate::roster) fn kept_in() -> u64 {"
        ]
    );
}

/// Test 14 — what is already `pub` has nothing to answer for.
#[test]
fn a_declaration_already_pub_is_neither_edited_nor_reported() {
    // Given runtime calling `already_public`, and nothing else reached
    let workspace = a_workspace_whose_modules_the_origin_still_reaches();
    let mut engine = AKnownReferenceSet::default().reaching(
        &workspace,
        an_item(ROSTER, 52, "already_public"),
        &[(RUNTIME, 1)],
    );

    // When the roster moves alone
    let moved = the_moved_roster(
        &workspace,
        &resolving(&workspace, &mut engine, &the_roster_moving_alone()),
    );

    // Then the roster lands as written — and, with something else reached, it does change
    assert_eq!(moved, source(THE_ROSTER));
    let mut reaching_more = the_roster_as_the_origin_reaches_it(&workspace);
    assert_ne!(
        the_moved_roster(
            &workspace,
            &resolving(&workspace, &mut reaching_more, &the_roster_moving_alone()),
        ),
        source(THE_ROSTER),
        "the control: a roster with reached `pub(crate)` declarations must change"
    );
}

/// Test 15 — a declaration whose keyword sits on the line above its name cannot be addressed by
/// the name's position, so the move refuses rather than guess.
#[test]
fn a_reached_declaration_whose_keyword_is_on_the_line_above_its_name_is_refused_naming_it_and_nothing_is_written(
) {
    // Given `split_declared`, written over two lines, and called by runtime
    let workspace = a_workspace_whose_modules_the_origin_still_reaches();
    let mut engine = AKnownReferenceSet::default().reaching(
        &workspace,
        an_item(SPLIT, 2, "split_declared"),
        &[(RUNTIME, 5)],
    );

    // When its module moves
    let refusal = crate_move::resolve_cluster(
        &mut engine,
        &workspace.workspace(),
        &a_cluster_of(vec![a_module(&["split"], ORIGIN_LIB)], Reexport::Glob),
    );

    // Then the move is refused, naming the declaration and the file
    assert_eq!(
        refusal
            .expect_err("the keyword is not on the name's line")
            .to_string(),
        RestructureError::SeamRefused(
            "the declaration of `split_declared` in `crates/origin/src/split.rs` has its keyword \
             on a line above its name, so the move cannot widen it: write the keyword and the \
             name on one line"
                .to_string()
        )
        .to_string()
    );
}

/// Test 16 — a cluster's widenings join each member's existing change: one change per file, in the
/// one coordinate space the header edits use.
#[test]
fn the_widenings_of_a_cluster_merge_into_each_members_header_change_with_one_change_per_file() {
    // Given the roster and the ledger, both with declarations runtime reaches
    let workspace = a_workspace_whose_modules_the_origin_still_reaches();
    let mut engine = the_roster_as_the_origin_reaches_it(&workspace).reaching(
        &workspace,
        an_item(LEDGER, 3, "ledger_rev"),
        &[(RUNTIME, 4)],
    );
    let cluster = a_cluster_of(
        vec![
            a_module(&["roster"], ORIGIN_LIB),
            a_module(&["ledger"], ORIGIN_LIB),
        ],
        Reexport::Glob,
    );

    // When they move together
    let edit = resolving(&workspace, &mut engine, &cluster);

    // Then each moving file has exactly one change, which carries its widening
    let changes_to = |path: &str| {
        edit.changes
            .iter()
            .filter(|change| matches!(change, FileEdit::Change { path: changed, .. } if changed == path))
            .count()
    };
    assert_eq!([changes_to(ROSTER), changes_to(LEDGER)], [1, 1]);
    assert_eq!(
        line(&after(&workspace, &edit, LEDGER), 3),
        "pub fn ledger_rev(tally: &Tally) -> u64 {"
    );
}

/// Test 17 — a widening with a reason is stated with it, so every front end that states widenings
/// through `console::widening` says why without an edit of its own.
#[test]
fn a_widening_with_a_reason_is_stated_with_it_and_one_without_is_stated_as_before() {
    // Given one widening through a glob, and one reached by a path
    let through_the_glob = VisibilityChange {
        item: "Progressing".to_string(),
        from: "pub(crate)".to_string(),
        to: "pub".to_string(),
        reason: Some("through `connection_service.rs`'s glob".to_string()),
    };
    let by_a_path = VisibilityChange {
        reason: None,
        ..through_the_glob.clone()
    };

    // When each is stated
    let stated = [
        tddy_code_restructuring::console::widening(&through_the_glob),
        tddy_code_restructuring::console::widening(&by_a_path),
    ];

    // Then
    assert_eq!(
        stated,
        [
            "`Progressing` pub(crate) -> pub (through `connection_service.rs`'s glob)".to_string(),
            "`Progressing` pub(crate) -> pub".to_string(),
        ]
    );
}

/// Test 18 (green pin) — a journal written before widenings had reasons still reads, and a widening
/// without one is written the way it always was.
#[test]
fn a_journal_record_written_before_reasons_existed_still_reads() {
    // Given a widening as a journal from before the field recorded it
    let recorded = r#"{"item":"strip_resize","from":"private","to":"pub(crate)"}"#;

    // When it is read back, and written again
    let read: VisibilityChange = serde_json::from_str(recorded).expect("the record reads");
    let written = serde_json::to_string(&read).expect("the record writes");

    // Then it has no reason, and says nothing about one
    assert_eq!(read.reason, None);
    assert_eq!(written, recorded);
}
