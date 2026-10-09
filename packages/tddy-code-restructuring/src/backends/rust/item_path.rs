//! Resolving an item path through rust-analyzer's document outline.
//!
//! `textDocument/documentSymbol` answers a tree: modules, types and functions, with each `impl`
//! block a node of its own whose children are its members. An [`ItemPath`]'s segments walk that
//! tree — a type segment matches the type *and* every `impl` of it, a `<T as Trait>` segment only
//! that trait's `impl` — and the walk refuses rather than picks when a segment matches twice.

use serde_json::Value;

use super::{attached_trivia_starts_at, failure, server_defect, uri_of, LspPoint, RustBackend};
use crate::edit::{Position, Range};
use crate::item_anchor::prefix::segments_below;
use crate::item_anchor::{module_path_of, ItemAtResolver, ItemResolver, ResolvedItem};
use crate::plan::{Fingerprint, ItemPath, ItemSegment};
use crate::{RestructureError, Result};

/// Where a walk of the outline ended: the node's full range and its name's position, zero-based as
/// the server reports them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OutlineHit {
    pub(crate) range: Value,
    pub(crate) selection_start: Value,
}

/// Why a walk of the outline found no single node.
#[derive(Debug, PartialEq, Eq)]
enum Miss {
    /// Nothing in the outline answers to this segment.
    Absent(String),
    /// More than one node answers to the last segment.
    Ambiguous(usize),
    /// A node answered, and its answer carries no range to resolve to.
    Unreadable(String),
    /// `<Type>#N` names an inherent impl that is not there: only `count` exist.
    NoSuchImpl { ordinal: usize, count: usize },
}

impl Miss {
    /// The refusal for `item`, said in terms of the file it was looked for in.
    fn refusal(self, item: &ItemPath, file: &str) -> RestructureError {
        match self {
            Miss::Absent(segment) => failure(format!(
                "`{item}` is not declared in {file}: nothing there is named `{segment}`"
            )),
            Miss::Ambiguous(count) => failure(format!(
                "`{item}` names {count} items in {file}{}",
                ambiguity_hint(item)
            )),
            Miss::NoSuchImpl { ordinal, count } => failure(format!(
                "`{item}` names inherent impl #{ordinal}, but {file} has only {count} inherent \
                 impl block{} of it",
                if count == 1 { "" } else { "s" }
            )),
            Miss::Unreadable(segment) => server_defect(format!(
                "the outline of {file} answers `{segment}` without a range"
            )),
        }
    }
}

/// How to tell apart the items an ambiguous `item` names: number an inherent impl, or qualify a
/// member with its trait.
fn ambiguity_hint(item: &ItemPath) -> String {
    if matches!(
        item.segments().last(),
        Some(ItemSegment::InherentImpl { nth: None, .. })
    ) {
        return format!(" — number the impl block, as `{item}#N` (`<Type>#N`, counted from 1)");
    }
    qualification_hint(item)
        .map(|qualified| format!(" — qualify it with the trait, as `{qualified}`"))
        .unwrap_or_default()
}

/// `item` with its parent segment written as `<Parent as Trait>`, when it has a parent to write so.
fn qualification_hint(item: &ItemPath) -> Option<String> {
    let segments = item.segments();
    let [before @ .., ItemSegment::Named(parent), last] = segments.as_slice() else {
        return None;
    };
    let mut pieces = vec![item.crate_name().to_string()];
    pieces.extend(before.iter().map(ItemSegment::spelled));
    pieces.push(format!("<{parent} as Trait>"));
    pieces.push(last.spelled());
    Some(pieces.join("::"))
}

/// Walk a `documentSymbol` answer down `segments`, refusing in the words of "this file".
///
/// The walk itself is [`walk`]; this is it spelled for a caller that has no file to name.
#[cfg(test)]
pub(crate) fn walk_outline(
    symbols: &Value,
    item: &ItemPath,
    segments: &[ItemSegment],
) -> Result<OutlineHit> {
    walk(symbols, segments).map_err(|miss| miss.refusal(item, "this file"))
}

/// Walk a `documentSymbol` answer down `segments`.
///
/// A segment matches every node it names: a module or type by its own name, and — before the last
/// segment — every `impl` of a type by the type it implements for, so members of several `impl`
/// blocks of one type are all in reach of the next segment. The last segment must name exactly one
/// node.
fn walk(symbols: &Value, segments: &[ItemSegment]) -> std::result::Result<OutlineHit, Miss> {
    let mut scopes = vec![symbols];

    for (index, segment) in segments.iter().enumerate() {
        let last = index + 1 == segments.len();
        let matches: Vec<&Value> = scopes
            .iter()
            .flat_map(|scope| scope.as_array().into_iter().flatten())
            .filter(|node| answers_to(node, segment, last))
            .collect();
        let matches = select_ordinal(matches, segment)?;

        match matches.as_slice() {
            [] => return Err(Miss::Absent(segment.spelled())),
            [node] if last => return hit_of(node, segment),
            many if last => return Err(Miss::Ambiguous(many.len())),
            _ => {}
        }
        scopes = matches
            .iter()
            .filter_map(|node| node.get("children"))
            .collect();
    }

    Err(Miss::Absent(String::new()))
}

/// The `nth` inherent impl among `matches` when `segment` numbers one, in source order; a number
/// beyond the blocks there are is a refusal, while no blocks at all is left to read as absent.
fn select_ordinal<'a>(
    matches: Vec<&'a Value>,
    segment: &ItemSegment,
) -> std::result::Result<Vec<&'a Value>, Miss> {
    let ItemSegment::InherentImpl {
        nth: Some(ordinal), ..
    } = segment
    else {
        return Ok(matches);
    };
    match matches.get(ordinal - 1) {
        Some(node) => Ok(vec![node]),
        None if matches.is_empty() => Ok(matches),
        None => Err(Miss::NoSuchImpl {
            ordinal: *ordinal,
            count: matches.len(),
        }),
    }
}

fn hit_of(node: &Value, segment: &ItemSegment) -> std::result::Result<OutlineHit, Miss> {
    match (node.get("range"), node.pointer("/selectionRange/start")) {
        (Some(range), Some(selection_start)) => Ok(OutlineHit {
            range: range.clone(),
            selection_start: selection_start.clone(),
        }),
        _ => Err(Miss::Unreadable(segment.spelled())),
    }
}

/// Whether an outline node is what `segment` names.
fn answers_to(node: &Value, segment: &ItemSegment, last: bool) -> bool {
    let Some(name) = node.get("name").and_then(Value::as_str) else {
        return false;
    };
    match segment {
        ItemSegment::Named(wanted) => {
            name == wanted
                || (!last
                    && read_impl(name).is_some_and(|block| base_name(block.self_type) == wanted))
        }
        ItemSegment::TraitImpl {
            self_type,
            trait_name,
        } => read_impl(name).is_some_and(|block| {
            base_name(block.self_type) == base_name(self_type)
                && block
                    .trait_name
                    .is_some_and(|implemented| base_name(implemented) == base_name(trait_name))
        }),
        ItemSegment::InherentImpl { self_type, .. } => read_impl(name).is_some_and(|block| {
            block.trait_name.is_none() && base_name(block.self_type) == base_name(self_type)
        }),
    }
}

/// What an `impl` block's outline label says: `impl Stack` or `impl Display for Stack`.
struct ImplLabel<'a> {
    trait_name: Option<&'a str>,
    self_type: &'a str,
}

fn read_impl(label: &str) -> Option<ImplLabel<'_>> {
    let rest = label.strip_prefix("impl")?;
    let rest = match rest.chars().next()? {
        '<' => after_generics(rest)?,
        ' ' => rest,
        _ => return None,
    };
    let rest = rest.trim();
    Some(match rest.split_once(" for ") {
        Some((implemented, self_type)) => ImplLabel {
            trait_name: Some(implemented.trim()),
            self_type: self_type.trim(),
        },
        None => ImplLabel {
            trait_name: None,
            self_type: rest,
        },
    })
}

/// `text` without its leading `<…>` generic parameter list.
fn after_generics(text: &str) -> Option<&str> {
    let mut depth = 0usize;
    for (at, character) in text.char_indices() {
        match character {
            '<' => depth += 1,
            '>' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(&text[at + 1..]);
                }
            }
            _ => {}
        }
    }
    None
}

/// A type or trait as its bare name: no reference, no generic arguments, no path in front of it.
fn base_name(written: &str) -> &str {
    let written = written
        .trim()
        .trim_start_matches('&')
        .trim_start_matches("mut ")
        .trim_start_matches("dyn ")
        .trim();
    let without_arguments = written.split('<').next().unwrap_or(written);
    without_arguments
        .rsplit("::")
        .next()
        .unwrap_or(without_arguments)
        .trim()
}

/// An open document with the outline the server gave for it.
struct Outlined {
    root: std::path::PathBuf,
    text: String,
    symbols: Value,
}

impl RustBackend {
    /// Open `file` and read its outline once the server can give one.
    ///
    /// The documents opened here are the caller's to close: both entry points go through
    /// [`RustBackend::closing_what_it_opens`].
    fn outline_of(&mut self, file: &str) -> Result<Outlined> {
        let root = self.workspace_root()?;
        let absolute = root.join(file);
        let text = std::fs::read_to_string(&absolute)
            .map_err(|error| failure(format!("{file} could not be read: {error}")))?;
        let uri = uri_of(&absolute);

        self.start(&root)?;
        self.did_open(&uri, &text)?;
        let symbols = self.settled_outline(&uri)?;
        Ok(Outlined {
            root,
            text,
            symbols,
        })
    }

    fn resolve_item_opening(&mut self, file: &str, item: &ItemPath) -> Result<ResolvedItem> {
        let outlined = self.outline_of(file)?;
        let module = module_path_of(&outlined.root, file)?;
        let segments = segments_below(item, &module, file)?;

        let hit = walk(&outlined.symbols, &segments).map_err(|miss| miss.refusal(item, file))?;
        resolved_from(&outlined.text, &hit)
    }

    fn item_enclosing_opening(
        &mut self,
        file: &str,
        range: Range,
    ) -> Result<(ItemPath, ResolvedItem)> {
        let outlined = self.outline_of(file)?;
        let module = module_path_of(&outlined.root, file)?;
        let text = &outlined.text;

        let chain = enclosing_chain(&outlined.symbols, range);

        let Some((innermost, siblings)) = chain.last().copied() else {
            return Err(failure(format!(
                "the position {}:{} is inside no item of {file}",
                range.start.line, range.start.col
            )));
        };

        let mut pieces = module;
        for (node, _) in &chain[..chain.len() - 1] {
            pieces.push(segment_of(node)?);
        }
        pieces.push(segment_in(innermost, siblings)?);

        let item = ItemPath::parse(&pieces.join("::"))?;
        let hit = hit_of(innermost, &ItemSegment::Named(item.to_string()))
            .map_err(|miss| miss.refusal(&item, file))?;
        Ok((item, resolved_from(text, &hit)?))
    }
}

/// The outline nodes enclosing `range`, outermost first, each with the siblings it was chosen from.
fn enclosing_chain(symbols: &Value, range: Range) -> Vec<(&Value, &[Value])> {
    let mut chain = Vec::new();
    let mut level = Some(symbols);
    while let Some(nodes) = level {
        let siblings = nodes.as_array().map(Vec::as_slice).unwrap_or_default();
        let Some(inner) = siblings.iter().find(|node| contains(node, range)) else {
            break;
        };
        chain.push((inner, siblings));
        level = inner.get("children");
    }
    chain
}

impl ItemResolver for RustBackend {
    fn resolve_item(&mut self, file: &str, item: &ItemPath) -> Result<ResolvedItem> {
        self.closing_what_it_opens(|backend| backend.resolve_item_opening(file, item))
    }
}

impl ItemAtResolver for RustBackend {
    fn item_enclosing(&mut self, file: &str, range: Range) -> Result<(ItemPath, ResolvedItem)> {
        self.closing_what_it_opens(|backend| backend.item_enclosing_opening(file, range))
    }
}

/// How a node of the outline is written in an item path: a type's `impl` is its type, a trait's
/// `impl` is qualified with the trait.
fn segment_of(node: &Value) -> Result<String> {
    let name = node
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| server_defect("an outline node carries no name"))?;
    Ok(match read_impl(name) {
        Some(ImplLabel {
            trait_name: Some(implemented),
            self_type,
        }) => ItemSegment::TraitImpl {
            self_type: base_name(self_type).to_string(),
            trait_name: base_name(implemented).to_string(),
        }
        .spelled(),
        Some(ImplLabel {
            trait_name: None,
            self_type,
        }) => base_name(self_type).to_string(),
        None => name.to_string(),
    })
}

/// How `node`, the innermost item an anchor lands in, is written: like [`segment_of`], except that
/// an inherent `impl` is itself the item — `<Type>`, numbered `<Type>#N` when `siblings` hold
/// several inherent impls of the type.
fn segment_in(node: &Value, siblings: &[Value]) -> Result<String> {
    let label = node.get("name").and_then(Value::as_str).unwrap_or_default();
    let Some(ImplLabel {
        trait_name: None,
        self_type,
    }) = read_impl(label)
    else {
        return segment_of(node);
    };
    let wanted = base_name(self_type);
    let same_type: Vec<&Value> = siblings
        .iter()
        .filter(|sibling| {
            sibling
                .get("name")
                .and_then(Value::as_str)
                .and_then(read_impl)
                .is_some_and(|block| {
                    block.trait_name.is_none() && base_name(block.self_type) == wanted
                })
        })
        .collect();
    let nth = same_type
        .iter()
        .position(|sibling| std::ptr::eq(*sibling, node))
        .filter(|_| same_type.len() > 1)
        .map(|index| index + 1);
    Ok(ItemSegment::InherentImpl {
        self_type: wanted.to_string(),
        nth,
    }
    .spelled())
}

/// A server position as the one-based position a plan carries.
///
/// Columns are the server's own, which this client negotiates to be byte offsets: that is the unit
/// every position a plan carries is sent back to the server in, so nothing is converted.
fn plan_position(point: LspPoint) -> Position {
    Position {
        line: point.line as u32 + 1,
        col: point.character as u32 + 1,
    }
}

/// Whether `node`'s extent holds `range`, both as one-based plan positions.
fn contains(node: &Value, range: Range) -> bool {
    let bounds = (
        LspPoint::read(node.pointer("/range/start")),
        LspPoint::read(node.pointer("/range/end")),
    );
    let (Ok(start), Ok(end)) = bounds else {
        return false;
    };
    let (start, end) = (plan_position(start), plan_position(end));
    let key = |position: Position| (position.line, position.col);
    key(start) <= key(range.start) && key(range.end) <= key(end)
}

/// An outline node's extent in the coordinates plans use, and the fingerprint of its lines.
///
/// The extent begins where the comments and attributes attached to the item begin, for the reason
/// `anchors --items` has always begun there: a range that starts at the keyword leaves the item's
/// own documentation behind.
fn resolved_from(text: &str, hit: &OutlineHit) -> Result<ResolvedItem> {
    let start = plan_position(LspPoint::read(hit.range.get("start"))?);
    let end = plan_position(LspPoint::read(hit.range.get("end"))?);
    let name = plan_position(LspPoint::read(Some(&hit.selection_start))?);

    let first_line = attached_trivia_starts_at(text, start.line);
    let start = if first_line == start.line {
        start
    } else {
        Position {
            line: first_line,
            col: 1,
        }
    };

    let item_lines: Vec<&str> = text
        .split('\n')
        .skip(start.line as usize - 1)
        .take(end.line.saturating_sub(start.line) as usize + 1)
        .collect();

    Ok(ResolvedItem {
        range: Range { start, end },
        name,
        fingerprint: Fingerprint::of(&item_lines.join("\n")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// An outline node the way rust-analyzer reports one: zero-based range, children for members.
    fn a_node(name: &str, first: u32, last: u32, children: Value) -> Value {
        json!({
            "name": name,
            "range": { "start": { "line": first, "character": 0 }, "end": { "line": last, "character": 1 } },
            "selectionRange": { "start": { "line": first, "character": 4 }, "end": { "line": first, "character": 8 } },
            "children": children,
        })
    }

    /// `mod workflow` holding `Stack`, two `impl`s of it with a `fmt` each, and `impl Stack { new }`.
    fn an_outline() -> Value {
        json!([a_node(
            "workflow",
            0,
            40,
            json!([
                a_node("Stack", 1, 4, json!([])),
                a_node(
                    "impl Stack",
                    6,
                    11,
                    json!([a_node("new", 7, 10, json!([]))])
                ),
                a_node(
                    "impl Display for Stack",
                    13,
                    17,
                    json!([a_node("fmt", 14, 16, json!([]))])
                ),
                a_node(
                    "impl Debug for Stack",
                    19,
                    23,
                    json!([a_node("fmt", 20, 22, json!([]))])
                ),
            ])
        )])
    }

    fn segments_of(path: &ItemPath) -> Vec<ItemSegment> {
        path.segments()
    }

    #[test]
    fn an_inherent_method_is_found_through_its_types_impl() {
        let path = ItemPath::parse("stacks::workflow::Stack::new").unwrap();

        let hit = walk_outline(&an_outline(), &path, &segments_of(&path));

        assert_eq!(
            hit.ok().map(|hit| hit.range["start"]["line"].clone()),
            Some(json!(7))
        );
    }

    #[test]
    fn a_name_two_trait_impls_define_is_refused() {
        let path = ItemPath::parse("stacks::workflow::Stack::fmt").unwrap();

        let refused = walk_outline(&an_outline(), &path, &segments_of(&path));

        assert!(matches!(
            refused,
            Err(crate::RestructureError::MalformedPlan(_))
        ));
    }

    #[test]
    fn a_trait_qualified_segment_picks_that_traits_impl() {
        let path = ItemPath::parse("stacks::workflow::<Stack as Debug>::fmt").unwrap();

        let hit = walk_outline(&an_outline(), &path, &segments_of(&path));

        assert_eq!(
            hit.ok().map(|hit| hit.range["start"]["line"].clone()),
            Some(json!(20))
        );
    }

    #[test]
    fn a_segment_nothing_answers_to_is_refused() {
        let path = ItemPath::parse("stacks::workflow::Deque::new").unwrap();

        let refused = walk_outline(&an_outline(), &path, &segments_of(&path));

        assert!(matches!(
            refused,
            Err(crate::RestructureError::MalformedPlan(_))
        ));
    }

    #[test]
    fn an_impl_label_names_its_type_and_the_trait_it_implements() {
        let label = read_impl("impl<T> fmt::Display for Wrapper<T>").unwrap();

        assert_eq!(
            (label.trait_name.map(base_name), base_name(label.self_type)),
            (Some("Display"), "Wrapper")
        );
    }

    #[test]
    fn a_function_whose_name_starts_with_impl_is_not_an_impl_block() {
        assert!(read_impl("implement").is_none());
    }

    #[test]
    fn a_member_of_a_generic_impl_is_reached_through_its_bare_type() {
        let outline = json!([
            a_node("Stack", 0, 2, json!([])),
            a_node(
                "impl<T> Stack<T>",
                4,
                9,
                json!([a_node("peek", 5, 8, json!([]))])
            ),
        ]);
        let path = ItemPath::parse("c::Stack::peek").unwrap();

        let hit = walk_outline(&outline, &path, &path.segments());

        assert_eq!(
            hit.ok().map(|hit| hit.range["start"]["line"].clone()),
            Some(json!(5))
        );
    }

    /// `Stack` with `impl Stack` twice (lines 6 and 12), and `impl Display for Stack` between.
    fn an_outline_with_two_inherent_impls() -> Value {
        json!([
            a_node("Stack", 0, 3, json!([])),
            a_node("impl Stack", 6, 9, json!([a_node("new", 7, 8, json!([]))])),
            a_node(
                "impl Display for Stack",
                9,
                11,
                json!([a_node("fmt", 10, 11, json!([]))])
            ),
            a_node(
                "impl Stack",
                12,
                15,
                json!([a_node("peek", 13, 14, json!([]))])
            ),
        ])
    }

    fn first_line_of(hit: Result<OutlineHit>) -> Option<Value> {
        hit.ok().map(|hit| hit.range["start"]["line"].clone())
    }

    fn refusal_of(hit: Result<OutlineHit>) -> String {
        hit.err().map(|error| error.to_string()).unwrap_or_default()
    }

    #[test]
    fn an_inherent_impl_block_is_found_by_its_type() {
        let path = ItemPath::parse("stacks::workflow::<Stack>").unwrap();

        let hit = walk_outline(&an_outline(), &path, &path.segments());

        assert_eq!(first_line_of(hit), Some(json!(6)));
    }

    #[test]
    fn two_inherent_impls_without_an_ordinal_are_ambiguous_and_the_hint_names_the_ordinal() {
        let path = ItemPath::parse("c::<Stack>").unwrap();

        let refused = walk_outline(
            &an_outline_with_two_inherent_impls(),
            &path,
            &path.segments(),
        );

        let message = refusal_of(refused);
        assert!(message.contains("names 2 items"), "{message}");
        assert!(message.contains("`c::<Stack>#N`"), "{message}");
    }

    #[test]
    fn an_ordinal_selects_that_inherent_impl_in_source_order() {
        let path = ItemPath::parse("c::<Stack>#2").unwrap();

        let hit = walk_outline(
            &an_outline_with_two_inherent_impls(),
            &path,
            &path.segments(),
        );

        assert_eq!(first_line_of(hit), Some(json!(12)));
    }

    #[test]
    fn an_ordinal_past_the_last_inherent_impl_is_refused_saying_how_many_exist() {
        let path = ItemPath::parse("c::<Stack>#3").unwrap();

        let refused = walk_outline(
            &an_outline_with_two_inherent_impls(),
            &path,
            &path.segments(),
        );

        let message = refusal_of(refused);
        assert!(message.contains("only 2 inherent impl"), "{message}");
    }

    #[test]
    fn a_trait_impl_of_the_same_type_is_not_an_inherent_impl() {
        let outline = json!([
            a_node("Stack", 0, 3, json!([])),
            a_node("impl Display for Stack", 5, 9, json!([])),
        ]);
        let path = ItemPath::parse("c::<Stack>").unwrap();

        let refused = walk_outline(&outline, &path, &path.segments());

        assert!(refusal_of(refused).contains("nothing there is named `<Stack>`"));
    }

    #[test]
    fn an_inherent_impl_of_a_generic_type_is_found_by_its_written_type() {
        let outline = json!([a_node("impl<T> Wrapper<T>", 2, 6, json!([]))]);
        let path = ItemPath::parse("c::<Wrapper<T>>").unwrap();

        let hit = walk_outline(&outline, &path, &path.segments());

        assert_eq!(first_line_of(hit), Some(json!(2)));
    }

    #[test]
    fn a_position_between_an_impls_members_is_spelled_as_the_impl_block() {
        let outline = an_outline_with_two_inherent_impls();
        let second_impl = &outline[3];
        let siblings = outline.as_array().unwrap();

        let spelled = segment_in(second_impl, siblings).unwrap();

        assert_eq!(spelled, "<Stack>#2");
    }

    #[test]
    fn the_only_inherent_impl_of_a_type_is_spelled_without_an_ordinal() {
        let outline = an_outline();
        let siblings = outline[0]["children"].as_array().unwrap();

        let spelled = segment_in(&siblings[1], siblings).unwrap();

        assert_eq!(spelled, "<Stack>");
    }
}
