//! A cross-crate move rewrites grouped `use` lines instead of refusing them — `#reshape` 8/19.
//!
//! `#carve` 21 moved three clusters out of `tddy-session-lifecycle`, and every one was refused before
//! a server answered: a grouped `use` whose leaves land on different qualifiers, a path that reaches a
//! co-moving module through the origin's glob facade read as staying behind, and a
//! `pub(in crate::connection_service)` read as a path back into the origin. Each was hand-edited
//! before the engine ran. These tests decide what the move writes from a reference set the test
//! hands it, so they need no server.
//!
//! The group rule is the one `repoint_facade_imports` uses: Rule P replaces the prefix in place when
//! every leaf agrees on it; otherwise Rule S keeps the members whose path stays, first, and lifts
//! each re-pointed member into a `use` of its own.

use std::collections::BTreeMap;

use tddy_code_restructuring::crate_move::{
    self, DeclarationKind, ItemReferences, ModuleReferences, Reference,
};
use tddy_code_restructuring::registry::Workspace;
use tddy_code_restructuring::{
    Destination, FileEdit, ModuleHome, MovingCluster, Overlay, Position, Reexport,
    RestructureError, WorkspaceEdit,
};

const ORIGIN: &str = "crates/origin";
const ORIGIN_LIB: &str = "crates/origin/src/lib.rs";
const DESTINATION_MANIFEST: &str = "crates/destination/Cargo.toml";
const SPAWNER: &str = "crates/origin/src/spawner.rs";
const CALLER: &str = "crates/origin/src/caller.rs";
const CALLBACKS: &str = "crates/origin/src/connection_service/agent_host_callbacks.rs";
const PROMPT: &str = "crates/origin/src/connection_service/attached_initial_prompt.rs";
const SPLIT_START: &str = "crates/origin/src/connection_service/split_start.rs";

/// `origin`'s root for the flat fixtures: the modules the clusters take, modules that stay, and the
/// facades that forward to `shared` (`clock`, `timer`) and to both crates at once (`mix`).
const FLAT_ORIGIN_ROOT: &str = "//! The origin.\n\n\
     pub mod agent_roster;\npub mod caller;\npub mod host;\npub mod livekit_rooms_stream;\n\
     pub mod spawn_worker;\npub mod spawner;\n\n\
     pub use shared::clock;\npub use shared::clock as timer;\n\n\
     pub mod mix {\n    pub use destination::records::Id;\n    pub use shared::clock::Clock;\n}\n\n\
     pub struct OriginOwned;\n";

/// `connection_service`'s module file for the nested fixtures: the `#carve` 21 R6 shape.
const THE_SERVICE: &str = "//! The service.\n\n\
     pub mod agent_host_callbacks;\npub mod attached_initial_prompt;\npub mod roster;\n\
     pub mod seed_codebase;\npub mod seeded_clone_guard;\npub mod split_start;\n\n\
     pub use roster::*;\npub use seed_codebase::*;\npub(crate) use shared::progress::*;\n";

struct AWorkspace {
    root: tempfile::TempDir,
    overlay: Overlay,
}

/// `shared`, `origin` (over both other crates) and `destination`, with `origin/src/<path>` holding
/// each of `files` — the crate root included — beside the modules every fixture shares.
fn a_workspace_whose_origin_holds(files: &[(&str, &str)]) -> AWorkspace {
    let mut workspace = AWorkspace {
        root: tempfile::tempdir().expect("a temporary directory"),
        overlay: Overlay::default(),
    }
    .with(
        "Cargo.toml",
        "[workspace]\nmembers = [\n    \"crates/shared\",\n    \"crates/origin\",\n    \
         \"crates/destination\",\n]\n",
    )
    .with(
        "crates/shared/Cargo.toml",
        "[package]\nname = \"shared\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .with(
        "crates/shared/src/lib.rs",
        "pub mod clock {\n    pub struct Clock;\n    pub struct Tick;\n}\n\n\
         pub mod progress {\n    pub struct AttachmentProgressSink;\n}\n",
    )
    .with(
        "crates/origin/Cargo.toml",
        "[package]\nname = \"origin\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
         [dependencies]\nshared = { path = \"../shared\" }\n\
         destination = { path = \"../destination\" }\n",
    )
    .with(
        DESTINATION_MANIFEST,
        "[package]\nname = \"destination\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
         [dependencies]\nshared = { path = \"../shared\" }\n",
    )
    .with(
        "crates/destination/src/lib.rs",
        "//! The destination.\n\npub mod records {\n    pub struct Id;\n}\n",
    )
    .with("crates/origin/src/host.rs", "pub struct Host;\n")
    .with(
        "crates/origin/src/spawn_worker.rs",
        "pub struct Worker;\n\npub struct Pool;\n",
    )
    .with(
        "crates/origin/src/connection_service/roster.rs",
        "pub struct AgentRoster;\n",
    )
    .with(
        "crates/origin/src/connection_service/seed_codebase.rs",
        "pub trait SeededAgentClones {}\n\npub fn seed() -> u32 {\n    1\n}\n",
    )
    .with(
        "crates/origin/src/connection_service/seeded_clone_guard.rs",
        "pub struct Guard;\n",
    );
    for (path, text) in files {
        workspace = workspace.with(&format!("{ORIGIN}/src/{path}"), text);
    }
    workspace
}

/// The flat fixture: `origin`'s root is [`FLAT_ORIGIN_ROOT`], `spawner` holds `spawner` and
/// `caller` holds `caller`.
fn a_flat_origin_whose_spawner_holds(spawner: &str, caller: &str) -> AWorkspace {
    a_workspace_whose_origin_holds(&[
        ("lib.rs", FLAT_ORIGIN_ROOT),
        ("spawner.rs", spawner),
        ("caller.rs", caller),
        ("livekit_rooms_stream.rs", "pub struct RoomRoster;\n"),
        ("agent_roster.rs", "pub struct Roster;\n"),
    ])
}

/// The nested fixture: `origin` declares only `connection_service`, whose module file is
/// [`THE_SERVICE`]; every member it declares holds an unused function unless `members` gives
/// `connection_service/<file>` a text of its own.
fn a_service_whose_members_hold(members: &[(&str, &str)]) -> AWorkspace {
    let mut workspace = a_workspace_whose_origin_holds(&[
        ("lib.rs", "//! The origin.\n\npub mod connection_service;\n"),
        ("connection_service.rs", THE_SERVICE),
        (
            "connection_service/agent_host_callbacks.rs",
            "pub fn unused() {}\n",
        ),
        (
            "connection_service/attached_initial_prompt.rs",
            "pub fn unused() {}\n",
        ),
        ("connection_service/split_start.rs", "pub fn unused() {}\n"),
    ]);
    for (file, text) in members {
        workspace = workspace.with(&format!("{ORIGIN}/src/connection_service/{file}"), text);
    }
    workspace
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

/// A member of a cluster leaving `origin`, by its module path: `["spawner"]`, or
/// `["connection_service", "seed_codebase"]`.
fn a_member(path: &[&str]) -> ModuleHome {
    let declared_in = match path {
        [_] => ORIGIN_LIB.to_string(),
        [parents @ .., _] => format!("{ORIGIN}/src/{}.rs", parents.join("/")),
        [] => panic!("a member names at least one module"),
    };
    ModuleHome {
        crate_dir: ORIGIN.to_string(),
        declared_in,
        path: path.iter().map(|segment| (*segment).to_string()).collect(),
    }
}

/// The modules at `members` moving to `destination` as one cluster.
fn a_cluster_of(members: &[&[&str]], reexport: Reexport) -> MovingCluster {
    MovingCluster {
        members: members.iter().map(|path| a_member(path)).collect(),
        destination: Destination {
            dir: "crates/destination".to_string(),
            package: "destination".to_string(),
            extern_name: "destination".to_string(),
        },
        reexport,
        creates: None,
    }
}

/// A reference set standing in for the Rust backend: each declaration of a moving file with every
/// place outside that names it.
#[derive(Default)]
struct AKnownReferenceSet {
    by_file: BTreeMap<String, Vec<ItemReferences>>,
}

fn nothing_reaches_the_cluster() -> AKnownReferenceSet {
    AKnownReferenceSet::default()
}

impl AKnownReferenceSet {
    /// `item`, declared in `declared_in`, named at every whole-word occurrence in `from`.
    fn reaching(
        mut self,
        workspace: &AWorkspace,
        item: &str,
        declared_in: &str,
        from: &[&str],
    ) -> Self {
        let referenced_at = from
            .iter()
            .flat_map(|path| {
                every_place_named(workspace, path, item)
                    .into_iter()
                    .map(|at| Reference {
                        path: (*path).to_string(),
                        at,
                    })
            })
            .collect();
        let declared_at = every_place_named(workspace, declared_in, item)
            .into_iter()
            .next()
            .unwrap_or_else(|| panic!("`{item}` is not declared in {declared_in}"));
        self.by_file
            .entry(declared_in.to_string())
            .or_default()
            .push(ItemReferences {
                item: item.to_string(),
                referenced_at,
                declared_at,
                within: Vec::new(),
                kind: DeclarationKind::Item,
            });
        self
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

/// Every one-based position at which `name` is written as a whole word in `path`.
fn every_place_named(workspace: &AWorkspace, path: &str, name: &str) -> Vec<Position> {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    workspace
        .read(path)
        .split('\n')
        .enumerate()
        .flat_map(|(index, written)| {
            written
                .match_indices(name)
                .filter(|(at, _)| {
                    let before = written[..*at].chars().next_back();
                    let after = written[at + name.len()..].chars().next();
                    !before.is_some_and(is_word) && !after.is_some_and(is_word)
                })
                .map(|(at, _)| Position {
                    line: index as u32 + 1,
                    col: written[..at].chars().count() as u32 + 1,
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn moving(
    workspace: &AWorkspace,
    engine: &mut AKnownReferenceSet,
    cluster: &MovingCluster,
) -> Result<WorkspaceEdit, RestructureError> {
    crate_move::resolve_cluster(engine, &workspace.workspace(), cluster)
}

fn moved(
    workspace: &AWorkspace,
    engine: &mut AKnownReferenceSet,
    cluster: &MovingCluster,
) -> WorkspaceEdit {
    moving(workspace, engine, cluster).expect("the move resolves")
}

fn the_refusal_of(
    workspace: &AWorkspace,
    engine: &mut AKnownReferenceSet,
    cluster: &MovingCluster,
) -> String {
    moving(workspace, engine, cluster)
        .expect_err("the move is refused")
        .to_string()
}

/// The text edits the move makes to `path`.
fn the_edits_to(edit: &WorkspaceEdit, path: &str) -> Vec<tddy_code_restructuring::TextEdit> {
    edit.changes
        .iter()
        .filter_map(|change| match change {
            FileEdit::Change {
                path: changed,
                edits,
            } if changed == path => Some(edits.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

/// What `path` reads as once the move's text edits to it are applied — its own text when it makes
/// none.
fn after(workspace: &AWorkspace, edit: &WorkspaceEdit, path: &str) -> String {
    tddy_code_restructuring::apply::edited(workspace.read(path), &the_edits_to(edit, path))
        .expect("the edits apply")
}

/// The `use` lines at the top of `text`, up to its first blank line.
fn the_header_of(text: &str) -> String {
    text.split("\n\n").next().unwrap_or_default().to_string()
}

/// Test 1 — the leaves of one group land on different qualifiers: the member that keeps its path
/// stays in the group, the re-pointed one leaves it.
#[test]
fn a_moved_files_group_whose_leaves_need_different_qualifiers_is_split_kept_members_first() {
    // Given `spawner` naming its co-moving sibling and a `shared` item through the origin's facade
    // in one group
    let workspace = a_flat_origin_whose_spawner_holds(
        "use crate::{spawn_worker::Worker, clock::Clock};\n\n\
         pub fn pair() -> (Worker, Clock) {\n    (Worker, Clock)\n}\n",
        "pub fn call() {}\n",
    );

    // When `spawner` and `spawn_worker` move together
    let edit = moved(
        &workspace,
        &mut nothing_reaches_the_cluster(),
        &a_cluster_of(&[&["spawner"], &["spawn_worker"]], Reexport::Glob),
    );

    // Then the sibling stays in the group and the facade path is its own `use`
    assert_eq!(
        the_header_of(&after(&workspace, &edit, SPAWNER)),
        "use crate::{spawn_worker::Worker};\nuse shared::clock::Clock;"
    );
}

/// Test 2 — a group whose leaves agree keeps today's in-place re-prefix, byte for byte, and a plain
/// `use` whose last segment changes keeps the name it bound.
#[test]
fn a_moved_files_group_whose_leaves_agree_is_re_prefixed_in_place_as_before() {
    // Given a group of two items of one facade, and a plain `use` of a renaming facade
    let workspace = a_flat_origin_whose_spawner_holds(
        "use crate::clock::{Clock, Tick};\nuse crate::timer;\n\n\
         pub fn pair() -> (Clock, Tick, timer::Clock) {\n    (Clock, Tick, timer::Clock)\n}\n",
        "pub fn call() {}\n",
    );

    // When `spawner` moves with its sibling
    let edit = moved(
        &workspace,
        &mut nothing_reaches_the_cluster(),
        &a_cluster_of(&[&["spawner"], &["spawn_worker"]], Reexport::Glob),
    );

    // Then the prefix is replaced in place, and the renamed module keeps its name
    assert_eq!(
        the_header_of(&after(&workspace, &edit, SPAWNER)),
        "use shared::clock::{Clock, Tick};\nuse shared::clock as timer;"
    );
}

/// Test 3 — a lifted member goes on binding the name the body uses: the one it wrote, or its own
/// alias.
#[test]
fn a_lifted_member_keeps_the_name_it_bound_and_its_own_alias() {
    // Given a group holding a kept sibling, a renaming facade and an aliased leaf
    let workspace = a_flat_origin_whose_spawner_holds(
        "use crate::{spawn_worker::Worker, timer, clock::Tick as Beat};\n\n\
         pub fn three() -> (Worker, timer::Clock, Beat) {\n    (Worker, timer::Clock, Beat)\n}\n",
        "pub fn call() {}\n",
    );

    // When `spawner` moves with its sibling
    let edit = moved(
        &workspace,
        &mut nothing_reaches_the_cluster(),
        &a_cluster_of(&[&["spawner"], &["spawn_worker"]], Reexport::Glob),
    );

    // Then each lifted member binds what it bound before
    assert_eq!(
        the_header_of(&after(&workspace, &edit, SPAWNER)),
        "use crate::{spawn_worker::Worker};\nuse shared::clock as timer;\n\
         use shared::clock::Tick as Beat;"
    );
}

/// Test 4 — a split keeps the statement's visibility on every line it writes, and the indentation of
/// a `use` inside an inline module.
#[test]
fn a_split_keeps_the_visibility_and_the_indentation_of_the_statement_it_replaced() {
    // Given a `pub use` group at the top, and a group inside an inline module
    let workspace = a_flat_origin_whose_spawner_holds(
        "pub use crate::{spawn_worker::Worker, clock::Clock};\n\n\
         pub mod inner {\n    use crate::{spawn_worker::Pool, clock::Tick};\n\n    \
         pub fn pair() -> (Pool, Tick) {\n        (Pool, Tick)\n    }\n}\n",
        "pub fn call() {}\n",
    );

    // When `spawner` moves with its sibling
    let edit = moved(
        &workspace,
        &mut nothing_reaches_the_cluster(),
        &a_cluster_of(&[&["spawner"], &["spawn_worker"]], Reexport::Glob),
    );

    // Then both splits keep what the statement they replaced carried
    assert_eq!(
        after(&workspace, &edit, SPAWNER),
        "pub use crate::{spawn_worker::Worker};\npub use shared::clock::Clock;\n\n\
         pub mod inner {\n    use crate::{spawn_worker::Pool};\n    use shared::clock::Tick;\n\n    \
         pub fn pair() -> (Pool, Tick) {\n        (Pool, Tick)\n    }\n}\n"
    );
}

/// Test 5 — a nested member whose leaves land on two prefixes cannot be lifted whole, so it is
/// flattened into one `use` per leaf, in leaf order.
#[test]
fn a_nested_member_whose_leaves_need_two_prefixes_is_flattened_into_one_use_per_leaf() {
    // Given a nested member over `mix`, which forwards one item from `shared` and one from the
    // destination
    let workspace = a_flat_origin_whose_spawner_holds(
        "use crate::{spawn_worker::Worker, mix::{Clock, Id}};\n\n\
         pub fn three() -> (Worker, Clock, Id) {\n    (Worker, Clock, Id)\n}\n",
        "pub fn call() {}\n",
    );

    // When `spawner` moves with its sibling
    let edit = moved(
        &workspace,
        &mut nothing_reaches_the_cluster(),
        &a_cluster_of(&[&["spawner"], &["spawn_worker"]], Reexport::Glob),
    );

    // Then the nested member becomes one statement per leaf
    assert_eq!(
        the_header_of(&after(&workspace, &edit, SPAWNER)),
        "use crate::{spawn_worker::Worker};\nuse shared::clock::Clock;\nuse crate::records::Id;"
    );
}

/// Test 6 — splitting a gated or documented group would repeat its attribute on every line, so the
/// move refuses it, naming the file and the line, and writes nothing.
#[test]
fn a_group_that_must_split_under_an_attribute_or_doc_comment_is_refused_naming_the_file_and_line_and_nothing_is_written(
) {
    // Given one `spawner` whose splitting group is gated, and one whose group is documented
    let gated = a_flat_origin_whose_spawner_holds(
        "#[cfg(unix)]\nuse crate::{spawn_worker::Worker, clock::Clock};\n",
        "pub fn call() {}\n",
    );
    let documented = a_flat_origin_whose_spawner_holds(
        "/// What spawning needs.\nuse crate::{spawn_worker::Worker, clock::Clock};\n",
        "pub fn call() {}\n",
    );
    let cluster = a_cluster_of(&[&["spawner"], &["spawn_worker"]], Reexport::Glob);

    // When each moves
    let refusals = [
        the_refusal_of(&gated, &mut nothing_reaches_the_cluster(), &cluster),
        the_refusal_of(&documented, &mut nothing_reaches_the_cluster(), &cluster),
    ];

    // Then both are refused at the group's line, saying why
    assert_eq!(
        refusals
            .clone()
            .map(|refusal| refusal.contains("crates/origin/src/spawner.rs:2")
                && refusal.contains("attribute or doc comment")),
        [true, true],
        "the refusals do not name the line and the attribute: {refusals:?}"
    );
}

/// Test 7 — the `#carve` 21 R6 file: a path through `connection_service`'s glob facade reaches the
/// co-moving `seed_codebase`, so it lands there; the group splits into the three `use` lines the
/// hand edit wrote; and the destination gains no dependency on the crate it came from.
#[test]
fn a_path_through_an_in_crate_glob_facade_to_a_co_moving_member_lands_at_that_member_and_is_no_edge(
) {
    // Given `agent_host_callbacks` naming `SeededAgentClones` through `pub use seed_codebase::*;`
    let workspace = a_service_whose_members_hold(&[(
        "agent_host_callbacks.rs",
        "use crate::connection_service::{seed_codebase, seeded_clone_guard, SeededAgentClones};\n\n\
         pub fn callbacks(_clones: &dyn SeededAgentClones) -> (u32, seeded_clone_guard::Guard) {\n    \
         (seed_codebase::seed(), seeded_clone_guard::Guard)\n}\n",
    )]);

    // When the three move as one cluster, leaving a facade
    let edit = moved(
        &workspace,
        &mut nothing_reaches_the_cluster(),
        &a_cluster_of(
            &[
                &["connection_service", "agent_host_callbacks"],
                &["connection_service", "seed_codebase"],
                &["connection_service", "seeded_clone_guard"],
            ],
            Reexport::Glob,
        ),
    );

    // Then the group is the hand edit's three lines, and `origin` is no dependency of `destination`
    assert_eq!(
        (
            the_header_of(&after(&workspace, &edit, CALLBACKS)),
            after(&workspace, &edit, DESTINATION_MANIFEST).contains("origin"),
        ),
        (
            "use crate::seed_codebase;\nuse crate::seeded_clone_guard;\n\
             use crate::seed_codebase::SeededAgentClones;"
                .to_string(),
            false
        )
    );
}

/// Test 8 — following the facade only counts a path as travelling when it reaches a member: one
/// that reaches a module staying behind is still an edge back, named by where it is defined.
#[test]
fn a_path_through_an_in_crate_glob_facade_to_a_module_staying_behind_is_still_an_edge_back() {
    // Given `agent_host_callbacks` naming `AgentRoster` through `pub use roster::*;`, and `roster`
    // staying
    let workspace = a_service_whose_members_hold(&[(
        "agent_host_callbacks.rs",
        "use crate::connection_service::AgentRoster;\n\n\
         pub fn roster() -> AgentRoster {\n    AgentRoster\n}\n",
    )]);

    // When it moves alone, leaving a facade
    let refusal = the_refusal_of(
        &workspace,
        &mut nothing_reaches_the_cluster(),
        &a_cluster_of(
            &[&["connection_service", "agent_host_callbacks"]],
            Reexport::Glob,
        ),
    );

    // Then the cycle refusal names the defining path
    assert!(
        refusal.contains("still names `origin`")
            && refusal.contains("origin::connection_service::roster::AgentRoster"),
        "the refusal does not name the edge back: {refusal}"
    );
}

/// Test 9 — once the group can be split, a member of it that stays in the origin is refused for
/// what it is: an edge back, not a group shape.
#[test]
fn a_split_group_whose_member_stays_in_the_origin_is_refused_as_a_dependency_cycle_not_as_a_group()
{
    // Given a group naming a `shared` item through a facade and an item the origin defines
    let workspace = a_flat_origin_whose_spawner_holds(
        "use crate::{clock::Clock, OriginOwned};\n\n\
         pub fn pair() -> (Clock, OriginOwned) {\n    (Clock, OriginOwned)\n}\n",
        "pub fn call() {}\n",
    );

    // When `spawner` moves, leaving a facade
    let refusal = the_refusal_of(
        &workspace,
        &mut nothing_reaches_the_cluster(),
        &a_cluster_of(&[&["spawner"]], Reexport::Glob),
    );

    // Then the refusal is the cycle, naming the origin's item
    assert!(
        refusal.contains("still names `origin` (origin::OriginOwned)")
            && !refusal.contains("write one `use` per path"),
        "the refusal is not the cycle: {refusal}"
    );
}

/// Test 10 — `pub(in crate::connection_service)` is a visibility, not a path back into the origin:
/// when `connection_service` stays, the narrowest spelling valid in the destination is `pub(crate)`.
#[test]
fn a_pub_in_restriction_naming_a_module_that_stays_becomes_pub_crate_and_is_no_edge() {
    // Given a member restricted to the module that holds it
    let workspace = a_service_whose_members_hold(&[(
        "attached_initial_prompt.rs",
        "pub(in crate::connection_service) fn prompt() -> u32 {\n    1\n}\n",
    )]);

    // When it moves out of `connection_service`, leaving a facade
    let edit = moved(
        &workspace,
        &mut nothing_reaches_the_cluster(),
        &a_cluster_of(
            &[&["connection_service", "attached_initial_prompt"]],
            Reexport::Glob,
        ),
    );

    // Then the restriction is `pub(crate)`
    assert_eq!(
        after(&workspace, &edit, PROMPT),
        "pub(crate) fn prompt() -> u32 {\n    1\n}\n"
    );
}

/// Test 11 — a restriction naming a module that moves too is rewritten to where that module lands,
/// as a path to a co-moving member always was.
#[test]
fn a_pub_in_restriction_naming_a_co_moving_module_is_rewritten_to_where_it_lands() {
    // Given a member restricted to itself
    let workspace = a_service_whose_members_hold(&[(
        "attached_initial_prompt.rs",
        "pub(in crate::connection_service::attached_initial_prompt) fn prompt() -> u32 {\n    1\n}\n",
    )]);

    // When it moves, leaving a facade
    let edit = moved(
        &workspace,
        &mut nothing_reaches_the_cluster(),
        &a_cluster_of(
            &[&["connection_service", "attached_initial_prompt"]],
            Reexport::Glob,
        ),
    );

    // Then the restriction names the module where it landed
    assert_eq!(
        after(&workspace, &edit, PROMPT),
        "pub(in crate::attached_initial_prompt) fn prompt() -> u32 {\n    1\n}\n"
    );
}

/// Test 12 — with no facade and no caller nothing refuses the move, and a restriction must still
/// never be written with a crate's name nor add the origin to the destination's manifest.
#[test]
fn a_pub_in_restriction_adds_no_manifest_line_and_is_never_written_with_a_crate_name() {
    // Given a member restricted to the module that holds it, which nothing outside names
    let workspace = a_service_whose_members_hold(&[(
        "attached_initial_prompt.rs",
        "pub(in crate::connection_service) fn prompt() -> u32 {\n    1\n}\n",
    )]);

    // When it moves without a facade
    let edit = moved(
        &workspace,
        &mut nothing_reaches_the_cluster(),
        &a_cluster_of(
            &[&["connection_service", "attached_initial_prompt"]],
            Reexport::None,
        ),
    );

    // Then the restriction is `pub(crate)` and the destination does not depend on `origin`
    assert_eq!(
        (
            after(&workspace, &edit, PROMPT),
            after(&workspace, &edit, DESTINATION_MANIFEST).contains("origin"),
        ),
        (
            "pub(crate) fn prompt() -> u32 {\n    1\n}\n".to_string(),
            false
        )
    );
}

/// Test 13 — without a facade the move re-points its callers; one written inside someone else's
/// group leaves the group as a `use` of its own instead of being spliced into it.
#[test]
fn a_callers_re_point_inside_a_grouped_use_splits_the_group_instead_of_splicing_into_it() {
    // Given a caller importing `RoomRoster` in a group with two modules that stay
    let workspace = a_flat_origin_whose_spawner_holds(
        "pub fn spawn() {}\n",
        "use crate::{agent_roster::Roster, livekit_rooms_stream::RoomRoster, spawn_worker::Worker};\n\n\
         pub fn all() -> (Roster, RoomRoster, Worker) {\n    (Roster, RoomRoster, Worker)\n}\n",
    );
    let mut engine = nothing_reaches_the_cluster().reaching(
        &workspace,
        "RoomRoster",
        "crates/origin/src/livekit_rooms_stream.rs",
        &[CALLER],
    );

    // When `livekit_rooms_stream` moves without a facade
    let edit = moved(
        &workspace,
        &mut engine,
        &a_cluster_of(&[&["livekit_rooms_stream"]], Reexport::None),
    );

    // Then the group keeps what stays, and the re-pointed path follows on its own
    assert_eq!(
        the_header_of(&after(&workspace, &edit, CALLER)),
        "use crate::{agent_roster::Roster, spawn_worker::Worker};\n\
         use destination::livekit_rooms_stream::RoomRoster;"
    );
}

/// Test 14 — a caller group naming two members of one cluster is rewritten once, as a statement,
/// rather than once per member over spans of the same group.
#[test]
fn a_callers_group_naming_two_members_of_one_cluster_receives_one_statement_edit() {
    // Given a caller importing from two members and from a module that stays, in one group
    let workspace = a_flat_origin_whose_spawner_holds(
        "pub fn spawn() {}\n",
        "use crate::{agent_roster::Roster, livekit_rooms_stream::RoomRoster, spawn_worker::Worker};\n\n\
         pub fn all() -> (Roster, RoomRoster, Worker) {\n    (Roster, RoomRoster, Worker)\n}\n",
    );
    let mut engine = nothing_reaches_the_cluster()
        .reaching(
            &workspace,
            "Roster",
            "crates/origin/src/agent_roster.rs",
            &[CALLER],
        )
        .reaching(
            &workspace,
            "RoomRoster",
            "crates/origin/src/livekit_rooms_stream.rs",
            &[CALLER],
        );

    // When both members move without a facade
    let edit = moved(
        &workspace,
        &mut engine,
        &a_cluster_of(
            &[&["agent_roster"], &["livekit_rooms_stream"]],
            Reexport::None,
        ),
    );

    // Then the caller gets one edit, which keeps what stays and lifts both members
    assert_eq!(
        (
            the_edits_to(&edit, CALLER).len(),
            the_header_of(&after(&workspace, &edit, CALLER)),
        ),
        (
            1,
            "use crate::{spawn_worker::Worker};\nuse destination::agent_roster::Roster;\n\
             use destination::livekit_rooms_stream::RoomRoster;"
                .to_string()
        )
    );
}

/// Test 15 — a caller group every leaf of which moves agrees on its new prefix, so it keeps its
/// shape under it.
#[test]
fn a_callers_group_whose_every_leaf_moves_is_re_prefixed_in_place() {
    // Given a caller importing only from the two members
    let workspace = a_flat_origin_whose_spawner_holds(
        "pub fn spawn() {}\n",
        "use crate::{agent_roster::Roster, livekit_rooms_stream::RoomRoster};\n\n\
         pub fn both() -> (Roster, RoomRoster) {\n    (Roster, RoomRoster)\n}\n",
    );
    let mut engine = nothing_reaches_the_cluster()
        .reaching(
            &workspace,
            "Roster",
            "crates/origin/src/agent_roster.rs",
            &[CALLER],
        )
        .reaching(
            &workspace,
            "RoomRoster",
            "crates/origin/src/livekit_rooms_stream.rs",
            &[CALLER],
        );

    // When both move without a facade
    let edit = moved(
        &workspace,
        &mut engine,
        &a_cluster_of(
            &[&["agent_roster"], &["livekit_rooms_stream"]],
            Reexport::None,
        ),
    );

    // Then the group is re-prefixed in place
    assert_eq!(
        the_header_of(&after(&workspace, &edit, CALLER)),
        "use destination::{agent_roster::Roster, livekit_rooms_stream::RoomRoster};"
    );
}

/// Test 16 (probe) — the `#carve` 21 R8 `AttachmentProgressSink` shape: a name the origin's module
/// globs in from another crate (`pub(crate) use shared::progress::*;`) is defined in that crate, so
/// it is no edge back and the moved file names it there.
#[test]
fn a_path_through_a_glob_the_origin_takes_from_another_crate_is_no_edge() {
    // Given `split_start` naming `AttachmentProgressSink` through its parent's glob of `shared`
    let workspace = a_service_whose_members_hold(&[(
        "split_start.rs",
        "use super::AttachmentProgressSink;\n\n\
         pub fn sink() -> AttachmentProgressSink {\n    AttachmentProgressSink\n}\n",
    )]);

    // When it moves, leaving a facade
    let edit = moved(
        &workspace,
        &mut nothing_reaches_the_cluster(),
        &a_cluster_of(&[&["connection_service", "split_start"]], Reexport::Glob),
    );

    // Then it names the item where `shared` defines it
    assert_eq!(
        the_header_of(&after(&workspace, &edit, SPLIT_START)),
        "use shared::progress::AttachmentProgressSink;"
    );
}
