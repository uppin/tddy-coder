//! Reading the run of items a move relocates out of the server's outline of the file.

use serde_json::Value;

use super::super::{
    attached_trivia_starts_at, failure, is_identifier, seam_refusal, visibility_in, LspPoint,
    SYMBOL_KIND_IMPL, SYMBOL_KIND_MODULE,
};
use super::text::{line_start, Edit};
use crate::edit::Range;
use crate::Result;

/// One named module-level item, with what a move needs to know about its declaration.
#[derive(Debug, Clone)]
pub(crate) struct Item {
    pub(crate) name: String,
    /// Where the server reports the item's name, which is where a reference query asks about it.
    pub(crate) position: Value,
    /// The visibility as written: `pub`, `pub(crate)`, or empty for private.
    pub(crate) visibility: String,
}

/// The items a move relocates, and the whole lines that hold them.
#[derive(Debug)]
pub(crate) struct Run {
    pub(crate) first_line: u32,
    pub(crate) last_line: u32,
    /// The named items. An `impl` block is part of the lines and has no name to list.
    pub(crate) items: Vec<Item>,
}

fn first_line_of(symbol: &Value) -> Option<u32> {
    Some(symbol.pointer("/range/start/line")?.as_u64()? as u32 + 1)
}

fn last_line_of(symbol: &Value) -> Option<u32> {
    Some(symbol.pointer("/range/end/line")?.as_u64()? as u32 + 1)
}

fn name_of(symbol: &Value) -> &str {
    symbol
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

/// The run of module-level items the lowered anchor `range` covers.
///
/// A range that is one point is an `item` anchor on a name, and stands for that whole item. Anything
/// else is the run an `items` anchor lowers to: every item that *starts* inside the lines.
///
/// Refused rather than approximated: a range inside an `impl` (its members are reached through
/// their type, and half an `impl` is not an item), a range that cuts an item in half, and a module
/// declaration, which is `reparent_module`'s.
pub(super) fn run_covering(symbols: &Value, text: &str, range: Range) -> Result<Run> {
    let roots: Vec<&Value> = symbols.as_array().into_iter().flatten().collect();
    let (first, last) = if range.start == range.end {
        named_item_lines(&roots, text, range).unwrap_or((range.start.line, range.end.line))
    } else {
        (range.start.line, range.end.line)
    };

    let moved: Vec<&Value> = roots
        .iter()
        .copied()
        .filter(|symbol| first_line_of(symbol).is_some_and(|line| (first..=last).contains(&line)))
        .collect();
    if moved.is_empty() {
        return Err(refusal_for_a_range_inside_an_item(&roots, first));
    }
    refuse_what_cannot_move(&moved, last)?;

    let items = moved
        .iter()
        .filter(|symbol| is_identifier(name_of(symbol)))
        .filter_map(|symbol| {
            let position = symbol.pointer("/selectionRange/start")?.clone();
            Some(Item {
                name: name_of(symbol).to_string(),
                visibility: visibility_before(text, &position),
                position,
            })
        })
        .collect();
    Ok(Run {
        first_line: first,
        last_line: last,
        items,
    })
}

/// The lines of the item whose name the point `range` is at, trivia included.
fn named_item_lines(roots: &[&Value], text: &str, range: Range) -> Option<(u32, u32)> {
    let symbol = roots.iter().find(|symbol| {
        symbol
            .pointer("/selectionRange/start/line")
            .and_then(Value::as_u64)
            == Some(u64::from(range.start.line) - 1)
    })?;
    Some((
        attached_trivia_starts_at(text, first_line_of(symbol)?),
        last_line_of(symbol)?,
    ))
}

fn refusal_for_a_range_inside_an_item(roots: &[&Value], line: u32) -> crate::RestructureError {
    let holder = roots.iter().find(|symbol| {
        first_line_of(symbol)
            .zip(last_line_of(symbol))
            .is_some_and(|(first, last)| (first..=last).contains(&line))
    });
    match holder {
        Some(symbol) if symbol.get("kind").and_then(Value::as_u64) == Some(SYMBOL_KIND_IMPL) => {
            seam_refusal(format!(
                "the range sits inside `{}`, and a member of an `impl` cannot move alone: it is \
                 reached through its type, and half an `impl` is not an item. Move the `impl` \
                 block itself, with its type",
                name_of(symbol)
            ))
        }
        Some(symbol) => seam_refusal(format!(
            "the range sits inside `{}`, which is not a module-level item of its own: only a \
             whole module-level item can move",
            name_of(symbol)
        )),
        None => failure("the anchor covers no module-level item of the file"),
    }
}

fn refuse_what_cannot_move(moved: &[&Value], last: u32) -> Result<()> {
    for symbol in moved {
        if symbol.get("kind").and_then(Value::as_u64) == Some(SYMBOL_KIND_MODULE) {
            return Err(seam_refusal(format!(
                "`{}` is a module, and `move_item` moves the items inside one: a module changes \
                 its parent with `reparent_module`",
                name_of(symbol)
            )));
        }
        if last_line_of(symbol).is_some_and(|end| end > last) {
            return Err(seam_refusal(format!(
                "the range cuts `{}` in half: it ends at line {last} and the item goes on",
                name_of(symbol)
            )));
        }
    }
    Ok(())
}

/// The named root items the run leaves behind, in file order.
pub(super) fn left_behind(symbols: &Value, text: &str, run: &Run) -> Vec<Item> {
    root_items(symbols, text, Some(run))
}

/// The named root items of the outline, in file order: all of them, or with `outside`, the ones
/// that do not start inside that run.
pub(crate) fn root_items(symbols: &Value, text: &str, outside: Option<&Run>) -> Vec<Item> {
    symbols
        .as_array()
        .into_iter()
        .flatten()
        .filter(|symbol| {
            !outside.is_some_and(|run| {
                first_line_of(symbol)
                    .is_some_and(|line| (run.first_line..=run.last_line).contains(&line))
            })
        })
        .filter(|symbol| is_identifier(name_of(symbol)))
        .filter_map(|symbol| {
            let position = symbol.pointer("/selectionRange/start")?.clone();
            Some(Item {
                name: name_of(symbol).to_string(),
                visibility: visibility_before(text, &position),
                position,
            })
        })
        .collect()
}

/// The text of the declaration line up to the name at `position`, past any attributes written on
/// the same line.
fn declaration_prefix<'a>(text: &'a str, position: &Value) -> Option<(usize, &'a str)> {
    let point = LspPoint {
        line: position.get("line")?.as_u64()? as usize,
        character: position.get("character")?.as_u64()? as usize,
    };
    let start = line_start(text, point.line as u32 + 1);
    let name_at = (start + point.character).min(text.len());
    let mut from = start;
    loop {
        let rest = text[from..name_at].trim_start();
        let skipped = text[from..name_at].len() - rest.len();
        match rest.strip_prefix("#[").and_then(|inner| inner.find(']')) {
            Some(close) => from += skipped + "#[".len() + close + 1,
            None => return Some((from + skipped, &text[from + skipped..name_at])),
        }
    }
}

/// The visibility an item was written with, read from the text before its name.
pub(super) fn visibility_before(text: &str, position: &Value) -> String {
    declaration_prefix(text, position)
        .map(|(_, prefix)| visibility_in(prefix))
        .unwrap_or_default()
}

/// The edit that gives the declaration of the item at `position` the visibility `to`.
///
/// The keyword goes where the declaration's own text begins. A declaration whose name stands at the
/// start of its line has its keyword on a line above, which this does not guess at.
pub(super) fn visibility_edit(text: &str, position: &Value, name: &str, to: &str) -> Result<Edit> {
    let (at, prefix) = declaration_prefix(text, position).ok_or_else(|| {
        failure(format!(
            "the server's position for `{name}` could not be read"
        ))
    })?;
    let written = visibility_in(prefix);
    if prefix[written.len()..].trim().is_empty() {
        return Err(seam_refusal(format!(
            "the declaration of `{name}` has its keyword on a line above its name, so its \
             visibility cannot be changed in place: write the keyword and the name on one line"
        )));
    }
    let after = prefix[written.len()..].len() - prefix[written.len()..].trim_start().len();
    let replacement = if to.is_empty() {
        String::new()
    } else {
        format!("{to} ")
    };
    Ok(Edit::replace(at..at + written.len() + after, replacement))
}
