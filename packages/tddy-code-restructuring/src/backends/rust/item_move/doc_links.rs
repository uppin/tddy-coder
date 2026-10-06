//! Intra-doc links to a moved item, made to follow it.
//!
//! rust-analyzer reports no position for a doc-link target — `textDocument/references` returns none
//! on the `///` / `//!` line — so the links are read from the text instead: a `` [`crate::old::f`] ``
//! in a doc comment is a link to the item that moved, and it is respelled to `` [`crate::new::f`] ``.
//!
//! Prose is not a link and is left alone; so is anything inside a fenced code block, which is an
//! example, not a link. A target written `self::` / `super::` is relative to the file's own module
//! and is left as written: this pass reads the fully-qualified forms only (`crate::…` and a crate
//! name), which are the ones a path to a moved item is written as.

use std::collections::BTreeMap;
use std::ops::Range;

use super::destination::Package;
use super::text::Edit;
use crate::crate_move::module_files::files_of;
use crate::registry::Workspace;
use crate::Result;

/// The edits that re-point every intra-doc link in `text` from an old path to the path its item
/// moved to. Each pair is `(old, new)`, both written from a crate root (`crate::a::b` or
/// `app::a::b`), and a link target that is the old path or reaches through it is respelled.
pub(in crate::backends::rust) fn edits(text: &str, moved: &[(String, String)]) -> Vec<Edit> {
    let mut edits = Vec::new();
    let mut fenced = false;
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        if let Some(body) = doc_body(line) {
            let start = offset + (line.len() - body.len());
            if body.trim_start().starts_with("```") {
                fenced = !fenced;
            } else if !fenced {
                edits.extend(links_in(body, start, moved));
            }
        }
        offset += line.len();
    }
    edits
}

/// The text after a `///` (outer) or `//!` (inner) marker, when `line` is a doc line. `////` is an
/// ordinary comment, not documentation.
fn doc_body(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let is_doc =
        trimmed.starts_with("//!") || (trimmed.starts_with("///") && !trimmed.starts_with("////"));
    is_doc.then(|| &trimmed[3..])
}

/// Every link in one doc line's `body`, whose byte `base` in the file is `base`, re-pointed.
fn links_in(body: &str, base: usize, moved: &[(String, String)]) -> Vec<Edit> {
    let mut edits = Vec::new();
    let mut at = 0usize;
    while let Some(open) = body[at..].find('[') {
        let open = at + open;
        let Some(close) = body[open + 1..].find(']').map(|index| open + 1 + index) else {
            break;
        };
        let (target, span) = if body[close + 1..].starts_with('(') {
            let Some(end) = body[close + 1..].find(')').map(|index| close + 1 + index) else {
                at = close + 1;
                continue;
            };
            (&body[close + 2..end], (close + 2)..end)
        } else {
            let inner = &body[open + 1..close];
            match inner
                .strip_prefix('`')
                .and_then(|inner| inner.strip_suffix('`'))
            {
                Some(stripped) => (stripped, (open + 2)..(close - 1)),
                None => (inner, (open + 1)..close),
            }
        };
        if let Some(replacement) = respell(target, moved) {
            edits.push(Edit::replace(
                base + span.start..base + span.end,
                replacement,
            ));
        }
        at = close + 1;
    }
    edits
}

/// `target` respelled to the new home, when it is one of the moved paths or reaches through one.
fn respell(target: &str, moved: &[(String, String)]) -> Option<String> {
    moved.iter().find_map(|(old, new)| {
        let rest = target.strip_prefix(old.as_str())?;
        (rest.is_empty() || rest.starts_with("::")).then(|| format!("{new}{rest}"))
    })
}

/// The edits for every file of `package`'s crate, read through `texts` (and added to it), except
/// those inside `avoid` — the span of a file whose bytes are being replaced wholesale, where a link
/// would be carried by the code that moves rather than left behind.
pub(in crate::backends::rust) fn across_the_crate(
    workspace: &Workspace<'_>,
    package: &Package,
    moved: &[(String, String)],
    texts: &mut BTreeMap<String, String>,
    avoid: Option<(&str, Range<usize>)>,
) -> Result<Vec<(String, Vec<Edit>)>> {
    let Some(root_file) = crate_root_file(workspace, package) else {
        return Ok(Vec::new());
    };
    let mut found: Vec<(String, Vec<Edit>)> = Vec::new();
    for file in files_of(workspace, &root_file)? {
        if !texts.contains_key(&file) {
            texts.insert(file.clone(), workspace.read(&file)?);
        }
        let mut here = edits(&texts[&file], moved);
        if let Some((skipped, span)) = &avoid {
            if file == *skipped {
                here.retain(|edit| edit.end <= span.start || edit.start >= span.end);
            }
        }
        if !here.is_empty() {
            found.push((file, here));
        }
    }
    Ok(found)
}

/// The crate root file of `package`: `src/lib.rs`, else `src/main.rs`.
fn crate_root_file(workspace: &Workspace<'_>, package: &Package) -> Option<String> {
    let source = package.dir.join("src");
    ["lib.rs", "main.rs"].iter().find_map(|name| {
        let candidate = source.join(name).to_string_lossy().to_string();
        workspace.read(&candidate).ok().map(|_| candidate)
    })
}

#[cfg(test)]
mod tests {
    use super::super::text::applied;
    use super::*;

    fn the_pair() -> Vec<(String, String)> {
        vec![(
            "crate::pairing::peer_has_no_such_session".to_string(),
            "crate::answers::peer_has_no_such_session".to_string(),
        )]
    }

    #[test]
    fn a_backticked_shortcut_link_follows_the_item() {
        let text = "/// See [`crate::pairing::peer_has_no_such_session`].\n";

        assert_eq!(
            applied(text, &edits(text, &the_pair())).unwrap(),
            "/// See [`crate::answers::peer_has_no_such_session`].\n"
        );
    }

    #[test]
    fn a_module_doc_link_and_an_inline_link_follow_the_item() {
        let text = "//! [`crate::pairing::peer_has_no_such_session`] and \
                    [the predicate](crate::pairing::peer_has_no_such_session).\n";

        let rewritten = applied(text, &edits(text, &the_pair())).unwrap();

        assert_eq!(
            rewritten,
            "//! [`crate::answers::peer_has_no_such_session`] and \
             [the predicate](crate::answers::peer_has_no_such_session).\n"
        );
    }

    #[test]
    fn prose_and_a_fenced_example_are_left_alone() {
        let text = concat!(
            "/// This lived at crate::pairing::peer_has_no_such_session until it moved.\n",
            "///\n",
            "/// ```text\n",
            "/// crate::pairing::peer_has_no_such_session(404)\n",
            "/// ```\n",
        );

        assert!(edits(text, &the_pair()).is_empty());
    }

    #[test]
    fn a_relative_link_is_not_this_passs() {
        let text = "/// See [`self::peer_has_no_such_session`] and [`peer_has_no_such_session`].\n";

        assert!(edits(text, &the_pair()).is_empty());
    }
}
