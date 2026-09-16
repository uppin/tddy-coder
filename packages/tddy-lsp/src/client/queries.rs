//! The LSP queries this client issues: diagnostics, definition, references, hover and symbols.
//! Each is one request whose `result` is read into the crate's own domain shapes.

use serde_json::{json, Value};

use super::parse::{
    parse_diagnostics, parse_location, parse_locations, parse_range, position_params,
};
use super::{Diagnostic, Location, LspClient, LspError, Position, SymbolInfo};

impl LspClient {
    /// Diagnostics for a document (cached `publishDiagnostics` or a pull request).
    pub async fn diagnostics(&self, uri: &str) -> Result<Vec<Diagnostic>, LspError> {
        if let Some(cached) = self.diagnostics.lock().unwrap().get(uri).cloned() {
            return Ok(cached);
        }
        // Nothing published yet — fall back to a pull diagnostic request.
        let result = self
            .request(
                "textDocument/diagnostic",
                json!({ "textDocument": { "uri": uri } }),
            )
            .await?;
        Ok(parse_diagnostics(result.get("items")))
    }

    /// Pull workspace-wide diagnostics, returning one `(uri, diagnostics)` group per
    /// reported document.
    pub async fn workspace_diagnostics(&self) -> Result<Vec<(String, Vec<Diagnostic>)>, LspError> {
        let result = self
            .request("workspace/diagnostic", json!({ "previousResultIds": [] }))
            .await?;
        let mut groups = Vec::new();
        if let Some(items) = result.get("items").and_then(Value::as_array) {
            for item in items {
                let uri = item
                    .get("uri")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                groups.push((uri, parse_diagnostics(item.get("items"))));
            }
        }
        Ok(groups)
    }

    /// Go-to-definition at a position.
    pub async fn definition(&self, uri: &str, pos: Position) -> Result<Vec<Location>, LspError> {
        let result = self
            .request("textDocument/definition", position_params(uri, pos))
            .await?;
        Ok(parse_locations(&result))
    }

    /// Find-references at a position.
    pub async fn references(&self, uri: &str, pos: Position) -> Result<Vec<Location>, LspError> {
        let mut params = position_params(uri, pos);
        params["context"] = json!({ "includeDeclaration": true });
        let result = self.request("textDocument/references", params).await?;
        Ok(parse_locations(&result))
    }

    /// Hover markdown at a position, if any.
    pub async fn hover(&self, uri: &str, pos: Position) -> Result<Option<String>, LspError> {
        let result = self
            .request("textDocument/hover", position_params(uri, pos))
            .await?;
        Ok(result
            .pointer("/contents/value")
            .and_then(Value::as_str)
            .map(str::to_string))
    }

    /// Document symbols for a file.
    pub async fn symbols(&self, uri: &str) -> Result<Vec<SymbolInfo>, LspError> {
        let result = self
            .request(
                "textDocument/documentSymbol",
                json!({ "textDocument": { "uri": uri } }),
            )
            .await?;
        let symbols = result
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .map(|item| SymbolInfo {
                        name: item
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        kind: item.get("kind").and_then(Value::as_u64).unwrap_or(0) as u8,
                        location: Location {
                            uri: uri.to_string(),
                            range: parse_range(item.get("range")),
                        },
                        container: item
                            .get("containerName")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(symbols)
    }

    /// Workspace symbol search.
    pub async fn workspace_symbols(&self, query: &str) -> Result<Vec<SymbolInfo>, LspError> {
        let result = self
            .request("workspace/symbol", json!({ "query": query }))
            .await?;
        let symbols = result
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .map(|item| SymbolInfo {
                        name: item
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        kind: item.get("kind").and_then(Value::as_u64).unwrap_or(0) as u8,
                        location: parse_location(item.get("location").unwrap_or(&Value::Null)),
                        container: item
                            .get("containerName")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(symbols)
    }
}
