//! Item anchors: from a crate-rooted item path to the exact coordinates the ledger translates.
//!
//! A plan's `item` and `items` anchors name *what* they act on, not *where* it sat when the plan was
//! written. They are resolved once, at run open, against the tree the run starts on, and lowered
//! into the `range` and `symbol` anchors every operation already understands. From there the
//! [`crate::PositionLedger`] carries them through the run exactly as it carries a hand-written range.
//!
//! The language-specific half — reading an outline, walking it by segment — lives behind
//! [`ItemResolver`]. What is here is what every language shares: which module a file is, how a
//! relative range becomes an absolute one, and when a resolved item is refused.

use std::path::{Path, PathBuf};

use crate::edit::{Position, Range};
use crate::plan::{split_path, Anchor, Fingerprint, ItemPath, Plan};
use crate::{RestructureError, Result};

/// An item the language server located.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedItem {
    /// The item's full extent, attributes and doc comments included, in one-based coordinates.
    pub range: Range,
    /// Where the item's name is — the position symbol operations act on.
    pub name: Position,
    /// The fingerprint of the item's text as the tree holds it now.
    pub fingerprint: Fingerprint,
}

/// A backend that can find an item by its path in one file.
pub trait ItemResolver {
    /// Locate `item` in `file` (relative to the workspace root).
    ///
    /// Refuses — never guesses — when the file's module path does not match the item's prefix, when
    /// a segment is absent, or when a segment matches more than one outline node. Nothing searches a
    /// file other than `file`.
    fn resolve_item(&mut self, file: &str, item: &ItemPath) -> Result<ResolvedItem>;
}

/// The module path of a source file inside its package, as `[crate, module, …]`.
///
/// The crate is the package's `name` with `-` read as `_`; `src/lib.rs` and `src/main.rs` are the
/// crate root, and `a/mod.rs` and `a.rs` are both module `a`. A file outside `src/` belongs to no
/// module path this can name and is refused.
pub fn module_path_of(root: &Path, file: &str) -> Result<Vec<String>> {
    let (package_dir, package) = owning_package(root, file)?;
    let source_dir = package_dir.join("src");
    let within_src = Path::new(file).strip_prefix(&source_dir).map_err(|_| {
        malformed(format!(
            "{file} is not under {}, so it is no module of the package `{package}`",
            source_dir.display()
        ))
    })?;

    let mut modules: Vec<String> = within_src
        .parent()
        .into_iter()
        .flat_map(Path::components)
        .map(|part| part.as_os_str().to_string_lossy().to_string())
        .collect();
    let stem = within_src
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_default();
    // `lib.rs` and `main.rs` are the crate root only at the top of `src/`; `mod.rs` is the module
    // its directory names, wherever it sits.
    let is_root_file = modules.is_empty() && matches!(stem.as_str(), "lib" | "main");
    if !is_root_file && stem != "mod" {
        modules.push(stem);
    }

    let mut path = vec![package.replace('-', "_")];
    path.extend(modules);
    Ok(path)
}

/// The directory (relative to `root`) and `[package] name` of the nearest package that holds `file`.
///
/// A workspace manifest declares no package, so it is walked past rather than mistaken for one.
pub(crate) fn owning_package(root: &Path, file: &str) -> Result<(PathBuf, String)> {
    let mut directory = Path::new(file).parent();
    while let Some(candidate) = directory {
        if let Ok(manifest) = std::fs::read_to_string(root.join(candidate).join("Cargo.toml")) {
            if let Some(name) = crate::crate_move::declared_package_name(&manifest) {
                return Ok((candidate.to_path_buf(), name.to_string()));
            }
        }
        directory = candidate.parent();
    }
    Err(malformed(format!(
        "{file} is in no package: no `Cargo.toml` declaring a `[package]` sits above it{}",
        repo_root_hint(root, file)
    )))
}

/// The sentence naming the repo-root path(s) to write, when `file` exists below some package.
///
/// Every path a plan carries is relative to the repo root; an author standing in a package writes
/// the path relative to it. The path is never resolved for them (that would be a fallback): the
/// refusal only says what to write. Empty when no package holds `file`.
fn repo_root_hint(root: &Path, file: &str) -> String {
    let mut candidates = Vec::new();
    collect_package_files(root, Path::new(""), Path::new(file), &mut candidates);
    candidates.sort();
    match candidates.as_slice() {
        [] => String::new(),
        [only] => format!("; write it from the repo root: `{only}`"),
        several => format!(
            "; several packages hold such a file, write it from the repo root as one of: {}",
            several
                .iter()
                .map(|path| format!("`{path}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// Every `<package dir>/<file>` that exists below `directory` (relative to `root`), as repo-root paths.
fn collect_package_files(root: &Path, directory: &Path, file: &Path, found: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(root.join(directory)) else {
        return;
    };
    let is_package = std::fs::read_to_string(root.join(directory).join("Cargo.toml"))
        .is_ok_and(|manifest| crate::crate_move::declared_package_name(&manifest).is_some());
    if is_package && root.join(directory).join(file).is_file() {
        found.push(directory.join(file).to_string_lossy().replace('\\', "/"));
    }
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_text = name.to_string_lossy();
        let is_build_or_hidden =
            name_text.starts_with('.') || matches!(&*name_text, "target" | "node_modules");
        if !is_build_or_hidden && entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            collect_package_files(root, &directory.join(&name), file, found);
        }
    }
}

/// The absolute range an item anchor names, given where its item was found.
///
/// `start`/`end` absent means the item itself, at its name. A range reaching outside the item is
/// refused as malformed.
pub fn absolute_range(
    resolved: &ResolvedItem,
    start: Option<Position>,
    end: Option<Position>,
) -> Result<Range> {
    range_within(None, resolved, start, end)
}

/// [`absolute_range`], refusing in the words of the item the anchor names.
fn absolute_range_of(
    item: &ItemPath,
    resolved: &ResolvedItem,
    start: Option<Position>,
    end: Option<Position>,
) -> Result<Range> {
    range_within(Some(item), resolved, start, end)
}

fn range_within(
    item: Option<&ItemPath>,
    resolved: &ResolvedItem,
    start: Option<Position>,
    end: Option<Position>,
) -> Result<Range> {
    let (start, end) = match (start, end) {
        (None, None) => {
            return Ok(Range {
                start: resolved.name,
                end: resolved.name,
            })
        }
        (Some(start), Some(end)) => (start, end),
        _ => {
            return Err(malformed(
                "a relative range needs both a `start` and an `end`, or neither",
            ))
        }
    };

    let first_line = resolved.range.start.line;
    let lines = resolved.range.end.line - first_line + 1;
    let absolute = |relative: Position| Position {
        line: first_line + relative.line - 1,
        col: relative.col,
    };
    let (from, to) = (absolute(start), absolute(end));

    let ends_after_the_item = (to.line, to.col) > (resolved.range.end.line, resolved.range.end.col);
    if start.line < 1 || end.line < 1 || end.line > lines || ends_after_the_item {
        return Err(malformed(format!(
            "the range {}:{}–{}:{} reaches outside {}, which is {lines} line(s) long",
            start.line,
            start.col,
            end.line,
            end.col,
            item.map_or_else(|| "the item".to_string(), |item| format!("`{item}`")),
        )));
    }
    Ok(Range {
        start: from,
        end: to,
    })
}

/// Every item anchor in `plan` resolved against the tree under `root` and lowered into the
/// snapshot coordinates the ledger translates.
///
/// Every `v1` anchor passes through untouched, so a plan with no item anchors comes back equal.
/// An item whose fingerprint no longer matches is refused with
/// [`crate::RestructureError::ItemChanged`], naming it.
pub fn resolve_item_anchors(
    plan: &Plan,
    root: &Path,
    resolver: &mut dyn ItemResolver,
) -> Result<Plan> {
    let mut lowered = plan.clone();
    for op in &mut lowered.ops {
        op.anchor = lower(&op.anchor, root, resolver)?;
        for member in &mut op.also {
            *member = lower(member, root, resolver)?;
        }
    }
    Ok(lowered)
}

/// The range `anchor` covers in the tree under `root`, the way a run would resolve it.
///
/// For a caller that reports where an anchor lands — an anchor it has just built, or one read out
/// of a plan — without running the plan. An anchor that names no range (a `symbol`) is refused.
pub fn span_of(anchor: &Anchor, root: &Path, resolver: &mut dyn ItemResolver) -> Result<Range> {
    match lower(anchor, root, resolver)? {
        Anchor::Range { start, end, .. } => Ok(Range { start, end }),
        _ => Err(malformed("a `symbol` anchor names no range")),
    }
}

/// Whether any anchor of any operation in `plan` is an item anchor, and so needs resolving.
pub fn has_item_anchors(plan: &Plan) -> bool {
    plan.ops
        .iter()
        .flat_map(|op| op.anchors())
        .any(|anchor| matches!(anchor, Anchor::Item { .. } | Anchor::Items { .. }))
}

/// One anchor in snapshot coordinates: an item anchor becomes the range it names, anything else is
/// already there.
fn lower(anchor: &Anchor, root: &Path, resolver: &mut dyn ItemResolver) -> Result<Anchor> {
    match anchor {
        Anchor::Item {
            item,
            file,
            start,
            end,
            fingerprint,
            ..
        } => {
            let found = resolver.resolve_item(file, item)?;
            refuse_if_changed(item, file, fingerprint, &found)?;
            let range = absolute_range_of(item, &found, *start, *end)?;
            Ok(Anchor::Range {
                file: file.clone(),
                start: range.start,
                end: range.end,
            })
        }
        Anchor::Items {
            file,
            items,
            fingerprints,
        } => {
            let mut found = Vec::with_capacity(items.len());
            for (item, fingerprint) in items.iter().zip(fingerprints) {
                let resolved = resolver.resolve_item(file, item)?;
                refuse_if_changed(item, file, fingerprint, &resolved)?;
                found.push(resolved);
            }
            let range = covering_run(root, file, items, &found)?;
            Ok(Anchor::Range {
                file: file.clone(),
                start: range.start,
                end: range.end,
            })
        }
        other => Ok(other.clone()),
    }
}

fn refuse_if_changed(
    item: &ItemPath,
    file: &str,
    fingerprint: &Fingerprint,
    found: &ResolvedItem,
) -> Result<()> {
    if &found.fingerprint == fingerprint {
        return Ok(());
    }
    Err(RestructureError::ItemChanged {
        item: item.to_string(),
        file: file.to_string(),
    })
}

/// The one range from the first item's first line to the last item's last, refusing items with
/// anything between them.
///
/// A seam is one contiguous range, so a span reaching from one item to a distant one would carry
/// everything in between — silently, and with nothing in the plan to show it. Only blank lines may
/// separate two items; the trivia that belongs to an item is already inside its own range.
fn covering_run(
    root: &Path,
    file: &str,
    items: &[ItemPath],
    found: &[ResolvedItem],
) -> Result<Range> {
    let (Some(first), Some(last)) = (found.first(), found.last()) else {
        return Err(malformed("an `items` anchor names no items"));
    };

    let text = std::fs::read_to_string(root.join(file))?;
    let lines: Vec<&str> = text.split('\n').collect();
    for (pair, names) in found.windows(2).zip(items.windows(2)) {
        let (before, after) = (&pair[0].range, &pair[1].range);
        let between = (before.end.line as usize)..(after.start.line as usize).saturating_sub(1);
        let in_order = after.start.line > before.end.line;
        let only_blank = lines
            .get(between)
            .is_some_and(|between| between.iter().all(|line| line.trim().is_empty()));
        if !in_order || !only_blank {
            return Err(RestructureError::SeamRefused(format!(
                "the named items are not adjacent: `{}` and `{}` {}, and a seam is one \
                 contiguous range — a span reaching from one to the other would carry everything \
                 in between.",
                names[0],
                names[1],
                if in_order {
                    "have other lines between them"
                } else {
                    "are not in source order"
                }
            )));
        }
    }

    Ok(Range {
        start: Position {
            line: first.range.start.line,
            col: 1,
        },
        end: last.range.end,
    })
}

/// The names an `--items` value carries: split at the commas outside `<..>`, trimmed, with empty
/// elements dropped.
///
/// The one place this is decided, for every front end that takes `--items` — the in-process CLI,
/// the daemon's own command line, `tddy-tools`, and the legacy flag parser. A comma inside `<..>`
/// belongs to the type (`<Pair<A, B>>` is one item), so clap's `value_delimiter = ','` cannot do
/// it; and without the trimming `--items "One, Two"` would name an item literally called `" Two"`,
/// and `--items "A,,B"` an unnamed one — a wrong answer with no error.
pub fn parse_item_list(list: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut depth = 0usize;
    let mut from = 0usize;
    for (at, character) in list.char_indices() {
        match character {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                names.push(&list[from..at]);
                from = at + 1;
            }
            _ => {}
        }
    }
    names.push(&list[from..]);
    names
        .into_iter()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect()
}

/// The `items` anchor over `names`, items of the module `file` is — what `anchors --items` emits.
///
/// A name is bare (`Alpha`) or already begins with the file's own module path (`krate::m::Alpha`),
/// which is not prefixed a second time; any other name containing `::` is refused saying what to
/// pass. Each is resolved like any other item path, so the anchor carries the fingerprints a later
/// run checks it against; a name `file` does not define at module level is refused naming it.
pub fn items_anchor(
    root: &Path,
    file: &str,
    names: &[String],
    resolver: &mut dyn ItemResolver,
) -> Result<Anchor> {
    let module = module_path_of(root, file)?.join("::");
    let items = names
        .iter()
        .map(|name| ItemPath::parse(&qualified_in_module(&module, file, name)?))
        .collect::<Result<Vec<_>>>()?;

    let found = items
        .iter()
        .map(|item| resolver.resolve_item(file, item))
        .collect::<Result<Vec<_>>>()?;
    covering_run(root, file, &items, &found)?;

    Ok(Anchor::Items {
        file: file.to_string(),
        fingerprints: found.into_iter().map(|item| item.fingerprint).collect(),
        items,
    })
}

/// `name` as a full item path of `module`: bare names get the prefix, names that already carry it
/// keep it, and a name qualified by anything else is refused.
fn qualified_in_module(module: &str, file: &str, name: &str) -> Result<String> {
    let own_prefix = format!("{module}::");
    if name.starts_with(&own_prefix) {
        return Ok(name.to_string());
    }
    // `::` inside angle brackets (`<Pair<a::B, C>>`) belongs to a type, not to a module path.
    if !name.contains("::") || split_path(name).is_some_and(|pieces| pieces.len() == 1) {
        return Ok(format!("{own_prefix}{name}"));
    }
    let bare = name.rsplit("::").next().unwrap_or(name);
    Err(malformed(format!(
        "`{name}` is not a bare item name of module `{module}`: pass the names `{file}` declares \
         (`{bare}`) or full paths beginning with `{own_prefix}` (`{own_prefix}{bare}`)"
    )))
}

/// The item anchor for the innermost item enclosing `range` in `file` — what `anchors --at` emits.
pub fn item_anchor_at(
    root: &Path,
    file: &str,
    range: Range,
    resolver: &mut dyn ItemAtResolver,
) -> Result<Anchor> {
    if !root.join(file).is_file() {
        return Err(malformed(format!(
            "{file} is not a file under {}",
            root.display()
        )));
    }

    let (item, found) = resolver.item_enclosing(file, range)?;
    let first_line = found.range.start.line;
    let relative = |position: Position| -> Result<Position> {
        position
            .line
            .checked_sub(first_line)
            .map(|above| Position {
                line: above + 1,
                col: position.col,
            })
            .ok_or_else(|| {
                malformed(format!(
                    "the position {}:{} lies above `{item}`, which starts at line {first_line}",
                    position.line, position.col
                ))
            })
    };

    Ok(Anchor::Item {
        start: Some(relative(range.start)?),
        end: Some(relative(range.end)?),
        fingerprint: found.fingerprint,
        hint: Some(range.start),
        item,
        file: file.to_string(),
    })
}

/// The refusal for an item anchor that reached `reached` without having been lowered at run open.
///
/// Arriving here is the caller's defect, never the plan's: every entry point that runs a plan lowers
/// it through [`resolve_item_anchors`] first, and an anchor that slips past would be read as though
/// its relative coordinates were absolute ones.
pub(crate) fn unlowered_item_anchor(anchor: &Anchor, reached: &str) -> RestructureError {
    let named = match anchor {
        Anchor::Item { item, .. } => format!("`{item}`"),
        Anchor::Items { items, .. } => items
            .iter()
            .map(|item| format!("`{item}`"))
            .collect::<Vec<_>>()
            .join(", "),
        Anchor::Symbol { .. } | Anchor::Range { .. } => "a position".to_string(),
    };
    RestructureError::ServerDefect(format!(
        "the item anchor for {named} in {} reached {reached} without being resolved at run open",
        anchor.file()
    ))
}

fn malformed(reason: impl Into<String>) -> RestructureError {
    RestructureError::MalformedPlan(reason.into())
}

/// A backend that can name the innermost item enclosing a position.
pub trait ItemAtResolver {
    /// The path of the innermost item in `file` whose full range contains `range`, and where it is.
    fn item_enclosing(&mut self, file: &str, range: Range) -> Result<(ItemPath, ResolvedItem)>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_list_is_one_name_per_top_level_comma_trimmed() {
        // Given a list written the way a person types one
        // When it is split
        // Then each name is trimmed and an empty element is dropped
        assert_eq!(
            parse_item_list("One, Two ,,Three,"),
            ["One", "Two", "Three"]
        );
    }

    #[test]
    fn a_comma_inside_angle_brackets_belongs_to_the_type() {
        // Given an impl block of a generic type between two plain items
        // When the list is split
        // Then the generic name stays whole
        assert_eq!(
            parse_item_list("One,<Pair<A, B>>,Two"),
            ["One", "<Pair<A, B>>", "Two"]
        );
    }

    #[test]
    fn a_nested_generic_self_type_with_several_commas_stays_whole() {
        // Given brackets nested two deep with commas at both levels
        // When the list is split
        // Then it is still one name
        assert_eq!(
            parse_item_list("<Map<K, Vec<(A, B)>>>#2,after"),
            ["<Map<K, Vec<(A, B)>>>#2", "after"]
        );
    }

    #[test]
    fn an_empty_item_list_names_nothing() {
        // Given a blank value
        // When it is split
        // Then there are no names
        assert!(parse_item_list("  , ,").is_empty());
    }

    fn a_resolved_item(first_line: u32, last_line: u32) -> ResolvedItem {
        ResolvedItem {
            range: Range {
                start: Position {
                    line: first_line,
                    col: 5,
                },
                end: Position {
                    line: last_line,
                    col: 6,
                },
            },
            name: Position {
                line: first_line,
                col: 12,
            },
            fingerprint: Fingerprint("sha256:ab".to_string()),
        }
    }

    fn a_package(files: &[(&str, &str)]) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let absolute = root.path().join(path);
            std::fs::create_dir_all(absolute.parent().unwrap()).unwrap();
            std::fs::write(absolute, text).unwrap();
        }
        root
    }

    const MANIFEST: &str = "[package]\nname = \"tddy-core\"\nversion = \"0.1.0\"\n";

    #[test]
    fn a_relative_range_is_counted_from_the_items_first_line() {
        // Given an item on lines 40–50
        let item = a_resolved_item(40, 50);

        // When its lines 2–3 are made absolute
        let range = absolute_range(
            &item,
            Some(Position { line: 2, col: 9 }),
            Some(Position { line: 3, col: 33 }),
        );

        // Then they are lines 41–42, columns unchanged
        assert_eq!(
            range.ok(),
            Some(Range {
                start: Position { line: 41, col: 9 },
                end: Position { line: 42, col: 33 },
            })
        );
    }

    #[test]
    fn no_relative_range_names_the_item_at_its_name() {
        let item = a_resolved_item(40, 50);

        assert_eq!(
            absolute_range(&item, None, None).ok(),
            Some(Range {
                start: Position { line: 40, col: 12 },
                end: Position { line: 40, col: 12 },
            })
        );
    }

    #[test]
    fn a_relative_range_past_the_items_last_line_is_refused() {
        let item = a_resolved_item(40, 44);

        let refused = absolute_range(
            &item,
            Some(Position { line: 2, col: 9 }),
            Some(Position { line: 9, col: 1 }),
        );

        assert!(matches!(
            refused,
            Err(crate::RestructureError::MalformedPlan(_))
        ));
    }

    #[test]
    fn a_file_under_src_is_the_module_its_path_names() {
        // Given a package named `tddy-core`
        let root = a_package(&[
            ("packages/core/Cargo.toml", MANIFEST),
            ("packages/core/src/workflow/stack.rs", ""),
        ]);

        // When the module path of a nested file is asked for
        let path = module_path_of(root.path(), "packages/core/src/workflow/stack.rs");

        // Then the crate reads `-` as `_` and the directories are modules
        assert_eq!(
            path.ok(),
            Some(vec![
                "tddy_core".to_string(),
                "workflow".to_string(),
                "stack".to_string()
            ])
        );
    }

    #[test]
    fn a_mod_rs_is_the_module_its_directory_names() {
        let root = a_package(&[
            ("packages/core/Cargo.toml", MANIFEST),
            ("packages/core/src/workflow/mod.rs", ""),
        ]);

        assert_eq!(
            module_path_of(root.path(), "packages/core/src/workflow/mod.rs").ok(),
            Some(vec!["tddy_core".to_string(), "workflow".to_string()])
        );
    }

    #[test]
    fn lib_rs_is_the_crate_root() {
        let root = a_package(&[
            ("packages/core/Cargo.toml", MANIFEST),
            ("packages/core/src/lib.rs", ""),
        ]);

        assert_eq!(
            module_path_of(root.path(), "packages/core/src/lib.rs").ok(),
            Some(vec!["tddy_core".to_string()])
        );
    }

    #[test]
    fn a_file_outside_src_is_refused() {
        let root = a_package(&[
            ("packages/core/Cargo.toml", MANIFEST),
            ("packages/core/tests/golden.rs", ""),
        ]);

        assert!(matches!(
            module_path_of(root.path(), "packages/core/tests/golden.rs"),
            Err(crate::RestructureError::MalformedPlan(_))
        ));
    }

    /// Resolves every item to the one `ResolvedItem` it was built with, whatever the file.
    struct Resolving(Vec<(String, ResolvedItem)>);

    impl ItemResolver for Resolving {
        fn resolve_item(&mut self, _file: &str, item: &ItemPath) -> Result<ResolvedItem> {
            self.0
                .iter()
                .find(|(path, _)| path == item.as_str())
                .map(|(_, resolved)| resolved.clone())
                .ok_or_else(|| malformed(format!("`{item}` is not declared")))
        }
    }

    fn an_item_anchor(item: &str, fingerprint: &str, start: Option<Position>) -> Anchor {
        Anchor::Item {
            item: ItemPath::parse(item).unwrap(),
            file: "src/lib.rs".to_string(),
            start,
            end: start,
            fingerprint: Fingerprint(fingerprint.to_string()),
            hint: None,
        }
    }

    fn a_plan_anchored_at(anchor: Anchor) -> Plan {
        let jsonl = format!(
            "{{\"v\":2,\"files\":{{}}}}\n{{\"op\":\"rename_symbol\",\"anchor\":{},\"name\":\"B\"}}\n",
            serde_json::to_string(&anchor).unwrap()
        );
        Plan::parse(&jsonl).unwrap()
    }

    #[test]
    fn an_item_anchor_is_lowered_to_the_range_its_item_sits_at() {
        // Given an item resolved at lines 40–50 and an anchor on its second line
        let plan = a_plan_anchored_at(an_item_anchor(
            "tddy_core::A::f",
            "sha256:ab",
            Some(Position { line: 2, col: 9 }),
        ));
        let mut resolver = Resolving(vec![(
            "tddy_core::A::f".to_string(),
            a_resolved_item(40, 50),
        )]);

        // When the plan is lowered
        let lowered = resolve_item_anchors(&plan, Path::new("."), &mut resolver).unwrap();

        // Then the anchor is the absolute range, in the file the item was found in
        assert_eq!(
            lowered.ops[0].anchor,
            Anchor::Range {
                file: "src/lib.rs".to_string(),
                start: Position { line: 41, col: 9 },
                end: Position { line: 41, col: 9 },
            }
        );
    }

    #[test]
    fn a_plan_with_no_item_anchor_is_lowered_to_itself() {
        let plan = Plan::parse(
            r#"{"v":1,"snapshot":{}}
{"op":"rename_symbol","anchor":{"kind":"symbol","file":"src/lib.rs","path":"A"},"name":"B"}
"#,
        )
        .unwrap();

        let lowered = resolve_item_anchors(&plan, Path::new("."), &mut Resolving(vec![]));

        assert_eq!(lowered.ok(), Some(plan));
    }

    #[test]
    fn an_item_whose_text_changed_is_refused_naming_it() {
        // Given an anchor fingerprinted over text the item no longer has
        let plan = a_plan_anchored_at(an_item_anchor("tddy_core::A::f", "sha256:old", None));
        let mut resolver = Resolving(vec![("tddy_core::A::f".to_string(), a_resolved_item(1, 3))]);

        // When the plan is lowered
        let refused = resolve_item_anchors(&plan, Path::new("."), &mut resolver);

        // Then the item and its file are named
        assert_eq!(
            refused.map(|_| ()).map_err(|error| error.to_string()),
            Err(
                "the item `tddy_core::A::f` in src/lib.rs changed since the plan was written — \
                 its fingerprint no longer matches; re-anchor it with `restructure anchors`"
                    .to_string()
            )
        );
    }

    #[test]
    fn items_with_other_code_between_them_are_not_one_run() {
        // Given two items on lines 1–2 and 5–6, with a statement on line 3
        let root = a_package(&[(
            "src/lib.rs",
            "struct A;\nstruct A2;\nconst X: u8 = 1;\n\nstruct B;\nstruct B2;\n",
        )]);
        let items = [
            ItemPath::parse("c::A").unwrap(),
            ItemPath::parse("c::B").unwrap(),
        ];
        let found = [a_resolved_item(1, 2), a_resolved_item(5, 6)];

        // When they are asked to be covered as one run
        let refused = covering_run(root.path(), "src/lib.rs", &items, &found);

        // Then the gap is named
        assert!(refused
            .map(|_| ())
            .map_err(|error| error.to_string())
            .unwrap_err()
            .contains("`c::A` and `c::B` have other lines between them"),);
    }

    #[test]
    fn items_separated_only_by_blank_lines_are_one_run() {
        let root = a_package(&[("src/lib.rs", "struct A;\n\n\nstruct B;\n")]);
        let items = [
            ItemPath::parse("c::A").unwrap(),
            ItemPath::parse("c::B").unwrap(),
        ];
        let found = [a_resolved_item(1, 1), a_resolved_item(4, 4)];

        let run = covering_run(root.path(), "src/lib.rs", &items, &found);

        assert_eq!(
            run.ok(),
            Some(Range {
                start: Position { line: 1, col: 1 },
                end: Position { line: 4, col: 6 },
            })
        );
    }

    fn a_queue_module_declaring_one_struct() -> tempfile::TempDir {
        a_package(&[
            ("packages/core/Cargo.toml", MANIFEST),
            ("packages/core/src/queue.rs", "pub struct Alpha;\n"),
        ])
    }

    fn the_anchor_over(root: &Path, name: &str) -> Result<Anchor> {
        let mut resolver = Resolving(vec![(
            "tddy_core::queue::Alpha".to_string(),
            a_resolved_item(1, 1),
        )]);
        items_anchor(
            root,
            "packages/core/src/queue.rs",
            &[name.to_string()],
            &mut resolver,
        )
    }

    #[test]
    fn a_bare_item_name_is_taken_as_an_item_of_the_files_module() {
        // Given a module `tddy_core::queue` declaring `Alpha`
        let root = a_queue_module_declaring_one_struct();

        // When the anchor is asked for over the bare name
        let anchor = the_anchor_over(root.path(), "Alpha");

        // Then it names the item by its full path
        assert_eq!(
            anchor.ok(),
            Some(Anchor::Items {
                file: "packages/core/src/queue.rs".to_string(),
                items: vec![ItemPath::parse("tddy_core::queue::Alpha").unwrap()],
                fingerprints: vec![Fingerprint("sha256:ab".to_string())],
            })
        );
    }

    #[test]
    fn a_name_already_qualified_by_the_files_module_is_not_prefixed_twice() {
        // Given a module `tddy_core::queue` declaring `Alpha`
        let root = a_queue_module_declaring_one_struct();

        // When the anchor is asked for over the fully qualified name
        let qualified = the_anchor_over(root.path(), "tddy_core::queue::Alpha");

        // Then it is the very anchor the bare name gives
        assert_eq!(qualified.ok(), the_anchor_over(root.path(), "Alpha").ok());
    }

    #[test]
    fn a_name_qualified_by_another_module_is_refused_saying_what_to_pass() {
        // Given a module `tddy_core::queue` declaring `Alpha`
        let root = a_queue_module_declaring_one_struct();

        // When the anchor is asked for over a path in some other module
        let refused = the_anchor_over(root.path(), "other::Alpha");

        // Then the refusal names the module and both accepted spellings
        assert_eq!(
            refused.map(|_| ()).map_err(|error| error.to_string()),
            Err("plan is malformed: `other::Alpha` is not a bare item name of module `tddy_core::queue`: pass the \
                 names `packages/core/src/queue.rs` declares (`Alpha`) or full paths beginning \
                 with `tddy_core::queue::` (`tddy_core::queue::Alpha`)"
                .to_string())
        );
    }

    #[test]
    fn a_struct_its_impl_block_and_a_function_after_it_are_one_run() {
        let root = a_package(&[(
            "src/lib.rs",
            "struct A;

impl A {
    fn go(&self) {}
}

fn describe(a: &A) {}
",
        )]);
        let items = ["c::A", "c::<A>", "c::describe"].map(|path| ItemPath::parse(path).unwrap());
        let found = [
            a_resolved_item(1, 1),
            a_resolved_item(3, 5),
            a_resolved_item(7, 7),
        ];

        let run = covering_run(root.path(), "src/lib.rs", &items, &found);

        assert_eq!(
            run.ok(),
            Some(Range {
                start: Position { line: 1, col: 1 },
                end: Position { line: 7, col: 6 },
            })
        );
    }

    fn the_anchor_over_impl_names(root: &Path, names: &[&str]) -> Result<Anchor> {
        let mut resolver = Resolving(
            names
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    let line = index as u32 + 1;
                    (
                        format!("tddy_core::queue::{name}"),
                        a_resolved_item(line, line),
                    )
                })
                .collect(),
        );
        let names: Vec<String> = names.iter().map(|name| name.to_string()).collect();
        items_anchor(root, "packages/core/src/queue.rs", &names, &mut resolver)
    }

    #[test]
    fn an_inherent_impl_name_is_a_bare_item_name_of_the_files_module() {
        let root = a_queue_module_declaring_one_struct();

        let anchor = the_anchor_over_impl_names(root.path(), &["<Alpha>#2", "<Pair<a::B, C>>"]);

        assert_eq!(
            anchor.ok().map(|anchor| match anchor {
                Anchor::Items { items, .. } => items.iter().map(ToString::to_string).collect(),
                _ => Vec::new(),
            }),
            Some(vec![
                "tddy_core::queue::<Alpha>#2".to_string(),
                "tddy_core::queue::<Pair<a::B, C>>".to_string(),
            ])
        );
    }
}
