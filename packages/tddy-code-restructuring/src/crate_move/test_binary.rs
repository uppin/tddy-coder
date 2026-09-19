//! Moving a test binary — `<crate>/tests/<name>.rs` — to the crate whose code it exercises.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::crate_move::destination::Destination;
use crate::crate_move::{header, manifest_edits, module_home};
use crate::edit::{FileEdit, TextEdit, WorkspaceEdit};
use crate::plan::RefactorOp;
use crate::registry::Workspace;

use super::malformed;
use super::Result;

/// Where a test binary sits, and where it is going.
///
/// Deliberately not [`Move`]: that struct carries `module`, `origin` and `reexport`, and a test
/// binary has no module name to declare, no `mod` line in any origin to remove, and no facade it
/// could ever leave behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestBinaryMove {
    /// The test file, relative to the repository root — `<crate>/tests/<name>.rs`.
    pub source: String,
    /// The binary's name, which is its file stem. Cargo derives it the same way.
    pub name: String,
    /// The crate the test is leaving. Needed only to resolve the paths it writes.
    pub origin: Destination,
    /// The crate whose code it actually exercises.
    pub destination: Destination,
}

impl TestBinaryMove {
    /// Where the file lands.
    #[must_use]
    pub fn moved_to(&self) -> String {
        format!("{}/tests/{}.rs", self.destination.dir, self.name)
    }
}

/// Read a test-binary move from its operation.
///
/// # Errors
///
/// Refuses an anchor that is not `<crate>/tests/<name>.rs` — a module move and a test-binary move
/// are different operations precisely because the path shapes differ, so admitting the wrong one
/// here would produce an edit neither operation's rules cover.
pub fn read_test_binary_move(workspace: &Workspace<'_>, op: &RefactorOp) -> Result<TestBinaryMove> {
    let source = op.anchor.file().to_string();
    let (crate_dir, name) = test_binary_in_a_crate(&source)?;
    let to = op.to.as_deref().ok_or_else(|| {
        malformed(
            "`move_test_binary_to_crate` needs `to`: the destination crate's directory, relative \
             to the repository root",
        )
    })?;

    Ok(TestBinaryMove {
        origin: Destination::read(workspace.root, &crate_dir)?,
        destination: Destination::read(workspace.root, to)?,
        source,
        name,
    })
}

/// The crate a test binary belongs to and the name cargo gives it, read from the path alone.
///
/// The shape *is* the operation's subject: cargo builds `<crate>/tests/<name>.rs` as a crate root
/// of its own, and nothing else under `tests/` is one — `tests/common/mod.rs` is a module of
/// whichever binaries declare it, and moving one on its own would take a module out of every
/// binary that names it.
///
/// Read from the string rather than from disk so the refusal can name the shape it wanted before
/// any manifest is opened: an anchor this far from a test binary will not have a crate to read
/// either, and "there is no `Cargo.toml` above it" is the wrong thing to say about it.
fn test_binary_in_a_crate(source: &str) -> Result<(String, String)> {
    let path = Path::new(source);
    let name = path
        .extension()
        .filter(|extension| *extension == "rs")
        .and_then(|_| path.file_stem())
        .and_then(|stem| stem.to_str());
    let crate_dir = path
        .parent()
        .filter(|tests| {
            tests
                .file_name()
                .is_some_and(|directory| directory == "tests")
        })
        .and_then(Path::parent)
        .map(|directory| directory.display().to_string())
        .filter(|directory| !directory.is_empty());

    match (crate_dir, name) {
        (Some(crate_dir), Some(name)) => Ok((crate_dir, name.to_string())),
        _ => Err(malformed(format!(
            "`{source}` is not a test binary: `move_test_binary_to_crate` moves \
             `<crate>/tests/<name>.rs`, the shape cargo builds as a crate root of its own. A file \
             under `src/` is a module and `move_module_to_crate` moves it; a file deeper than \
             `tests/` is a module of a binary rather than a binary"
        ))),
    }
}

/// Resolve the move of a test binary to the crate whose code it exercises.
///
/// Three edits, and no more: the rename, the moved file's own text, and the destination's
/// `[dev-dependencies]`. There is no origin edit at all — cargo auto-discovers `tests/*.rs`, so the
/// crate the test left never named it and has nothing to stop naming.
///
/// The re-pointing pass is where this differs from a module move in substance rather than shape. A
/// test reaches its subject through whatever path compiled at the time it was written, which in
/// this workspace may be **two** re-export facades deep: `tddy_daemon::host_registry` is
/// `tddy-session-lifecycle`'s re-export of `tddy-host-service`'s module. Re-pointing it one hop
/// short produces a test that compiles and still names the wrong crate, so every path is resolved
/// with [`defining_crate`](module_home::defining_crate).
///
/// # Errors
///
/// Refuses when the anchor is not a test binary, when the destination is not a crate, or when a
/// path the moved test names cannot be resolved to a defining crate.
pub fn resolve_test_binary_move(
    workspace: &Workspace<'_>,
    moving: &TestBinaryMove,
) -> Result<WorkspaceEdit> {
    let text = workspace.read(&moving.source)?;
    let repointed = repointed_references(workspace, &text, &moving.origin)?;

    Ok(WorkspaceEdit {
        changes: vec![
            FileEdit::Rename {
                from: moving.source.clone(),
                to: moving.moved_to(),
            },
            // Addressed at the path the file is moving *from*: [`crate::apply`] applies changes
            // before renames, so the file is still where the plan found it when its own text is
            // rewritten.
            FileEdit::Change {
                path: moving.source.clone(),
                edits: repointed.edits,
            },
            destination_dev_dependencies(workspace, moving, &repointed.named)?,
        ],
    })
}

/// What the moved test says about the crates it will name in its new home.
struct References {
    /// The edits that re-point it, empty when every path it writes means the same thing in the
    /// destination as it did in the crate it left.
    edits: Vec<TextEdit>,
    /// Every crate the moved test's code names once re-pointed, by extern name, carrying that
    /// crate's own directory when the walk to it stayed inside this workspace.
    named: BTreeMap<String, Option<Destination>>,
}

/// The moved test's references, re-pointed at the crates that actually define what they reach.
///
/// Only a path naming the origin **by its extern name** changes, because that is the only kind
/// whose meaning the move alters. A test binary already is its own crate: `crate::` and `super::`
/// name the binary — the `mod common;` it declares, not the library beside it — and mean exactly
/// the same thing in the destination's `tests/`. That is the opposite of a module move, where
/// `crate::` is the whole of the header pass.
///
/// A path whose first segment is a module the file itself declares is left alone for the same
/// reason. `use common::PTY_STUB_OUTPUT;` under a `mod common;` reaches the binary's *own* module,
/// backed by `tests/common/mod.rs`, and in Rust 2018 an unqualified first segment resolves to a
/// crate-root item before it resolves to an extern crate. Reading it as one would send the walk
/// looking for a crate nobody declares and then refuse a plan that is correct.
///
/// **Every** occurrence of the origin's extern name is re-pointed, not just the ones in the leading
/// `use` header — and this is the second place a test binary parts company with a module move. A
/// moved module keeps its own `crate::`, so a path in a function body means the same thing
/// afterwards and is deliberately left alone. A test binary takes the whole file out, and
/// `tddy_daemon::project_storage::add_project(…)` in a body is an extern-crate path naming a crate
/// the destination need not depend on at all: 110 such paths, and one `use` indented inside a
/// `mod tests { … }`, survived a pass that read the header alone.
///
/// The crates named — which is what the destination's `[dev-dependencies]` are built from — are
/// collected from code only. See [`readable_spans`] for what that means and why prose is re-pointed
/// but never counted.
fn repointed_references(
    workspace: &Workspace<'_>,
    text: &str,
    origin: &Destination,
) -> Result<References> {
    let mut references = References {
        edits: Vec::new(),
        named: BTreeMap::new(),
    };

    record_crates_declared_in_the_header(text, origin, &mut references.named);

    for occurrence in origin_named_paths(text, &origin.extern_name) {
        let (qualifier, rest) = match occurrence.path.split_once("::") {
            Some(split) => split,
            None => (occurrence.path, ""),
        };
        // A group or a glob in prose is not a declaration to split: there is no `use` to write one
        // path per, and no single crate name that could stand for every member of the group.
        if occurrence.prose == Prose::Comment && (rest.starts_with('{') || rest.starts_with('*')) {
            continue;
        }

        let defining = defining_home(workspace, origin, occurrence.path, rest)?;
        if defining.extern_name != origin.extern_name {
            references.edits.push(manifest_edits::replacement(
                text,
                occurrence.at..occurrence.at + qualifier.len(),
                &defining.extern_name,
            ));
        }
        if occurrence.prose == Prose::Code {
            record(&mut references.named, defining.extern_name, defining.home);
        }
    }

    Ok(references)
}

/// The crates the moved test's `use` header names that are somebody else's to resolve.
///
/// `tokio`, `serde_json`, and the origin itself where a bare `use tddy_daemon;` names no module to
/// resolve: each has to travel to the destination's `[dev-dependencies]`, and none of them is
/// re-pointed. Every path that *does* name the origin and a module inside it is left to the
/// whole-file pass, which reaches this same declaration and would otherwise edit it twice.
///
/// A declaration is the only place another crate can be *recognised*. The origin's extern name is
/// known, so `tddy_daemon::…` is a crate path wherever it is written; nothing else is —
/// `PermissionMode::Plan` and `tokio::time::sleep` are the same shape, and only a type checker can
/// say which of them names a crate. So a crate reached from a body with no declaration of its own
/// is beyond a text pass either way, and this is where the crates to depend on are read from.
fn record_crates_declared_in_the_header(
    text: &str,
    origin: &Destination,
    named: &mut BTreeMap<String, Option<Destination>>,
) {
    let own_modules = modules_declared_in(text);

    for (_, path) in header::use_declarations(text) {
        let (qualifier, rest) = match path.split_once("::") {
            Some(split) => split,
            None => (path, ""),
        };

        if matches!(
            qualifier,
            "crate" | "super" | "self" | "std" | "core" | "alloc"
        ) || own_modules.contains(qualifier)
        {
            continue;
        }
        if qualifier != origin.extern_name {
            record(named, qualifier.to_string(), None);
        } else if rest.is_empty() {
            record(named, origin.extern_name.clone(), Some(origin.clone()));
        }
    }
}

/// Whether a stretch of a file is code the compiler resolves, or prose written beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Prose {
    Code,
    Comment,
}

/// One path in the moved test that opens with the origin's extern name.
struct Occurrence<'a> {
    /// Where the extern name starts, as a byte offset into the whole file.
    at: usize,
    /// The path as written, from the extern name to the end of its last segment.
    path: &'a str,
    /// Whether the compiler reads it, which is what says whether it is a dependency as well as a
    /// rewrite.
    prose: Prose,
}

/// Every path in the moved test that opens with the origin's extern name, in the order written.
///
/// A path is only one if the extern name is a **whole** first segment. `tddy_daemon_kernel::config`
/// opens with a longer identifier and is another crate's path entirely; `crate::tddy_daemon` names
/// something else again. Both are excluded by reading what sits either side of the match — and
/// `tddy-daemon-kernel` is a real crate here, named by 8 of the 95 suites in the first plan.
///
/// Only a name followed by `::` is reported. A bare `tddy_daemon` names no module, so there is
/// nothing to resolve and nothing to re-point — the one place it matters, `use tddy_daemon;`, is a
/// declaration and is read as one by [`record_crates_declared_in_the_header`].
fn origin_named_paths<'a>(text: &'a str, extern_name: &str) -> Vec<Occurrence<'a>> {
    let mut found = Vec::new();

    for (span, prose) in readable_spans(text) {
        let mut searched = span.start;
        while let Some(offset) = text[searched..span.end].find(extern_name) {
            let at = searched + offset;
            searched = at + extern_name.len();

            if !is_a_whole_first_segment(text, at, extern_name, span.end) {
                continue;
            }
            let path = written_path_from(text, at, span.end);
            if !path.contains("::") {
                continue;
            }
            found.push(Occurrence { at, path, prose });
        }
    }
    found
}

/// Whether the match at `at` is the whole of the first segment of the path it sits in.
fn is_a_whole_first_segment(text: &str, at: usize, extern_name: &str, limit: usize) -> bool {
    let before = &text[..at];

    !before.ends_with(header::is_path_character)
        && !before.ends_with("::")
        && segment_length(&text[at..limit]) == extern_name.len()
}

/// The whole `a::b::C` written from `at`, stopping at `limit`.
///
/// A group or a glob is reported as the one character that says which — `tddy_daemon::{` — because
/// that is all its reader has to tell apart, and reading to the closing brace would mean matching
/// nesting in prose that need not have any.
fn written_path_from(text: &str, at: usize, limit: usize) -> &str {
    let mut end = at + segment_length(&text[at..limit]);

    while let Some(behind) = text[end..limit].strip_prefix("::") {
        let length = segment_length(behind);
        if length > 0 {
            end += "::".len() + length;
            continue;
        }
        if behind.starts_with('{') || behind.starts_with('*') {
            end += "::".len() + 1;
        }
        break;
    }
    &text[at..end]
}

/// How much of `text` the path segment at its start occupies.
fn segment_length(text: &str) -> usize {
    text.find(|character: char| !header::is_path_character(character))
        .unwrap_or(text.len())
}

/// The file split into the stretches a path may be read out of, each labelled with what it is.
///
/// Three kinds of text name crates and only two of them are this operation's to read:
///
/// - **Code** is re-pointed and counted. A path there is resolved by the compiler, so leaving it
///   naming the origin is a file that does not build in its new home.
/// - **Comments** are re-pointed and not counted. A sentence describing what a suite exercises is
///   wrong the moment the suite exercises it from somewhere else, and a comment naming a crate the
///   file no longer uses is exactly the debt this operation exists to pay off. It is prose, not a
///   declaration, so it is no reason for the destination to gain a dependency.
/// - **String literals are left alone entirely.** What is written in one is data the suite asserts
///   on — an error message, a fixture, a path — produced by whatever emits it rather than resolved
///   from this file's imports. Rewriting it would change what the test asserts.
fn readable_spans(text: &str) -> Vec<(std::ops::Range<usize>, Prose)> {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut code_from = 0usize;
    let mut at = 0usize;

    fn code_before(spans: &mut Vec<(std::ops::Range<usize>, Prose)>, from: usize, at: usize) {
        if from < at {
            spans.push((from..at, Prose::Code));
        }
    }

    while at < bytes.len() {
        if bytes[at..].starts_with(b"//") {
            code_before(&mut spans, code_from, at);
            let end = text[at..]
                .find('\n')
                .map_or(text.len(), |newline| at + newline);
            spans.push((at..end, Prose::Comment));
            at = end;
        } else if bytes[at..].starts_with(b"/*") {
            code_before(&mut spans, code_from, at);
            let end = block_comment_end(text, at);
            spans.push((at..end, Prose::Comment));
            at = end;
        } else if let Some(end) = literal_end(text, at) {
            code_before(&mut spans, code_from, at);
            at = end;
        } else {
            at += 1;
            continue;
        }
        code_from = at;
    }

    code_before(&mut spans, code_from, bytes.len());
    spans
}

/// Where the block comment opening at `at` closes, counting the nesting Rust allows in one.
fn block_comment_end(text: &str, at: usize) -> usize {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut cursor = at;

    while cursor < bytes.len() {
        if bytes[cursor..].starts_with(b"/*") {
            depth += 1;
            cursor += 2;
        } else if bytes[cursor..].starts_with(b"*/") {
            depth -= 1;
            cursor += 2;
            if depth == 0 {
                return cursor;
            }
        } else {
            cursor += 1;
        }
    }
    bytes.len()
}

/// Where the literal starting at `at` ends, if one starts there at all.
///
/// Every shape a Rust literal takes, because each of them can hold the two characters that would
/// otherwise be read as code: an ordinary or byte string, a raw string of any hash depth, and a
/// character literal. The last is the delicate one — `'` opens a lifetime far more often than it
/// opens a literal, so it counts only when a closing quote follows one character or one escape.
fn literal_end(text: &str, at: usize) -> Option<usize> {
    let rest = &text[at..];

    if let Some(hashes) = raw_string_hashes(rest) {
        let terminator = format!("\"{}", "#".repeat(hashes));
        let opened = at + rest.find('"')? + 1;
        return Some(match text[opened..].find(&terminator) {
            Some(closed) => opened + closed + terminator.len(),
            None => text.len(),
        });
    }
    if rest.starts_with('"') || rest.starts_with("b\"") {
        return Some(string_end(text, at + rest.find('"')? + 1));
    }
    if rest.starts_with('\'') || rest.starts_with("b'") {
        return character_literal_end(text, at + rest.find('\'')?);
    }
    None
}

/// How many hashes the raw string starting here opens with, if it is one.
fn raw_string_hashes(rest: &str) -> Option<usize> {
    let after_prefix = rest.strip_prefix("br").or_else(|| rest.strip_prefix('r'))?;
    let hashes = after_prefix.len() - after_prefix.trim_start_matches('#').len();

    after_prefix[hashes..].starts_with('"').then_some(hashes)
}

/// Where the ordinary string opened just before `from` closes, honouring backslash escapes.
fn string_end(text: &str, from: usize) -> usize {
    let bytes = text.as_bytes();
    let mut cursor = from;

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\\' => cursor += 2,
            b'"' => return cursor + 1,
            _ => cursor += 1,
        }
    }
    bytes.len()
}

/// Where the character literal at `at` closes, or `None` when the quote opens a lifetime.
fn character_literal_end(text: &str, at: usize) -> Option<usize> {
    let rest = &text[at + 1..];
    let mut characters = rest.chars();
    let first = characters.next()?;

    let body = if first == '\\' {
        match characters.next()? {
            'u' => rest.find('}')? + 1,
            escaped => '\\'.len_utf8() + escaped.len_utf8(),
        }
    } else {
        first.len_utf8()
    };

    rest[body..]
        .starts_with('\'')
        .then_some(at + 1 + body + '\''.len_utf8())
}

/// The modules the moved file declares itself, which its own paths can name without a crate.
///
/// Cargo compiles every `tests/<name>.rs` as a crate root of its own, so a `mod` written there is a
/// crate-root item of that binary — `mod common;` backed by `tests/common/mod.rs`, or an inline
/// `mod fakes { … }`, both equally. Neither travels through any manifest, so neither is a crate the
/// destination could gain a dependency on.
///
/// Only an unindented declaration counts: one inside a function body or a nested module is not a
/// crate-root item, and a path in the file's header cannot name it — the same line the header
/// scanner draws, drawn once more here so the two agree about what "top level" means.
fn modules_declared_in(text: &str) -> BTreeSet<String> {
    text.lines().filter_map(module_declared_by).collect()
}

/// The module a top-level `mod` line declares, whether it is a file module or an inline one.
fn module_declared_by(line: &str) -> Option<String> {
    let trimmed = line.trim_end();
    if trimmed.starts_with(char::is_whitespace) {
        return None;
    }

    let declaration = match trimmed.strip_prefix("pub") {
        // `pub`, `pub(crate)`, `pub(super)` — the visibility says nothing about whether the module
        // is this binary's own, so whatever follows the parentheses is what has to be read.
        Some(visibility) => visibility.trim_start_matches(|c| c != ' ').trim_start(),
        None => trimmed,
    };
    let name = declaration
        .strip_prefix("mod ")?
        .trim_start()
        .split(|c: char| c == ';' || c == '{' || c.is_whitespace())
        .next()?;

    (!name.is_empty() && name.chars().all(header::is_path_character)).then(|| name.to_string())
}

/// Note a crate the moved test names, keeping the directory if either sighting of it found one.
fn record(
    named: &mut BTreeMap<String, Option<Destination>>,
    extern_name: String,
    home: Option<Destination>,
) {
    let known = named.entry(extern_name).or_insert(None);
    if known.is_none() {
        *known = home;
    }
}

/// The crate that defines what an origin-named path reaches, and its directory when it is one of
/// this workspace's own.
struct Defining {
    extern_name: String,
    home: Option<Destination>,
}

/// Walk the facades an origin-named path is reached through, to the crate that defines it.
///
/// [`module_home::defining_crate`] resolves **one** re-export, which is all a module move needs:
/// the crate that hop names is one the origin's own manifest declares, so the dependency line can
/// be carried across verbatim. A test binary was written against whatever path compiled at the
/// time, and in this workspace that is up to two facades deep — `tddy_daemon::host_registry` is
/// `tddy-session-lifecycle`'s re-export of `tddy-host-service`'s module. Stopping at the first hop
/// re-points the test at a crate that merely passes the module on, which compiles and is still the
/// wrong crate; so the same resolution is applied again from each crate it lands in. That is why
/// this walks the parent operation's answer rather than restating it.
///
/// The walk stops at a crate that defines the module itself, at one this workspace does not reach
/// by a path dependency — a published crate's re-export is not ours to follow — and at one it has
/// already visited, which a pair of crates re-exporting each other would otherwise loop through.
///
/// # Errors
///
/// Refuses a path whose first segment is the origin and whose second is a group or a glob. Which
/// crate defines `{a, b}` has as many answers as the group has members, and they need not agree;
/// splitting the declaration is the plan author's call, not this operation's.
fn defining_home(
    workspace: &Workspace<'_>,
    origin: &Destination,
    path: &str,
    rest: &str,
) -> Result<Defining> {
    // A bare `use <crate>;` names no module to resolve, so the crate the test wrote is the one it
    // means — there is no facade in a path with nothing behind the crate name.
    if rest.is_empty() {
        return Ok(Defining {
            extern_name: origin.extern_name.clone(),
            home: Some(origin.clone()),
        });
    }
    if rest.starts_with('{') || rest.starts_with('*') {
        return Err(malformed(format!(
            "`{path}` reaches several modules of `{}` at once, and they need not be defined by the \
             same crate — write one `use` per path so each can be re-pointed at the crate that \
             defines it",
            origin.package
        )));
    }

    let mut visited = BTreeSet::from([origin.extern_name.clone()]);
    let mut current = origin.clone();

    loop {
        let reached = format!("{}::{rest}", current.extern_name);
        let Some(next) = module_home::defining_crate(workspace, &current, &reached)? else {
            break;
        };
        if next == current.extern_name || !visited.insert(next.clone()) {
            break;
        }
        let Some(further) = current.path_dependency(workspace.root, &next)? else {
            return Ok(Defining {
                extern_name: next,
                home: None,
            });
        };
        current = further;
    }

    Ok(Defining {
        extern_name: current.extern_name.clone(),
        home: Some(current),
    })
}

/// The destination's `[dev-dependencies]`, gaining every crate the moved test names.
///
/// `[dev-dependencies]`, not `[dependencies]`: the file lands in the destination's `tests/`, and a
/// crate only a test needs is not one the library needs. A crate the destination already declares
/// as an ordinary dependency is left alone, because cargo compiles a test binary against both
/// tables — declaring it twice would be a second version to keep in step.
///
/// A crate the walk resolved to a directory has its line **authored**, from the destination back
/// to it. That is the one fact this operation may write itself, for the same reason a module move
/// writes the path back to the crate it left: it is a fact about this repository's own layout. A
/// crate the walk could not place is copied from the manifest that already declares it, because a
/// version invented here would be a fact about the world this operation has no way to know.
///
/// There is deliberately no dependency-cycle refusal, the one a module move makes. Cargo permits a
/// dev-dependency cycle precisely because it does not enter the library's own build, so a test
/// that goes on naming the crate it left is a normal outcome here rather than a defect.
fn destination_dev_dependencies(
    workspace: &Workspace<'_>,
    moving: &TestBinaryMove,
    named: &BTreeMap<String, Option<Destination>>,
) -> Result<FileEdit> {
    let path = format!("{}/Cargo.toml", moving.destination.dir);
    let text = workspace.read(&path)?;
    let origin = workspace.read(&format!("{}/Cargo.toml", moving.origin.dir))?;

    let mut lines = Vec::new();
    for (extern_name, home) in named {
        // The destination is what the test exercises, and a crate cannot depend on itself.
        if *extern_name == moving.destination.extern_name {
            continue;
        }
        if manifest_edits::declares_dependency(
            &text,
            manifest_edits::Table::Dependencies,
            extern_name,
        ) || manifest_edits::declares_dependency(
            &text,
            manifest_edits::Table::DevDependencies,
            extern_name,
        ) {
            continue;
        }

        if let Some(home) = home {
            lines.push(format!(
                "{} = {{ path = \"{}\" }}",
                home.package,
                manifest_edits::relative_from(&moving.destination.dir, &home.dir)
            ));
            continue;
        }

        let declared = manifest_edits::dependency_line(
            &origin,
            manifest_edits::Table::Dependencies,
            extern_name,
        )
        .or_else(|| {
            manifest_edits::dependency_line(
                &origin,
                manifest_edits::Table::DevDependencies,
                extern_name,
            )
        })
        .ok_or_else(|| {
            malformed(format!(
                "the moved test names `{extern_name}`, which {}/Cargo.toml declares in neither \
                 `[dependencies]` nor `[dev-dependencies]` — there is nothing to carry across",
                moving.origin.dir
            ))
        })?;
        lines.push(manifest_edits::re_anchored(
            &declared,
            &moving.origin.dir,
            &moving.destination.dir,
        ));
    }

    Ok(FileEdit::Change {
        path,
        edits: manifest_edits::with_dependencies(
            &text,
            manifest_edits::Table::DevDependencies,
            &lines,
        ),
    })
}
