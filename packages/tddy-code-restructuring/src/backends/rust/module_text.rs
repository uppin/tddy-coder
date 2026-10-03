use super::server_defect;

use super::is_identifier_char;

use super::is_identifier;

use super::visibility_in;

use super::seam_refusal;

use crate::{backends::rust::import_text, Result};

use crate::edit::Range;

/// Refuse an extraction whose module name the parent already uses for something else.
///
/// The name is the one piece of text an extraction invents, and it is the only name a facade can
/// collide on: every other name in the module was already unique, and an extraction only moves names
/// out. A `mod report` written beside an existing `pub mod report;` is `E0428`, and nothing in the
/// assist's own output says so — the run reports success and the crate stops compiling.
///
/// Read from the text, so it costs no crate graph and answers before a server is started. A
/// declaration inside the range is not counted: those move into the new module and vacate the name.
pub(crate) fn refuse_module_name_taken(text: &str, module: &str, range: Range) -> Result<()> {
    let retained = outside_the_range(text, range);

    let taken = module_declaration(&retained, module).or_else(|| {
        import_text::imported_paths(&retained)
            .into_iter()
            .find(|path| path.rsplit("::").next() == Some(module))
            .map(|path| format!("use {path}"))
    });

    match taken {
        None => Ok(()),
        Some(binding) => Err(seam_refusal(format!(
            "`{module}` is already taken in this module by `{binding}`. A second declaration of the \
             name is `E0428` and the assist writes it without complaint, so the run would report \
             success against a crate that no longer compiles. Give the module a different name."
        ))),
    }
}

/// The text with the relocated range blanked out, line numbering preserved.
///
/// An extraction moves names *out*, so a declaration inside the seam vacates the parent and its name
/// is free for the new module to take. Blanking rather than deleting keeps every remaining line where
/// it was, which is what lets the same read serve checks that report a line.
fn outside_the_range(text: &str, range: Range) -> String {
    text.split('\n')
        .enumerate()
        .map(|(index, line)| {
            let number = index as u32 + 1;
            if number >= range.start.line && number <= range.end.line {
                ""
            } else {
                line
            }
        })
        .collect::<Vec<&str>>()
        .join("\n")
}

/// The `mod` declaration of `name` the text carries, if it carries one.
///
/// Lexical and deliberately narrow. A line that merely mentions the name — a comment, a doc comment,
/// an attribute, a call — declares nothing, and a name that is a prefix of another (`report` beside
/// `reporting`) is not it either.
fn module_declaration(text: &str, name: &str) -> Option<String> {
    text.split('\n').map(str::trim).find_map(|line| {
        if line.starts_with("//") || line.starts_with('#') {
            return None;
        }
        let rest = strip_visibility(line).strip_prefix("mod ")?;
        (rest.trim_end_matches([';', '{', ' ']).trim() == name).then(|| line.to_string())
    })
}

/// A declaration with its leading `pub`, `pub(crate)`, `pub(super)` … removed.
fn strip_visibility(declaration: &str) -> &str {
    let visibility = visibility_in(declaration);
    if visibility.is_empty() {
        return declaration;
    }
    declaration[visibility.len()..].trim_start()
}

/// Every path an attribute names by string, with the line it was written on.
///
/// `#[serde(default = "default_extend")]` reaches an item the way a call does, but rust-analyzer
/// answers `textDocument/references` on that item without it: serde builds the call out of the
/// string's *contents*, so the identifier it generates has no span in the source. A seam is
/// therefore free to separate the two, and only the compiler ever says so.
///
/// A `doc` attribute is excluded: its string is prose, and prose that happens to read as a path is
/// not a reference to anything.
pub(crate) fn attribute_path_names(text: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();

    for (index, line) in text.split('\n').enumerate() {
        let trimmed = line.trim();
        let Some(body) = trimmed
            .strip_prefix("#![")
            .or_else(|| trimmed.strip_prefix("#["))
        else {
            continue;
        };
        if body.starts_with("doc") {
            continue;
        }
        for literal in body.split('"').skip(1).step_by(2) {
            if is_path(literal) {
                found.push((index + 1, literal.to_string()));
            }
        }
    }

    found
}

/// Whether a string literal reads as a Rust path, and so names something rather than saying something.
fn is_path(literal: &str) -> bool {
    !literal.is_empty() && literal.split("::").all(is_identifier)
}

/// Refuse a seam that would separate an attribute's string path from the item it names.
///
/// Only an unqualified name is weighed. A qualified one — `crate::defaults::extend` — resolves the
/// same from either side of the seam, so moving the attribute or the item changes nothing. A bare
/// identifier resolves in the scope the attribute sits in, so putting the two in different modules
/// breaks it, silently, in code no diff shows.
///
/// A name this file does not declare is left alone: it lives in another module, and this seam is not
/// what separates them.
pub(crate) fn refuse_split_attribute_paths(text: &str, range: Range) -> Result<()> {
    let split: Vec<String> = attribute_path_names(text)
        .into_iter()
        .filter(|(_, path)| !path.contains("::"))
        .filter_map(|(line, path)| {
            let declared = declaration_line(text, &path)?;
            let inside = |line: usize| {
                line as u32 >= range.start.line && line as u32 <= range.end.line
            };
            (inside(line) != inside(declared)).then(|| {
                format!("`{path}`, named by the attribute on line {line} and declared on line {declared}")
            })
        })
        .collect();

    if split.is_empty() {
        return Ok(());
    }

    Err(seam_refusal(format!(
        "the seam would separate an attribute from the item its string names: {}. The name resolves \
         in the scope the attribute sits in, and no reference query reports the attribute — a macro \
         builds the call out of the string's contents, so the identifier it generates has no span to \
         find. Cut the seam so the two stay together.",
        split.join("; ")
    )))
}

/// The one-based line on which the text declares `name`, if it declares it.
fn declaration_line(text: &str, name: &str) -> Option<usize> {
    const KEYWORDS: [&str; 8] = [
        "fn ", "struct ", "enum ", "trait ", "mod ", "const ", "static ", "type ",
    ];

    text.split('\n').enumerate().find_map(|(index, line)| {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with('#') {
            return None;
        }
        let declaration = strip_visibility(trimmed);
        let rest = KEYWORDS
            .iter()
            .find_map(|keyword| declaration.strip_prefix(keyword))?;
        let declared = rest
            .split(|c: char| !is_identifier_char(c))
            .next()
            .unwrap_or_default();
        (declared == name).then_some(index + 1)
    })
}

/// Insert the facade immediately after the module the assist wrote.
///
/// The module's end is found by indentation rather than by counting braces. A brace inside a string
/// literal is ordinary Rust — `format!("{:.1}", …)` carries a pair — and counting would be at their
/// mercy, while the assist always writes the closing brace alone on a line at the `mod` keyword's own
/// indent. Searching for a marker after the module is no better: an adjacent seam's markers move, and
/// one item's doc comment is routinely a prefix of another's.
pub(crate) fn with_facade(text: &str, module: &str, lines: &[String]) -> Result<String> {
    if lines.is_empty() {
        return Ok(text.to_string());
    }

    let mut source: Vec<String> = text.split('\n').map(str::to_string).collect();
    let block = module_bounds(&source, module)?;

    for (offset, line) in lines.iter().enumerate() {
        source.insert(block.closed + 1 + offset, format!("{}{line}", block.indent));
    }

    Ok(source.join("\n"))
}

/// The lines an inline module opens and closes on, and the indent it sits at.
pub(crate) struct ModuleBlock {
    pub(crate) opened: usize,
    pub(crate) closed: usize,
    pub(crate) indent: String,
}

/// Locate the module the assist wrote.
///
/// By indentation, not by counting braces. A brace inside a string literal is ordinary Rust —
/// `format!("{:.1}", …)` carries a pair — and counting would be at their mercy, while the assist
/// always writes the closing brace alone on a line at the `mod` keyword's own indent. Searching for a
/// marker after the module is no better: an adjacent seam's markers move, and one item's doc comment
/// is routinely a prefix of another's.
pub(crate) fn module_bounds(source: &[String], module: &str) -> Result<ModuleBlock> {
    let header = format!("mod {module}");

    let opened = source
        .iter()
        .position(|line| line.trim_start().starts_with(&header) && line.trim_end().ends_with('{'))
        .ok_or_else(|| {
            server_defect(format!(
                "rust-analyzer did not write a `{header}` block where one was expected"
            ))
        })?;

    let indent =
        source[opened][..source[opened].len() - source[opened].trim_start().len()].to_string();
    let closing = format!("{indent}}}");
    let closed = source
        .iter()
        .skip(opened + 1)
        .position(|line| line.trim_end() == closing)
        .map(|offset| opened + 1 + offset)
        .ok_or_else(|| server_defect(format!("`{header}` is never closed at its own indent")))?;

    Ok(ModuleBlock {
        opened,
        closed,
        indent,
    })
}

/// The path an `as` alias binds, for an alias the text declares *outside* the extracted module.
///
/// Read from the parent's own `use` tree rather than from the server, because the server reports
/// what a path resolves to and not what a file chose to call it. Only declarations outside the
/// module are considered: one inside it would already have bound the name.
pub(crate) fn alias_target(text: &str, module: &str, alias: &str) -> Option<String> {
    let source: Vec<String> = text.split('\n').map(str::to_string).collect();
    let block = module_bounds(&source, module).ok()?;
    let outside: String = source
        .iter()
        .enumerate()
        .filter(|(line, _)| *line < block.opened || *line > block.closed)
        .map(|(_, text)| text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    aliased_bindings(&outside)
        .into_iter()
        .find(|(bound, _)| bound == alias)
        .map(|(_, path)| path)
}

/// The path a *non-aliased* binding the text declares outside the module gives to `name`.
///
/// `use crate::tool_engine;` binds `tool_engine`; `use a::b::Thing;` binds `Thing`. Used only where
/// the server offered nothing, so it never overrides an opinion rust-analyzer actually has.
pub(crate) fn parent_binding(text: &str, module: &str, name: &str) -> Option<String> {
    let source: Vec<String> = text.split('\n').map(str::to_string).collect();
    let block = module_bounds(&source, module).ok()?;
    let outside: String = source
        .iter()
        .enumerate()
        .filter(|(line, _)| *line < block.opened || *line > block.closed)
        .map(|(_, text)| text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    import_text::imported_paths(&outside)
        .into_iter()
        .find(|path| path.rsplit("::").next() == Some(name))
}

/// Every `(alias, path)` pair the text's `use` declarations bind through `as`.
pub(crate) fn aliased_bindings(text: &str) -> Vec<(String, String)> {
    let mut bindings = Vec::new();

    for statement in text.split(';') {
        if let Some(tree) = import_text::use_tree(statement) {
            collect_aliases(tree, "", &mut bindings);
        }
    }

    bindings
}

/// Walk a `use` tree, recording only the members that carry an `as` clause.
fn collect_aliases(tree: &str, prefix: &str, bindings: &mut Vec<(String, String)>) {
    let tree = tree.trim();

    let Some(open) = tree.find('{') else {
        if let Some((path, alias)) = tree.split_once(" as ") {
            let alias = alias.trim();
            let path = path.trim();
            // `as _` binds no name, so nothing can be unresolved under it.
            if alias != "_" && !path.is_empty() {
                bindings.push((alias.to_string(), format!("{prefix}{path}")));
            }
        }
        return;
    };

    let head = format!("{prefix}{}", &tree[..open]);
    let close = tree.rfind('}').unwrap_or(tree.len());

    for member in import_text::group_members(&tree[open + 1..close]) {
        collect_aliases(member, &head, bindings);
    }
}

/// The text with `line` inserted as the extracted module's first declaration.
pub(crate) fn with_module_import(text: &str, module: &str, line: &str) -> Result<String> {
    let mut source: Vec<String> = text.split('\n').map(str::to_string).collect();
    let block = module_bounds(&source, module)?;
    source.insert(block.opened + 1, format!("{}    {line}", block.indent));
    Ok(source.join("\n"))
}
