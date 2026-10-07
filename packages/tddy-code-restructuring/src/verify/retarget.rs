//! What `verify` is told about an `impl` retarget the author made, so it can account for it.
//!
//! A declaration, never an inference: from the two trees alone a retarget cannot be told from a hand
//! edit, and `verify` exists to show hand edits. Without one `verify` behaves exactly as before.
//!
//! Two rules read the declaration, and they run between visibility pairing and re-point pairing:
//!
//! - **R1, rename pairing.** A lost statement and a gained one pair 1:1 when the gained one equals
//!   the lost one with every whole-identifier `from` (outside strings, comments and lifetimes)
//!   replaced by `to`, compared on the key the re-point pass uses (visibility and lowercase module
//!   qualifiers ignored). This pairs `OLD::build(…)` with `NEW::build(…)` and a whole generic header.
//! - **R2, header accounting.** A split repeats the `impl` header — `impl<T> Host<T>`, a `where`
//!   clause and its predicate lines are statements `verify` reads — once for each added block. For
//!   each declared retarget the original header is read out of the ref's own statements and the
//!   repeated (old) and rewritten (new) headers it produces are excused from the gained statements.
//!
//! Both count into `Excused::repointed`. A rename that was not declared, a changed argument, a lost
//! statement or a lost comment stay reported.

use std::collections::BTreeMap;
use std::str::FromStr;

use super::statements;

/// The retargets an author declares to `verify`, one per `retarget_impl` they ran.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Declared {
    pub retargets: Vec<Retarget>,
    /// The call re-points declared (`--repoint`, one per `repoint_call`): rule R-call, in
    /// [`super::repoint`].
    pub repoints: Vec<super::Repoint>,
}

/// One declared retarget: the members of `impl from` became members of `impl to`. Bare type
/// identifiers, generics stripped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Retarget {
    pub from: String,
    pub to: String,
}

impl FromStr for Retarget {
    type Err = String;

    /// Read `OLD=NEW`, the form `--retarget` and `VerifyRequest.retargets` carry.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let identifier = |name: &str| {
            let mut characters = name.chars();
            characters
                .next()
                .is_some_and(|first| first.is_alphabetic() || first == '_')
                && characters.all(|rest| rest.is_alphanumeric() || rest == '_')
        };
        match text.split_once('=') {
            Some((from, to)) if identifier(from) && identifier(to) && from != to => Ok(Retarget {
                from: from.to_string(),
                to: to.to_string(),
            }),
            _ => Err(format!(
                "`{text}` is not a retarget: write `OLD=NEW`, two different bare type names"
            )),
        }
    }
}

impl Declared {
    /// The declaration the texts of both kinds name, or the first that is not one: retargets as
    /// `OLD=NEW` type names, repoints as `OLD=NEW` callee texts.
    pub fn from_declarations<'a>(
        retargets: impl IntoIterator<Item = &'a String>,
        repoints: impl IntoIterator<Item = &'a String>,
    ) -> Result<Declared, String> {
        Ok(Declared {
            repoints: repoints
                .into_iter()
                .map(|text| text.parse())
                .collect::<Result<_, _>>()?,
            ..Declared::from_texts(retargets)?
        })
    }

    /// The declaration the texts name, or the first that is not one.
    pub fn from_texts<'a>(texts: impl IntoIterator<Item = &'a String>) -> Result<Declared, String> {
        Ok(Declared {
            retargets: texts
                .into_iter()
                .map(|text| text.parse())
                .collect::<Result<_, _>>()?,
            repoints: Vec::new(),
        })
    }
}

/// What R1 and R2 leave of two lists, and how many pairs they excused.
pub(crate) struct Passed {
    pub(crate) missing: Vec<String>,
    pub(crate) added: Vec<String>,
    pub(crate) pairs: usize,
}

/// The declaration's two rules over the leftovers of visibility pairing: rename pairing (R1), then
/// header accounting (R2), each counted into the pairs they excuse.
///
/// `before` is the ref's own statements in source order, which is where R2 reads the header a split
/// repeated. Nothing here is done without a declaration: an empty `Declared` returns the lists as
/// they came.
pub(crate) fn account(
    missing: Vec<String>,
    added: Vec<String>,
    declared: &Declared,
    before: &[String],
) -> Passed {
    let renamed = rename_pairs(missing, added, declared);
    let headed = repeated_headers(renamed.missing, renamed.added, declared, before);
    Passed {
        missing: headed.missing,
        added: headed.added,
        pairs: renamed.pairs + headed.pairs,
    }
}

/// R1: pair each lost statement with a gained one equal to it once every whole-identifier `from`
/// becomes `to`, for one of the declared retargets, one to one.
fn rename_pairs(missing: Vec<String>, added: Vec<String>, declared: &Declared) -> Passed {
    let mut by_key: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for statement in added {
        by_key.entry(key(&statement)).or_default().push(statement);
    }
    let mut unpaired = Vec::new();
    let mut pairs = 0;
    for statement in missing {
        let renamed = declared.retargets.iter().find_map(|retarget| {
            let candidate = replace_whole_identifier(&statement, &retarget.from, &retarget.to);
            let key = key(&candidate);
            by_key
                .get_mut(&key)
                .filter(|bucket| !bucket.is_empty())
                .map(|bucket| {
                    bucket.pop();
                    key
                })
        });
        match renamed {
            Some(_) => pairs += 1,
            None => unpaired.push(statement),
        }
    }
    let mut added: Vec<String> = by_key.into_values().flatten().collect();
    added.sort();
    Passed {
        missing: unpaired,
        added,
        pairs,
    }
}

/// R2: excuse the header a split repeated. For each declared retarget the ref's own header — an
/// inherent `impl` line, its `where` clause and its predicate lines — is read out of `before`, and
/// both it and the same header with the self type renamed are removed from the gained statements.
fn repeated_headers(
    missing: Vec<String>,
    added: Vec<String>,
    declared: &Declared,
    before: &[String],
) -> Passed {
    let mut wanted: BTreeMap<String, usize> = BTreeMap::new();
    for retarget in &declared.retargets {
        let Some(header) = header_groups(before)
            .into_iter()
            .find(|group| impl_self_type(&group[0]).as_deref() == Some(&retarget.from))
        else {
            continue;
        };
        for line in &header {
            *wanted.entry(line.clone()).or_default() += 1;
        }
        *wanted
            .entry(replace_whole_identifier(
                &header[0],
                &retarget.from,
                &retarget.to,
            ))
            .or_default() += 1;
        for line in &header[1..] {
            *wanted.entry(line.clone()).or_default() += 1;
        }
    }

    let mut pairs = 0;
    let mut kept = Vec::new();
    for statement in added {
        match wanted.get_mut(&statement) {
            Some(left) if *left > 0 => {
                *left -= 1;
                pairs += 1;
            }
            _ => kept.push(statement),
        }
    }
    Passed {
        missing,
        added: kept,
        pairs,
    }
}

/// The inherent `impl` header groups among `statements`, in source order: a line that opens a
/// header, then its `where` and each predicate line (one ending with a comma).
fn header_groups(statements: &[String]) -> Vec<Vec<String>> {
    let mut groups = Vec::new();
    let mut at = 0;
    while at < statements.len() {
        if impl_self_type(&statements[at]).is_none() {
            at += 1;
            continue;
        }
        let mut group = vec![statements[at].clone()];
        let mut cursor = at + 1;
        if statements.get(cursor).map(String::as_str) == Some("where") {
            group.push(statements[cursor].clone());
            cursor += 1;
            while statements
                .get(cursor)
                .is_some_and(|line| line.trim_end().ends_with(','))
            {
                group.push(statements[cursor].clone());
                cursor += 1;
            }
        }
        groups.push(group);
        at = cursor;
    }
    groups
}

/// The bare self type of an inherent `impl` header line, or `None` when the line opens no inherent
/// header — a trait impl (which `retarget_impl` refuses) or anything that is not an `impl`.
fn impl_self_type(line: &str) -> Option<String> {
    let body = statements::strip_visibility(line);
    let rest = body.strip_prefix("impl")?;
    let rest = rest.trim_start();
    let rest = if rest.starts_with('<') {
        after_generics(rest)?
    } else {
        rest
    };
    let rest = rest.trim_start();
    if rest.contains(" for ") {
        return None;
    }
    let written = rest
        .split(|character: char| character == '{' || character.is_whitespace())
        .next()?;
    let name = written.split('<').next()?.rsplit("::").next()?.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// `text` without its leading `<…>` list, when it opens one.
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

/// The key R1 compares on: the statement without a leading visibility or lowercase module
/// qualifiers, with runs of whitespace collapsed — the key the re-point pass uses.
fn key(statement: &str) -> String {
    statements::strip_qualifiers(statements::strip_visibility(statement))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// `text` with every whole identifier `from` replaced by `to`, leaving string literals, comments and
/// lifetimes as written.
fn replace_whole_identifier(text: &str, from: &str, to: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < chars.len() {
        let character = chars[at];
        if character == '"' {
            let end = string_end(&chars, at);
            out.extend(&chars[at..end]);
            at = end;
        } else if character == '/' && chars.get(at + 1) == Some(&'/') {
            out.extend(&chars[at..]);
            break;
        } else if character == '\'' && is_word_byte(chars.get(at + 1)) {
            let end = word_end(&chars, at + 1);
            out.extend(&chars[at..end]);
            at = end;
        } else if is_word_byte(Some(&character)) {
            let end = word_end(&chars, at);
            let word: String = chars[at..end].iter().collect();
            out.push_str(if word == from { to } else { &word });
            at = end;
        } else {
            out.push(character);
            at += 1;
        }
    }
    out
}

pub(crate) fn is_word_byte(character: Option<&char>) -> bool {
    character.is_some_and(|character| character.is_alphanumeric() || *character == '_')
}

pub(crate) fn word_end(chars: &[char], at: usize) -> usize {
    let mut end = at;
    while is_word_byte(chars.get(end)) {
        end += 1;
    }
    end
}

/// The offset just past the string literal opening at `at`.
pub(crate) fn string_end(chars: &[char], at: usize) -> usize {
    let mut end = at + 1;
    while end < chars.len() {
        match chars[end] {
            '\\' => end += 2,
            '"' => return end + 1,
            _ => end += 1,
        }
    }
    chars.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_string()).collect()
    }

    #[test]
    fn replaces_a_whole_identifier_and_leaves_a_longer_one_alone() {
        assert_eq!(
            replace_whole_identifier("Host::new(Hostage, \"Host\")", "Host", "Roster"),
            "Roster::new(Hostage, \"Host\")"
        );
    }

    #[test]
    fn leaves_a_lifetime_and_a_comment_alone() {
        assert_eq!(
            replace_whole_identifier("fn f<'Host>(x: &'Host Host) {} // Host", "Host", "Roster"),
            "fn f<'Host>(x: &'Host Roster) {} // Host"
        );
    }

    #[test]
    fn reads_the_self_type_of_an_inherent_generic_header() {
        assert_eq!(impl_self_type("impl<T> Host<T>"), Some("Host".to_string()));
        assert_eq!(impl_self_type("impl Host"), Some("Host".to_string()));
        assert_eq!(impl_self_type("impl Display for Host"), None);
        assert_eq!(impl_self_type("fn get(&self) -> T {"), None);
    }

    #[test]
    fn groups_a_header_with_its_where_clause() {
        let statements = texts(&[
            "impl<T> Host<T>",
            "where",
            "T: Copy,",
            "fn get(&self) -> T {",
        ]);

        assert_eq!(
            header_groups(&statements),
            vec![texts(&["impl<T> Host<T>", "where", "T: Copy,"])]
        );
    }
}
