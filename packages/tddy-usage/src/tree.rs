//! The conversation tree: children located from each conversation's files, to any depth.

use std::collections::HashSet;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use crate::jsonl::fold_files;
use crate::template::TemplateError;
use crate::token_accounting::ConversationRecord;
use crate::{ChildrenSpec, FieldPath, IdentitySpec, JsonlSource, PathTemplate, UsageContext};

/// How many levels of children below the main conversation are followed.
const MAX_DEPTH: usize = 16;

/// How many leading lines of a file are searched for its identity and match fields.
const HEADER_LINES: usize = 50;

/// The variables a children template binds: the parent or child file's directory and stem.
pub(crate) const FILE_VARIABLES: &[&str] = &["dir", "stem"];

/// The main conversation's variables.
pub(crate) const MAIN_VARIABLES: &[&str] =
    &["agent_home", "session_dir", "conversation_id", "worktree"];

/// The rows `source` yields for the session `ctx` describes: the main conversation (every file
/// matching `main.path` merged into one row), then its children depth-first, siblings by id.
/// `None` when the session has no conversation id to look for.
pub(crate) fn conversation_rows(
    source: &JsonlSource,
    ctx: &UsageContext,
) -> Option<Vec<ConversationRecord>> {
    if let Some(identity) = &source.identity {
        return identified_rows(source, identity, ctx);
    }
    let id = ctx.conversation_id.clone()?;
    let files = main_files(source, ctx);
    let main = row(&ctx.agent_label, &id, None, &files, source, ctx);
    let mut rows = vec![main];

    let mut visited: HashSet<PathBuf> = files.iter().cloned().collect();
    if let Some(children) = &source.children {
        let mut found = Vec::new();
        for file in &files {
            found.extend(children_of(
                file,
                children,
                &id,
                1,
                &mut visited,
                source,
                ctx,
            ));
        }
        rows.extend(in_sibling_order(found));
    }
    Some(rows)
}

/// The files `main.path` matches, in path order. A template that cannot be rendered — a variable
/// with no value for this session — matches none.
fn main_files(source: &JsonlSource, ctx: &UsageContext) -> Vec<PathBuf> {
    match render_main(&source.main.path, ctx, true) {
        Some(pattern) => glob_files(&pattern),
        None => Vec::new(),
    }
}

/// `template` over the main conversation's variables; the values are glob-escaped when the result
/// is a glob pattern. `None` when a variable has no value for this session.
fn render_main(template: &PathTemplate, ctx: &UsageContext, as_glob: bool) -> Option<String> {
    let lookup = |name: &str| -> Option<String> {
        let value = match name {
            "agent_home" => ctx.agent_home.as_ref().map(|p| p.display().to_string()),
            "session_dir" => Some(ctx.session_dir.display().to_string()),
            "conversation_id" => ctx.conversation_id.clone(),
            "worktree" => ctx.worktree.as_ref().map(|p| p.display().to_string()),
            _ => None,
        };
        value.map(|v| {
            if as_glob {
                glob::Pattern::escape(&v)
            } else {
                v
            }
        })
    };
    match template.render(MAIN_VARIABLES, &lookup) {
        Ok(text) => Some(text),
        Err(TemplateError::Unset { .. }) => None,
        Err(e) => {
            log::warn!("usage: {e}");
            None
        }
    }
}

/// A conversation file's identity, read from its header.
struct Identified {
    file: PathBuf,
    id: String,
    parent: Option<String>,
    /// Whether the header satisfies `main.match` (true when the source has none).
    matches_main: bool,
}

/// The rows of a source whose conversations name themselves: the main conversation is the newest
/// root file matching `main.match`, its children every file declaring a conversation as its parent,
/// to any depth. `None` when no file qualifies as the main conversation.
fn identified_rows(
    source: &JsonlSource,
    identity: &IdentitySpec,
    ctx: &UsageContext,
) -> Option<Vec<ConversationRecord>> {
    let wanted = match &source.main.match_ {
        Some(m) => Some((&m.field, render_main(&m.equals, ctx, false)?)),
        None => None,
    };
    let files: Vec<Identified> = main_files(source, ctx)
        .into_iter()
        .filter_map(|file| {
            let header = header_values(&file);
            let id = header_string(&header, &identity.id)?;
            let parent = identity
                .parent
                .as_ref()
                .and_then(|path| header_string(&header, path));
            let matches_main = wanted.as_ref().is_none_or(|(field, equals)| {
                header_string(&header, field).as_ref() == Some(equals)
            });
            Some(Identified {
                file,
                id,
                parent,
                matches_main,
            })
        })
        .collect();
    let main = files
        .iter()
        .filter(|f| f.parent.is_none() && f.matches_main)
        .max_by_key(|f| {
            let modified = std::fs::metadata(&f.file).and_then(|m| m.modified()).ok();
            (modified, f.file.clone())
        })?;

    let mut rows = vec![row(
        &ctx.agent_label,
        &main.id,
        None,
        std::slice::from_ref(&main.file),
        source,
        ctx,
    )];
    let mut visited: HashSet<&str> = HashSet::from([main.id.as_str()]);
    if source.children_by.is_some() {
        spawned_rows(&main.id, &files, 1, &mut visited, source, ctx, &mut rows);
    }
    Some(rows)
}

/// The rows of every file declaring `parent_id` as its parent, depth-first, siblings by id.
fn spawned_rows<'a>(
    parent_id: &str,
    files: &'a [Identified],
    depth: usize,
    visited: &mut HashSet<&'a str>,
    source: &JsonlSource,
    ctx: &UsageContext,
    rows: &mut Vec<ConversationRecord>,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let mut children: Vec<&'a Identified> = files
        .iter()
        .filter(|f| f.parent.as_deref() == Some(parent_id))
        .collect();
    children.sort_by(|a, b| a.id.cmp(&b.id));
    for child in children {
        if !visited.insert(child.id.as_str()) {
            continue;
        }
        rows.push(row(
            &child.id,
            &child.id,
            Some(parent_id),
            std::slice::from_ref(&child.file),
            source,
            ctx,
        ));
        spawned_rows(&child.id, files, depth + 1, visited, source, ctx, rows);
    }
}

/// The JSON lines among the file's first [`HEADER_LINES`]; an unreadable file has none.
fn header_values(file: &Path) -> Vec<serde_json::Value> {
    let Ok(opened) = std::fs::File::open(file) else {
        return Vec::new();
    };
    std::io::BufReader::new(opened)
        .lines()
        .take(HEADER_LINES)
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str(&line).ok())
        .collect()
}

/// The string at `field` on the first header line that holds it.
fn header_string(header: &[serde_json::Value], field: &FieldPath) -> Option<String> {
    header
        .iter()
        .find_map(|value| field.lookup(value)?.as_str().map(str::to_string))
}

/// The child conversations of `parent_file`, each with its own descendants, in sibling order.
fn children_of(
    parent_file: &Path,
    children: &ChildrenSpec,
    parent_id: &str,
    depth: usize,
    visited: &mut HashSet<PathBuf>,
    source: &JsonlSource,
    ctx: &UsageContext,
) -> Vec<(ConversationRecord, Vec<ConversationRecord>)> {
    if depth > MAX_DEPTH {
        return Vec::new();
    }
    let Some(pattern) = render_over_file(&children.path, parent_file, true) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for file in glob_files(&pattern) {
        if !visited.insert(file.clone()) {
            continue;
        }
        let Some(id) = render_over_file(&children.id, &file, false) else {
            continue;
        };
        let name = child_name(children, &file, &id);
        let record = row(
            &name,
            &id,
            Some(parent_id),
            std::slice::from_ref(&file),
            source,
            ctx,
        );
        let grandchildren = children_of(&file, children, &id, depth + 1, visited, source, ctx);
        found.push((record, in_sibling_order(grandchildren)));
    }
    found
}

/// Siblings sorted by id, each followed by its own descendants.
fn in_sibling_order(
    mut found: Vec<(ConversationRecord, Vec<ConversationRecord>)>,
) -> Vec<ConversationRecord> {
    found.sort_by(|a, b| a.0.id.cmp(&b.0.id));
    found
        .into_iter()
        .flat_map(|(child, descendants)| std::iter::once(child).chain(descendants))
        .collect()
}

/// The child's display name: the sidecar's field, else the spec's default; its id when the spec
/// declares no name.
fn child_name(children: &ChildrenSpec, file: &Path, id: &str) -> String {
    let Some(name) = &children.name else {
        return id.to_string();
    };
    render_over_file(&name.sidecar, file, false)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|json| field_string(&name.field, &json))
        .unwrap_or_else(|| name.default.clone())
}

fn field_string(field: &FieldPath, json: &serde_json::Value) -> Option<String> {
    field.lookup(json)?.as_str().map(str::to_string)
}

/// `template` over `file`'s `{dir}` and `{stem}`; glob metacharacters in the values are escaped
/// when the result is a glob pattern.
fn render_over_file(template: &PathTemplate, file: &Path, as_glob: bool) -> Option<String> {
    let dir = file.parent()?.display().to_string();
    let stem = file.file_stem()?.to_string_lossy().into_owned();
    let lookup = |name: &str| -> Option<String> {
        let value = match name {
            "dir" => dir.clone(),
            "stem" => stem.clone(),
            _ => return None,
        };
        Some(if as_glob {
            glob::Pattern::escape(&value)
        } else {
            value
        })
    };
    match template.render(FILE_VARIABLES, &lookup) {
        Ok(text) => Some(text),
        Err(e) => {
            log::warn!("usage: {e}");
            None
        }
    }
}

fn glob_files(pattern: &str) -> Vec<PathBuf> {
    match glob::glob(pattern) {
        Ok(paths) => {
            let mut files: Vec<PathBuf> = paths
                .filter_map(Result::ok)
                .filter(|p| p.is_file())
                .collect();
            files.sort();
            files
        }
        Err(e) => {
            log::warn!("usage: bad path pattern '{pattern}': {e}");
            Vec::new()
        }
    }
}

/// One conversation's row, folded from `files` (none: a zero row on the fallback model).
fn row(
    agent: &str,
    id: &str,
    parent_id: Option<&str>,
    files: &[PathBuf],
    source: &JsonlSource,
    ctx: &UsageContext,
) -> ConversationRecord {
    let folded = fold_files(files, &source.record);
    ConversationRecord {
        agent: agent.to_string(),
        id: id.to_string(),
        model: folded.model.unwrap_or_else(|| ctx.fallback_model.clone()),
        input_tokens: folded.input,
        output_tokens: folded.output,
        total_tokens: folded.input
            + folded.cache_read
            + folded.cache_write
            + folded.output
            + folded.reasoning,
        turns: folded.turns,
        cache_read_tokens: folded.cache_read,
        cache_write_tokens: folded.cache_write,
        reasoning_tokens: folded.reasoning,
        cost_usd: folded.cost,
        parent_id: parent_id.map(str::to_string),
    }
}
