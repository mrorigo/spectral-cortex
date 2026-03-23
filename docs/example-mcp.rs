use anyhow::{anyhow, Result};
use fnv::FnvHasher;
use std::hash::Hasher;

/// Computes the 2-character hex hash of a line's content (trimmed of trailing whitespace).
pub fn hash_line(content: &str) -> String {
    let trimmed = content.trim_end();
    let mut hasher = FnvHasher::default();
    hasher.write(trimmed.as_bytes());
    let hash = (hasher.finish() & 0xff) as u8;
    format!("{:02x}", hash)
}

/// Tags each line of the content with its line number and hash.
pub fn tag_content(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut result = String::new();
    for (i, line) in lines.iter().enumerate() {
        let h = hash_line(line);
        result.push_str(&format!("{}:{}|{}\n", i + 1, h, line));
    }
    result
}

/// Computes a 6-character hex hash of the entire file content using FNV.
/// This is shorter and more agent-friendly than SHA-256 while providing
/// sufficient collision resistance for practical file editing scenarios.
pub fn compute_file_hash(content: &str) -> String {
    let mut hasher = FnvHasher::default();
    hasher.write(content.as_bytes());
    let hash = hasher.finish();
    // Use 24 bits (6 hex chars) for reasonable collision resistance
    format!("{:06x}", hash & 0xFFFFFF)
}

#[derive(Debug, Clone, PartialEq)]
pub struct LineAnchor {
    pub line_num: usize,
    pub hash: String,
}

impl std::str::FromStr for LineAnchor {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 2 {
            return Err(anyhow!("Invalid anchor format. Expected 'line_num:hash'"));
        }
        let line_num = parts[0].parse::<usize>()?;
        let hash = parts[1].to_string();
        Ok(LineAnchor { line_num, hash })
    }
}

pub enum OperationType {
    Replace,
    InsertAfter,
    InsertBefore,
    Delete,
}

pub struct HashlineOperation {
    pub op_type: OperationType,
    pub anchor: LineAnchor,
    pub end_anchor: Option<LineAnchor>,
    pub content: Option<String>,
}

/// Resolves a line anchor to its current line index in the file.
/// Provides exact match first, then fuzzy match by hash if exactly one match is found.
pub fn resolve_anchor(lines: &[&str], anchor: &LineAnchor) -> Result<usize> {
    // 1-indexed to 0-indexed
    let idx = anchor.line_num.saturating_sub(1);

    // 1. Exact match
    if idx < lines.len() && hash_line(lines[idx]) == anchor.hash {
        return Ok(idx);
    }

    // 2. Fuzzy match (search for unique hash)
    let mut matches = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if hash_line(line) == anchor.hash {
            matches.push(i);
        }
    }

    if matches.len() == 1 {
        Ok(matches[0])
    } else if matches.is_empty() {
        Err(anyhow!(
            "Anchor {}:{} not found",
            anchor.line_num,
            anchor.hash
        ))
    } else {
        Err(anyhow!(
            "Anchor {}:{} is ambiguous ({} matches found)",
            anchor.line_num,
            anchor.hash,
            matches.len()
        ))
    }
}

pub fn apply_operations(content: &str, operations: Vec<HashlineOperation>) -> Result<String> {
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    // Sort operations by anchor line number in reverse to avoid index shifts affecting subsequent operations.
    // However, since anchors can move, we should resolve all anchors against ORIGINAL state first,
    // or apply them carefully.

    // For simplicity, we'll collect the resolved target indices first.
    let ref_lines: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    let mut resolved_ops: Vec<(usize, Option<usize>, OperationType, Option<String>)> = Vec::new();

    for op in operations {
        let start_idx = resolve_anchor(&ref_lines, &op.anchor)?;
        let end_idx = if let Some(ref end) = op.end_anchor {
            Some(resolve_anchor(&ref_lines, end)?)
        } else {
            None
        };
        resolved_ops.push((start_idx, end_idx, op.op_type, op.content));
    }

    // Sort by start_idx descending
    resolved_ops.sort_by(|a, b| b.0.cmp(&a.0));

    for (start, end, op_type, content) in resolved_ops {
        match op_type {
            OperationType::Replace => {
                let count = if let Some(e) = end {
                    if e < start {
                        return Err(anyhow!("End anchor is before start anchor"));
                    }
                    e - start + 1
                } else {
                    1
                };
                lines.drain(start..start + count);
                if let Some(c) = content {
                    let new_lines: Vec<String> = c.lines().map(|s| s.to_string()).collect();
                    for (i, nl) in new_lines.into_iter().enumerate() {
                        lines.insert(start + i, nl);
                    }
                }
            }
            OperationType::Delete => {
                let count = if let Some(e) = end {
                    if e < start {
                        return Err(anyhow!("End anchor is before start anchor"));
                    }
                    e - start + 1
                } else {
                    1
                };
                lines.drain(start..start + count);
            }
            OperationType::InsertAfter => {
                if let Some(c) = content {
                    let new_lines: Vec<String> = c.lines().map(|s| s.to_string()).collect();
                    for (i, nl) in new_lines.into_iter().enumerate() {
                        lines.insert(start + 1 + i, nl);
                    }
                }
            }
            OperationType::InsertBefore => {
                if let Some(c) = content {
                    let new_lines: Vec<String> = c.lines().map(|s| s.to_string()).collect();
                    for (i, nl) in new_lines.into_iter().enumerate() {
                        lines.insert(start + i, nl);
                    }
                }
            }
        }
    }

    let mut result = lines.join("\n");
    if content.ends_with('\n') && !result.ends_with('\n') {
        result.push('\n');
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_line() {
        assert_eq!(hash_line("hello"), hash_line("hello  "));
        assert_ne!(hash_line("hello"), hash_line("world"));
        let h = hash_line("test");
        assert_eq!(h.len(), 2);
    }

    #[test]
    fn test_apply_operations() -> Result<()> {
        let content = "line1\nline2\nline3\n";
        let h2 = hash_line("line2");
        let ops = vec![HashlineOperation {
            op_type: OperationType::Replace,
            anchor: format!("2:{}", h2).parse()?,
            end_anchor: None,
            content: Some("new line 2".to_string()),
        }];

        let result = apply_operations(content, ops)?;
        assert_eq!(result, "line1\nnew line 2\nline3\n");
        Ok(())
    }
}
mod hashline;
mod tools;

use rmcp::{model::*, tool_handler, transport::stdio, ServerHandler, ServiceExt};

use crate::tools::HashfileServer;

#[tool_handler]
impl ServerHandler for HashfileServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("Hashfile MCP Server - provides reliable file editing using hash-anchored operations.".into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let service = HashfileServer::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
use std::path::PathBuf;
use rmcp::model::Root;
use anyhow::{Result, anyhow};
use url::Url;

pub struct RootsManager {
    roots: Vec<Root>,
}

impl RootsManager {
    pub fn new() -> Self {
        Self { roots: Vec::new() }
    }

    pub fn set_roots(&mut self, roots: Vec<Root>) {
        self.roots = roots;
    }

    pub fn is_path_allowed(&self, path_str: &str) -> Result<bool> {
        let path = PathBuf::from(path_str);
        
        // Ensure path is absolute
        if !path.is_absolute() {
            return Err(anyhow!("Path must be absolute: {}", path_str));
        }

        // We use canonicalize to resolve symlinks and '..' for security.
        // If the path doesn't exist, we check its parent.
        let absolute_path = if path.exists() {
            match path.canonicalize() {
                Ok(p) => p,
                Err(e) => return Err(anyhow!("Failed to canonicalize path {}: {}", path_str, e)),
            }
        } else {
            let parent = path.parent().ok_or_else(|| anyhow!("Path has no parent"))?;
            if parent.exists() {
                match parent.canonicalize() {
                    Ok(p) => p.join(path.file_name().ok_or_else(|| anyhow!("Invalid filename"))?),
                    Err(e) => return Err(anyhow!("Failed to canonicalize parent of {}: {}", path_str, e)),
                }
            } else {
                return Ok(false);
            }
        };

        for root in &self.roots {
            if let Ok(root_uri) = Url::parse(&root.uri) {
                if root_uri.scheme() == "file" {
                    let root_path_str = root_uri.path();
                    let root_path = PathBuf::from(root_path_str);
                    if let Ok(abs_root) = root_path.canonicalize() {
                        if absolute_path.starts_with(abs_root) {
                            return Ok(true);
                        }
                    } else if absolute_path.starts_with(root_path) {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }
}
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    schemars,
};
use serde::Deserialize;
use std::fs;

use crate::hashline;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReadTextInput {
    #[schemars(description = "Absolute path to the file to read")]
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WriteTextInput {
    #[schemars(description = "Absolute path to the file to write")]
    pub path: String,
    #[schemars(description = "Content to write to the file")]
    pub content: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EditTextInput {
    #[schemars(description = "Absolute path to the file to edit")]
    pub path: String,
    #[schemars(description = "6-character hash of the entire file content from the last read")]
    pub file_hash: String,
    pub operations: Vec<EditOperation>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EditOperation {
    #[schemars(description = "Type of operation: replace, insert_after, insert_before, or delete")]
    pub op_type: String,
    #[schemars(description = "Anchor in lineNum:hash format")]
    pub anchor: String,
    #[schemars(description = "Optional end anchor in lineNum:hash format for range operations")]
    pub end_anchor: Option<String>,
    #[schemars(description = "New content for replace or insert operations")]
    pub content: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HashfileServer {
    pub tool_router: ToolRouter<Self>,
}

#[rmcp::tool_router]
impl HashfileServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[rmcp::tool(
        description = "Read a file and return hashline-tagged content for reliable editing"
    )]
    fn read_text_file(
        &self,
        Parameters(ReadTextInput { path }): Parameters<ReadTextInput>,
    ) -> String {
        match Self::read_text_file_impl(&path) {
            Ok(output) => output,
            Err(e) => format!("Error: {}", e),
        }
    }

    #[rmcp::tool(description = "Write content to a file, creating it if it doesn't exist")]
    fn write_text_file(&self, Parameters(input): Parameters<WriteTextInput>) -> String {
        match Self::write_text_file_impl(&input.path, &input.content) {
            Ok(msg) => msg,
            Err(e) => format!("Error: {}", e),
        }
    }

    #[rmcp::tool(description = "Edit a file using hash-anchored operations")]
    fn edit_text_file(&self, Parameters(input): Parameters<EditTextInput>) -> String {
        match Self::edit_text_file_impl(&input.path, &input.file_hash, input.operations) {
            Ok(msg) => msg,
            Err(e) => format!("Error: {}", e),
        }
    }
}

impl HashfileServer {
    fn read_text_file_impl(path: &str) -> anyhow::Result<String> {
        let content = fs::read_to_string(path)?;
        let tagged = hashline::tag_content(&content);
        let file_hash = hashline::compute_file_hash(&content);
        let total_lines = content.lines().count();

        let output = format!(
            "{}\n---\nhashline_version: 1\ntotal_lines: {}\nfile_hash: {}\n",
            tagged, total_lines, file_hash
        );

        Ok(output)
    }

    fn write_text_file_impl(path: &str, content: &str) -> anyhow::Result<String> {
        fs::write(path, content)?;
        Ok(format!("Successfully wrote {} bytes to {}", content.len(), path))
    }

    fn edit_text_file_impl(
        path: &str,
        file_hash: &str,
        operations: Vec<EditOperation>,
    ) -> anyhow::Result<String> {
        let current_content = fs::read_to_string(path)?;
        let current_hash = hashline::compute_file_hash(&current_content);

        if current_hash != file_hash {
            return Err(anyhow::anyhow!(
                "File {} has been modified since it was last read. Please re-read the file.",
                path
            ));
        }

        let mut ops = Vec::new();
        for op in operations {
            let anchor = op.anchor.parse::<hashline::LineAnchor>()?;
            let end_anchor = if let Some(ea) = op.end_anchor {
                Some(ea.parse::<hashline::LineAnchor>()?)
            } else {
                None
            };

            let op_type = match op.op_type.as_str() {
                "replace" => hashline::OperationType::Replace,
                "insert_after" => hashline::OperationType::InsertAfter,
                "insert_before" => hashline::OperationType::InsertBefore,
                "delete" => hashline::OperationType::Delete,
                _ => return Err(anyhow::anyhow!("Invalid operation type: {}", op.op_type)),
            };

            ops.push(hashline::HashlineOperation {
                op_type,
                anchor,
                end_anchor,
                content: op.content,
            });
        }

        let new_content = hashline::apply_operations(&current_content, ops)?;
        fs::write(path, &new_content)?;

        Ok(format!("Successfully edited {}", path))
    }
}
