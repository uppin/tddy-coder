use super::super::early_return::masked_to_code;
use super::super::signature::{arrow_before, offset_at, parameter_list, skip_whitespace};
use super::super::{failure, seam_refusal};
use super::{
    binding, edits_of, entries, inserting, reordered, replacing, required, separator_colon,
    with_an_entry_at, Replacement, Span,
};
use crate::edit::{Position, TextEdit};
use crate::plan::rust_syntax::permutation;
use crate::plan::{OrderKey, RefactorKind, RefactorOp};
use crate::Result;

/// The edits that rewrite the declaration of the function whose own name starts at `name_at` as
/// `op` asks.
///
/// Only `change_param_type`, `add_param`, `reorder_params` and `change_return_type` with a `type`
/// are rewrites of this kind; `change_return_type` with a `variant` is an assist's.
pub(in super::super) fn rewrite_declaration(
    text: &str,
    op: &RefactorOp,
    name_at: Position,
) -> Result<Vec<TextEdit>> {
    let code = masked_to_code(text);
    let (open, close) = offset_at(text, name_at)
        .and_then(|name| parameter_list(&code, name))
        .ok_or_else(|| seam_refusal("the anchor does not name a function with a parameter list"))?;
    let parameters = parameters(&code, open, close);

    let replacements = match op.op {
        RefactorKind::ChangeParamType => change_param_type(&code, &parameters, op),
        RefactorKind::AddParam => add_param(&code, (open, &parameters), op),
        RefactorKind::ReorderParams => reorder_params(text, &code, &parameters, op),
        RefactorKind::ChangeReturnType => change_return_type(&code, close, op),
        other => Err(failure(format!("{other:?} does not rewrite a declaration"))),
    }?;
    Ok(edits_of(text, replacements))
}

/// The type the function whose own name starts at `name_at` returns, as written, or `None` when it
/// declares none.
pub(in super::super) fn returned_type(text: &str, name_at: Position) -> Option<String> {
    let code = masked_to_code(text);
    let (_, close) = parameter_list(&code, offset_at(text, name_at)?)?;
    let span = return_type_span(&code, close)?;
    Some(span.of(text).to_string())
}

fn change_param_type(code: &str, parameters: &[Span], op: &RefactorOp) -> Result<Vec<Replacement>> {
    let name = required(op.name.as_deref(), "name")?;
    let new_type = required(op.type_.as_deref(), "type")?.trim();
    let parameter = parameters
        .iter()
        .find(|parameter| binding(code, **parameter) == Some(name))
        .ok_or_else(|| {
            seam_refusal(format!(
                "`{name}` is not a parameter of the function the anchor names"
            ))
        })?;
    let colon = separator_colon(code, *parameter)
        .ok_or_else(|| seam_refusal(format!("`{name}` declares no type to change")))?;
    let from = skip_whitespace(code.as_bytes(), colon + 1);
    Ok(replacing(
        Span {
            from,
            to: parameter.to,
        },
        new_type,
    ))
}

fn add_param(
    code: &str,
    (open, parameters): (usize, &[Span]),
    op: &RefactorOp,
) -> Result<Vec<Replacement>> {
    let name = required(op.name.as_deref(), "name")?;
    let new_type = required(op.type_.as_deref(), "type")?.trim();
    let position = required(op.variant.as_deref(), "variant")?;

    if parameters
        .iter()
        .any(|parameter| binding(code, *parameter) == Some(name))
    {
        return Err(seam_refusal(format!(
            "`{name}` is already a parameter of the function the anchor names"
        )));
    }

    // A receiver stays first: `first` means first after it.
    let receivers = parameters
        .iter()
        .take_while(|parameter| binding(code, **parameter).is_none())
        .count();
    let index = match position {
        "first" => receivers,
        "last" => parameters.len(),
        after => {
            let after = after.strip_prefix("after:").unwrap_or(after);
            parameters
                .iter()
                .position(|parameter| binding(code, *parameter) == Some(after))
                .ok_or_else(|| {
                    seam_refusal(format!(
                        "`{after}` is not a parameter of the function the anchor names"
                    ))
                })?
                + 1
        }
    };
    Ok(with_an_entry_at(
        open,
        parameters,
        index,
        &format!("{name}: {new_type}"),
    ))
}

fn reorder_params(
    text: &str,
    code: &str,
    parameters: &[Span],
    op: &RefactorOp,
) -> Result<Vec<Replacement>> {
    let named: Vec<(Span, String)> = parameters
        .iter()
        .filter_map(|parameter| Some((*parameter, binding(code, *parameter)?.to_string())))
        .collect();
    let current: Vec<OrderKey> = named
        .iter()
        .map(|(_, name)| OrderKey::Name(name.clone()))
        .collect();
    let moved = permutation(&current, &op.order)?;
    let spans: Vec<Span> = named.iter().map(|(span, _)| *span).collect();
    Ok(reordered(text, &spans, &moved))
}

fn change_return_type(code: &str, close: usize, op: &RefactorOp) -> Result<Vec<Replacement>> {
    let new_type = required(op.type_.as_deref(), "type")?.trim();
    Ok(match return_type_span(code, close) {
        Some(declared) => replacing(declared, new_type),
        None => inserting(close + 1, &format!(" -> {new_type}")),
    })
}

/// The `T` of `-> T` after the parameter list closing at `close`, up to the body, a `where` clause
/// or the `;` of a declaration without a body.
fn return_type_span(code: &str, close: usize) -> Option<Span> {
    let bytes = code.as_bytes();
    let arrow = skip_whitespace(bytes, close + 1);
    if !code[arrow..].starts_with("->") {
        return None;
    }
    let from = skip_whitespace(bytes, arrow + "->".len());

    let mut depth = 0usize;
    let mut to = from;
    while to < bytes.len() {
        match bytes[to] {
            b'(' | b'[' | b'<' => depth += 1,
            b')' | b']' => depth = depth.saturating_sub(1),
            b'>' if !arrow_before(bytes, to) => depth = depth.saturating_sub(1),
            b'{' | b';' if depth == 0 => break,
            b'w' if depth == 0 && begins_the_word(code, to, "where") => break,
            _ => {}
        }
        to += 1;
    }
    let to = from + code[from..to].trim_end().len();
    (to > from).then_some(Span { from, to })
}

fn begins_the_word(code: &str, at: usize, word: &str) -> bool {
    let bytes = code.as_bytes();
    let is_word_byte = |byte: &u8| byte.is_ascii_alphanumeric() || *byte == b'_';
    code[at..].starts_with(word)
        && !at
            .checked_sub(1)
            .is_some_and(|before| is_word_byte(&bytes[before]))
        && !bytes.get(at + word.len()).is_some_and(is_word_byte)
}

/// The parameters between the parentheses at `open` and `close`, each trimmed of its whitespace.
fn parameters(code: &str, open: usize, close: usize) -> Vec<Span> {
    entries(code, open, close, true)
}
