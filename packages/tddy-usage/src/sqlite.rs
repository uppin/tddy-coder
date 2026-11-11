//! The `sqlite` format: one row per conversation in a table, read through a read-only connection
//! so a running agent's writer is never contended (`#usage-binding 5/5`).

use std::collections::HashSet;
use std::time::Duration;

use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags};
use serde_json::{Map, Number, Value};

use crate::jsonl::Folded;
use crate::token_accounting::ConversationRecord;
use crate::tree::{record_of, render_main, MAX_DEPTH};
use crate::{SqliteSource, UsageContext};

/// How long a reader waits on a writer before reporting the store unavailable.
const BUSY_TIMEOUT: Duration = Duration::from_millis(200);

/// A plain SQL identifier: the only shape a table or column name may take in a binding.
pub(crate) fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Every table and column name `source` puts into SQL, with the binding position it was written
/// at, so a refusal can say where.
pub(crate) fn named_identifiers(source: &SqliteSource) -> Vec<(&'static str, &str)> {
    let mut names = vec![
        ("table", source.table.as_str()),
        ("id", source.id.as_str()),
        ("root.column", source.root.column.as_str()),
        ("root.newest_by", source.root.newest_by.as_str()),
    ];
    if let Some(parent) = &source.parent {
        names.push(("parent", parent.as_str()));
    }
    for (position, field) in [
        ("cost", &source.cost),
        ("model", &source.model),
        ("name", &source.name),
    ] {
        if let Some(field) = field {
            names.push((position, field.as_str()));
        }
    }
    let tokens = &source.tokens;
    for (position, expr) in [
        ("tokens.input", &tokens.input),
        ("tokens.cache_read", &tokens.cache_read),
        ("tokens.cache_write", &tokens.cache_write),
        ("tokens.output", &tokens.output),
        ("tokens.reasoning", &tokens.reasoning),
    ] {
        if let Some(expr) = expr {
            names.extend(expr.paths().map(|path| (position, path.as_str())));
        }
    }
    names
}

/// The rows `source` yields for the session `ctx` describes: the main conversation (the newest root
/// row matching `root`, or a zero row when the store or the row is absent), then its children
/// depth-first, siblings by id. `None` when the store cannot be read now — busy, locked or
/// malformed — so the caller keeps its last snapshot rather than showing zeros.
pub(crate) fn conversation_rows(
    source: &SqliteSource,
    ctx: &UsageContext,
) -> Option<Vec<ConversationRecord>> {
    let path = render_main(&source.path, ctx, false)?;
    let wanted = render_main(&source.root.equals, ctx, false)?;
    if !std::path::Path::new(&path).is_file() {
        return Some(vec![zero_main_row(ctx)]);
    }
    let query = Queries::of(source);
    let db = open_read_only(&path)
        .map_err(|e| log::warn!("usage: cannot open {path}: {e}"))
        .ok()?;
    let Some(main) = query
        .root(&db, &wanted)
        .map_err(|e| log::warn!("usage: cannot read {path}: {e}"))
        .ok()?
    else {
        return Some(vec![zero_main_row(ctx)]);
    };

    let main_id = main.id.clone();
    let mut rows = vec![record(source, &ctx.agent_label, None, &main, ctx)];
    if source.parent.is_some() {
        let mut visited = HashSet::from([main_id.clone()]);
        let walked = walk_children(
            &db,
            &query,
            source,
            ctx,
            &main_id,
            1,
            &mut visited,
            &mut rows,
        );
        if let Err(e) = walked {
            log::warn!("usage: cannot read {path}: {e}");
            return None;
        }
    }
    Some(rows)
}

/// The main row of a session whose store has no conversation yet. Its id is the conversation id
/// the session knows, else the agent's label.
fn zero_main_row(ctx: &UsageContext) -> ConversationRecord {
    let id = ctx
        .conversation_id
        .clone()
        .unwrap_or_else(|| ctx.agent_label.clone());
    record_of(&ctx.agent_label, &id, None, Folded::default(), ctx)
}

fn open_read_only(path: &str) -> rusqlite::Result<Connection> {
    let db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    db.busy_timeout(BUSY_TIMEOUT)?;
    Ok(db)
}

/// One conversation row as read: its id and the selected columns as a JSON object.
struct Found {
    id: String,
    columns: Value,
}

/// The two statements a source runs, built only from identifiers validated at load.
struct Queries {
    id_column: String,
    columns: Vec<String>,
    root: String,
    children: Option<String>,
}

impl Queries {
    fn of(source: &SqliteSource) -> Self {
        let mut columns: Vec<String> = Vec::new();
        let selected = named_identifiers(source)
            .into_iter()
            .filter(|(position, _)| {
                !matches!(*position, "table" | "root.column" | "root.newest_by")
            })
            .map(|(_, name)| name);
        for name in selected {
            if !columns.iter().any(|c| c == name) {
                columns.push(name.to_string());
            }
        }
        let select = columns
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let table = &source.table;
        let root_column = &source.root.column;
        let newest = &source.root.newest_by;
        let top_level = source
            .parent
            .as_ref()
            .map(|parent| format!(" AND \"{parent}\" IS NULL"))
            .unwrap_or_default();
        let root = format!(
            "SELECT {select} FROM \"{table}\" WHERE \"{root_column}\" = ?1{top_level} \
             ORDER BY \"{newest}\" DESC LIMIT 1"
        );
        let children = source.parent.as_ref().map(|parent| {
            format!(
                "SELECT {select} FROM \"{table}\" WHERE \"{parent}\" = ?1 ORDER BY \"{}\"",
                source.id
            )
        });
        Queries {
            id_column: source.id.clone(),
            columns,
            root,
            children,
        }
    }

    fn root(&self, db: &Connection, wanted: &str) -> rusqlite::Result<Option<Found>> {
        let mut found = self.run(db, &self.root, wanted)?;
        Ok(found.pop())
    }

    fn children_of(&self, db: &Connection, parent_id: &str) -> rusqlite::Result<Vec<Found>> {
        match &self.children {
            Some(sql) => self.run(db, sql, parent_id),
            None => Ok(Vec::new()),
        }
    }

    fn run(&self, db: &Connection, sql: &str, bound: &str) -> rusqlite::Result<Vec<Found>> {
        let mut statement = db.prepare(sql)?;
        let rows = statement.query_map([bound], |row| {
            let mut object = Map::new();
            for (index, name) in self.columns.iter().enumerate() {
                object.insert(name.clone(), json_of(row.get_ref(index)?));
            }
            Ok(Value::Object(object))
        })?;
        let mut found = Vec::new();
        for columns in rows {
            let columns = columns?;
            // A row without an id cannot be nested under or reported; it is skipped.
            let id = match columns.get(&self.id_column) {
                Some(Value::String(id)) => id.clone(),
                Some(Value::Number(id)) => id.to_string(),
                _ => continue,
            };
            found.push(Found { id, columns });
        }
        Ok(found)
    }
}

/// A column value as JSON; a blob is not a figure and reads as null.
fn json_of(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Integer(i) => Value::Number(i.into()),
        ValueRef::Real(f) => Number::from_f64(f).map_or(Value::Null, Value::Number),
        ValueRef::Text(bytes) => Value::String(String::from_utf8_lossy(bytes).into_owned()),
        ValueRef::Null | ValueRef::Blob(_) => Value::Null,
    }
}

/// The rows of every child of `parent_id`, depth-first, siblings by id.
#[allow(clippy::too_many_arguments)]
fn walk_children(
    db: &Connection,
    query: &Queries,
    source: &SqliteSource,
    ctx: &UsageContext,
    parent_id: &str,
    depth: usize,
    visited: &mut HashSet<String>,
    rows: &mut Vec<ConversationRecord>,
) -> rusqlite::Result<()> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    for child in query.children_of(db, parent_id)? {
        if !visited.insert(child.id.clone()) {
            continue;
        }
        let name = source
            .name
            .as_ref()
            .and_then(|field| field.lookup(&child.columns))
            .and_then(Value::as_str)
            .unwrap_or(&child.id)
            .to_string();
        rows.push(record(source, &name, Some(parent_id), &child, ctx));
        walk_children(db, query, source, ctx, &child.id, depth + 1, visited, rows)?;
    }
    Ok(())
}

/// One conversation's record from its row: counters from the counter expressions, the cost and
/// model from their columns. `turns` is not a column of such a store and reads 0.
fn record(
    source: &SqliteSource,
    agent: &str,
    parent_id: Option<&str>,
    found: &Found,
    ctx: &UsageContext,
) -> ConversationRecord {
    let columns = &found.columns;
    let number = |expr: &Option<crate::CounterExpr>| {
        expr.as_ref().and_then(|e| e.evaluate(columns)).unwrap_or(0)
    };
    let tokens = &source.tokens;
    let folded = Folded {
        input: number(&tokens.input),
        cache_read: number(&tokens.cache_read),
        cache_write: number(&tokens.cache_write),
        output: number(&tokens.output),
        reasoning: number(&tokens.reasoning),
        cost: source
            .cost
            .as_ref()
            .and_then(|field| field.lookup(columns))
            .and_then(Value::as_f64),
        model: source
            .model
            .as_ref()
            .and_then(|field| field.lookup(columns))
            .and_then(Value::as_str)
            .map(str::to_string),
        turns: 0,
    };
    record_of(agent, &found.id, parent_id, folded, ctx)
}
