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
//! - **R3, delegator accounting.** `retarget_impl`'s `leave_delegator` leaves a forwarding method in
//!   each moved method's slot: a gained `fn` signature line the ref already has, and a gained body
//!   `<receiver>.<m>(<arguments>)` (or `….await`, or `NEW::<m>(…)`) whose method and argument names
//!   are exactly those of an excused signature. Each is excused once per signature the ref holds.
//!
//! All count into `Excused::repointed`. A rename that was not declared, a changed argument, a lost
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
    /// The `self` rebinds declared (`--rebind`, one per `read_fields_through`): rule R-rebind, in
    /// [`super::rebind`].
    pub rebinds: Vec<super::Rebind>,
    /// The detaches declared (`--detach TYPE=NAME`, one per `detach_method` type): rule R-detach,
    /// in [`super::detach`].
    pub detaches: Vec<super::Detach>,
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
            rebinds: Vec::new(),
            detaches: Vec::new(),
        })
    }

    /// This declaration with the rebinds the texts name added, or the first that is not one.
    pub fn with_rebinds<'a>(
        self,
        rebinds: impl IntoIterator<Item = &'a String>,
    ) -> Result<Declared, String> {
        Ok(Declared {
            rebinds: rebinds
                .into_iter()
                .map(|text| text.parse())
                .collect::<Result<_, _>>()?,
            ..self
        })
    }

    /// This declaration with the detaches the texts name added, or the first that is not one.
    pub fn with_detaches<'a>(
        self,
        detaches: impl IntoIterator<Item = &'a String>,
    ) -> Result<Declared, String> {
        let detaches: Vec<super::Detach> = detaches
            .into_iter()
            .map(|text| text.parse())
            .collect::<Result<_, _>>()?;
        // R7: a detach implies the rebind of the parameter its receiver became, once each.
        let mut rebinds = self.rebinds;
        for detach in &detaches {
            if !rebinds.iter().any(|rebind| rebind.name == detach.name) {
                rebinds.push(super::Rebind {
                    name: detach.name.clone(),
                });
            }
        }
        Ok(Declared {
            detaches,
            rebinds,
            retargets: self.retargets,
            repoints: self.repoints,
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
    let forwarded = delegators(headed.missing, headed.added, declared, before);
    Passed {
        missing: forwarded.missing,
        added: forwarded.added,
        pairs: renamed.pairs + headed.pairs + forwarded.pairs,
    }
}

/// A moved method as its forwarding method names it: the method and its parameters but `self`.
#[derive(Debug, PartialEq, Eq)]
struct Forwarded {
    method: String,
    arguments: Vec<String>,
}

/// R3: excuse the delegators a declared retarget leaves. A gained signature line equal to one the
/// ref has is excused once per such ref statement, and a gained forwarding call naming exactly an
/// excused signature's method and parameters is excused once per signature.
fn delegators(
    missing: Vec<String>,
    added: Vec<String>,
    declared: &Declared,
    before: &[String],
) -> Passed {
    if declared.retargets.is_empty() {
        return Passed {
            missing,
            added,
            pairs: 0,
        };
    }
    let mut ref_signatures: BTreeMap<&str, usize> = BTreeMap::new();
    for statement in before
        .iter()
        .filter(|statement| signature_of(statement).is_some())
    {
        *ref_signatures
            .entry(statements::strip_visibility(statement))
            .or_default() += 1;
    }

    let mut pairs = 0;
    let mut forwardable = Vec::new();
    let mut rest = Vec::new();
    for statement in added {
        let held = ref_signatures
            .get_mut(statements::strip_visibility(&statement))
            .filter(|left| **left > 0);
        match (held, signature_of(&statement)) {
            (Some(left), Some(signature)) => {
                *left -= 1;
                pairs += 1;
                forwardable.push(signature);
            }
            _ => rest.push(statement),
        }
    }

    let mut kept = Vec::new();
    for statement in rest {
        let slot = forwarding_call(&statement, declared)
            .and_then(|call| forwardable.iter().position(|signature| *signature == call));
        match slot {
            Some(at) => {
                forwardable.swap_remove(at);
                pairs += 1;
            }
            None => kept.push(statement),
        }
    }
    Passed {
        missing,
        added: kept,
        pairs,
    }
}

/// The method and parameter names of a `fn` signature line (one that ends `{`), when `statement`
/// is one.
fn signature_of(statement: &str) -> Option<Forwarded> {
    let line = statement.strip_suffix('{')?.trim_end();
    let at = line.match_indices("fn ").find_map(|(at, _)| {
        (at == 0 || line.as_bytes()[at - 1] == b' ').then_some(at + "fn ".len())
    })?;
    let rest = &line[at..];
    let method = &rest[..rest.find(|c: char| !(c.is_alphanumeric() || c == '_'))?];
    let open = rest.find('(')?;
    let inner = &rest[open + 1..matching_close(rest, open)?];
    let arguments = split_top_level(inner)
        .into_iter()
        .filter_map(|parameter| {
            let pattern = parameter
                .split_once(':')
                .map_or(parameter, |(head, _)| head);
            let pattern = pattern.trim();
            (pattern
                .split_whitespace()
                .last()
                .map(|word| word.trim_start_matches('&'))
                != Some("self"))
            .then(|| pattern.strip_prefix("mut ").unwrap_or(pattern).to_string())
        })
        .collect();
    Some(Forwarded {
        method: method.to_string(),
        arguments,
    })
}

/// The method and arguments of a forwarding call: `<receiver>.<m>(<arguments>)`, its `.await`ed
/// form, or `<NEW>::<m>(…)` for a `NEW` a declared retarget names.
fn forwarding_call(statement: &str, declared: &Declared) -> Option<Forwarded> {
    let call = statement.strip_suffix(".await").unwrap_or(statement);
    call.ends_with(')').then_some(())?;
    let open = opening_of_last_paren(call)?;
    let callee = &call[..open];
    let (receiver, method) = match callee.rsplit_once('.') {
        Some(split) => split,
        None => {
            let (path, method) = callee.rsplit_once("::")?;
            let new_type = path.rsplit("::").next()?;
            declared
                .retargets
                .iter()
                .any(|retarget| retarget.to == new_type)
                .then_some((path, method))?
        }
    };
    let is_name =
        |text: &str| !text.is_empty() && text.chars().all(|c| c.is_alphanumeric() || c == '_');
    (is_name(method) && !receiver.is_empty()).then(|| Forwarded {
        method: method.to_string(),
        arguments: split_top_level(&call[open + 1..call.len() - 1])
            .into_iter()
            .map(|argument| argument.trim().to_string())
            .collect(),
    })
}

/// The offset of the `(` that the `)` ending `call` closes.
fn opening_of_last_paren(call: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (at, byte) in call.bytes().enumerate().rev() {
        match byte {
            b')' => depth += 1,
            b'(' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}

/// The offset of the bracket closing the one opened at `open`, when it closes.
fn matching_close(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (at, byte) in text.bytes().enumerate().skip(open) {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}

/// `text` split at the commas that sit in no bracket (angle brackets included, a `->` not being
/// one), empty parts dropped.
fn split_top_level(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut start, mut previous) = (0usize, 0, ' ');
    for (at, character) in text.char_indices() {
        match character {
            '(' | '[' | '{' | '<' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            '>' if previous != '-' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&text[start..at]);
                start = at + 1;
            }
            _ => {}
        }
        previous = character;
    }
    parts.push(&text[start..]);
    parts.retain(|part| !part.trim().is_empty());
    parts
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

/// R4, repeated generic headers: a member move that opens a new block repeats the `impl` header of
/// the block it leaves — `impl<T> Host<T>`, its `where` clause and predicate lines are statements
/// `verify` reads. Needs no declaration, because the repeat is a whole header the ref already holds:
/// for each generic header group of `before`, a gained copy of **every** line of it (the `impl<…>`
/// line, the `where`, each predicate) is excused, once per copy. A gained `where` or predicate with
/// no whole header beside it, and a header the ref does not have, stay reported.
pub(crate) fn repeated_generic_headers(
    missing: Vec<String>,
    added: Vec<String>,
    before: &[String],
) -> Passed {
    let mut added = added;
    let mut pairs = 0;
    for group in header_groups(before)
        .into_iter()
        .filter(|group| statements::strip_visibility(&group[0]).starts_with("impl<"))
    {
        while let Some(positions) = positions_of(&added, &group) {
            for at in positions.into_iter().rev() {
                added.remove(at);
            }
            pairs += group.len();
        }
    }
    Passed {
        missing,
        added,
        pairs,
    }
}

/// The index in `added` of one copy of each line of `group` (a line twice in the group takes two),
/// when `added` holds every one.
fn positions_of(added: &[String], group: &[String]) -> Option<Vec<usize>> {
    let mut taken: Vec<usize> = Vec::new();
    for line in group {
        let at = added
            .iter()
            .enumerate()
            .position(|(at, candidate)| candidate == line && !taken.contains(&at))?;
        taken.push(at);
    }
    taken.sort_unstable();
    Some(taken)
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
