// Rust guideline compliant 2026-02-15

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, RwLock,
};

use anyhow::{Context, Result};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool_handler,
    transport::stdio,
    ErrorData as McpError, ServerHandler, ServiceExt,
};
use serde::Deserialize;
use serde_json::{json, Value};
use spectral_cortex::{load_smg_json, SpectralMemoryGraph};

const DEFAULT_TOP_K: usize = 5;
// const DEFAULT_LINKS_K: usize = 3;
const DEFAULT_SNIPPET_CHARS: usize = 140;

fn markdown_output_schema() -> Arc<JsonObject> {
    Arc::new(
        json!({
            "type": "object",
            "properties": { "markdown": { "type": "string" } },
            "required": ["markdown"],
            "additionalProperties": false
        })
        .as_object()
        .cloned()
        .unwrap_or_default(),
    )
}

fn query_output_schema() -> Arc<JsonObject> {
    Arc::new(json!({"type":"object","properties":{"markdown":{"type":"string"},"results":{"type":"array","items":{"type":"object"}}},"required":["markdown","results"],"additionalProperties":false}).as_object().cloned().unwrap_or_default())
}

/// Input for querying the graph with a text query.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct QueryGraphInput {
    #[schemars(description = "Text query to run against the graph")]
    pub query: String,
    #[schemars(description = "Number of rows to return (default: 5)")]
    pub top_k: Option<usize>,
    #[schemars(description = "Number of related notes per hit (default: 3)")]
    pub links_k: Option<usize>,
    #[schemars(description = "Maximum characters per snippet (default: 140)")]
    pub snippet_chars: Option<usize>,
    #[schemars(description = "Optional minimum score threshold")]
    pub min_score: Option<f32>,
    #[schemars(description = "Optional file path substring filter")]
    pub file: Option<String>,
    #[schemars(description = "Optional symbol ID substring filter")]
    pub symbol: Option<String>,
    #[schemars(description = "Temporal reranking weight, from 0 to 1 (default: 0.2)")]
    pub temporal_weight: Option<f32>,
    pub candidate_k: Option<usize>,
    pub no_temporal: Option<bool>,
    pub temporal_mode: Option<String>,
    pub temporal_half_life_days: Option<f32>,
    pub time_start: Option<String>,
    pub time_end: Option<String>,
    pub time_window_days: Option<f64>,
    pub temporal_now: Option<String>,
}

/// Input for inspecting one note and its related notes.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InspectNoteInput {
    #[schemars(description = "Note id to inspect")]
    pub note_id: u32,
    #[schemars(description = "Number of related notes to include (default: 10)")]
    pub links_k: Option<usize>,
    #[schemars(description = "Maximum characters per snippet (default: 140)")]
    pub snippet_chars: Option<usize>,
}

/// Input for listing long-range links.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LongRangeLinksInput {
    #[schemars(description = "Number of long-range links to include (default: 20)")]
    pub top_k: Option<usize>,
}

/// Input for quick graph summary.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GraphSummaryInput {
    /// Optional commit ID whose topology counts should be reported.
    pub topology_commit_id: Option<String>,
}

/// Input for structural hotspots analysis.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct StructuralHotspotsInput {
    #[schemars(description = "Number of hotspots to return (default: 10)")]
    pub top_k: Option<usize>,
}

/// Input for inspecting symbol history.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SymbolHistoryInput {
    #[schemars(description = "Stable symbol_id to inspect")]
    pub symbol_id: String,
    #[schemars(description = "Maximum number of history entries to return (default: all)")]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CommitDiffInput {
    pub commit_id: String,
    pub file: Option<String>,
}

/// MCP server that provides compact tools for SMG query and inspection.
#[derive(Clone)]
pub struct SpectralCortexMcpServer {
    #[allow(dead_code)]
    pub tool_router: ToolRouter<Self>,
    pub smg_path: String,
    pub smg: Arc<RwLock<Arc<SpectralMemoryGraph>>>,
}

#[tool_handler]
impl ServerHandler for SpectralCortexMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(ProtocolVersion::V_2026_07_28)
            .with_instructions(
                "Spectral Cortex MCP Server: compact markdown tools for querying a preloaded SMG file. Prefer small top_k values to keep responses token efficient.",
            )
    }
}

#[rmcp::tool_router]
impl SpectralCortexMcpServer {
    fn success_result(markdown: String) -> CallToolResult {
        let mut result = CallToolResult::structured(json!({ "markdown": markdown }));
        result.content = vec![ContentBlock::text(
            result
                .structured_content
                .as_ref()
                .and_then(|value| value.get("markdown"))
                .and_then(|value| value.as_str())
                .unwrap_or_default(),
        )];
        result
    }

    /// Construct a new server instance.
    pub fn new(smg_path: String, smg: SpectralMemoryGraph) -> Self {
        Self {
            tool_router: Self::tool_router(),
            smg_path,
            smg: Arc::new(RwLock::new(Arc::new(smg))),
        }
    }

    /// Query the graph and return a compact markdown table.
    #[rmcp::tool(
        description = "Run semantic query against an SMG and return token-efficient markdown results",
        output_schema = query_output_schema()
    )]
    fn query_graph(
        &self,
        Parameters(input): Parameters<QueryGraphInput>,
    ) -> Result<CallToolResult, McpError> {
        match self.query_graph_impl(input) {
            Ok((markdown, results)) => {
                let mut result =
                    CallToolResult::structured(json!({"markdown": markdown, "results": results}));
                result.content = vec![ContentBlock::text(markdown)];
                Ok(result)
            }
            Err(err) => Err(McpError::internal_error(err.to_string(), None)),
        }
    }

    /// Inspect a single note and show top related notes with similarity.
    #[rmcp::tool(
        description = "Inspect one note by id and return compact markdown with related notes",
        output_schema = markdown_output_schema()
    )]
    fn inspect_note(
        &self,
        Parameters(input): Parameters<InspectNoteInput>,
    ) -> Result<CallToolResult, McpError> {
        match self.inspect_note_impl(input) {
            Ok(output) => Ok(Self::success_result(output)),
            Err(err) => Err(McpError::internal_error(err.to_string(), None)),
        }
    }

    /// List long-range links from the graph.
    #[rmcp::tool(
        description = "List long-range spectral links as compact markdown",
        output_schema = markdown_output_schema()
    )]
    fn long_range_links(
        &self,
        Parameters(input): Parameters<LongRangeLinksInput>,
    ) -> Result<CallToolResult, McpError> {
        match self.long_range_links_impl(input) {
            Ok(output) => Ok(Self::success_result(output)),
            Err(err) => Err(McpError::internal_error(err.to_string(), None)),
        }
    }

    /// Return a small summary of graph size and available structures.
    #[rmcp::tool(
        description = "Return compact summary metadata for an SMG",
        output_schema = markdown_output_schema()
    )]
    fn graph_summary(
        &self,
        Parameters(input): Parameters<GraphSummaryInput>,
    ) -> Result<CallToolResult, McpError> {
        match self.graph_summary_impl(input) {
            Ok(output) => Ok(Self::success_result(output)),
            Err(err) => Err(McpError::internal_error(err.to_string(), None)),
        }
    }

    /// Retrieve the most churn-heavy structural hotspots.
    #[rmcp::tool(
        description = "Report top churn-heavy AST symbols",
        output_schema = markdown_output_schema()
    )]
    fn get_structural_hotspots(
        &self,
        Parameters(input): Parameters<StructuralHotspotsInput>,
    ) -> Result<CallToolResult, McpError> {
        match self.get_structural_hotspots_impl(input) {
            Ok(output) => Ok(Self::success_result(output)),
            Err(err) => Err(McpError::internal_error(err.to_string(), None)),
        }
    }

    /// Retrieve chronological change history for a single symbol.
    #[rmcp::tool(
        description = "Retrieve timeline of a symbol's evolution",
        output_schema = markdown_output_schema()
    )]
    fn inspect_symbol_history(
        &self,
        Parameters(input): Parameters<SymbolHistoryInput>,
    ) -> Result<CallToolResult, McpError> {
        match self.inspect_symbol_history_impl(input) {
            Ok(output) => Ok(Self::success_result(output)),
            Err(err) => Err(McpError::internal_error(err.to_string(), None)),
        }
    }

    #[rmcp::tool(description = "Retrieve a bounded commit diff excerpt from the source Git repository", output_schema = markdown_output_schema())]
    fn get_commit_diff(
        &self,
        Parameters(input): Parameters<CommitDiffInput>,
    ) -> Result<CallToolResult, McpError> {
        match self.get_commit_diff_impl(input) {
            Ok(output) => Ok(Self::success_result(output)),
            Err(err) => Err(McpError::internal_error(err.to_string(), None)),
        }
    }
}

impl SpectralCortexMcpServer {
    fn bounded_limit(
        value: Option<usize>,
        default_value: usize,
        maximum: usize,
        name: &str,
    ) -> Result<usize> {
        let value = value.unwrap_or(default_value);
        if !(1..=maximum).contains(&value) {
            anyhow::bail!("{name} must be between 1 and {maximum}")
        }
        Ok(value)
    }

    fn compact_snippet(text: &str, max_chars: usize) -> String {
        let single_line = text.replace('\n', " ").replace("  ", " ");
        if single_line.len() > max_chars {
            format!("{}...", &single_line[..max_chars])
        } else {
            single_line
        }
    }

    fn query_graph_impl(&self, input: QueryGraphInput) -> Result<(String, Vec<Value>)> {
        let smg = self
            .smg
            .read()
            .map_err(|_| anyhow::anyhow!("SMG snapshot lock poisoned"))?
            .clone();
        let top_k = Self::bounded_limit(input.top_k, DEFAULT_TOP_K, 20, "top_k")?;
        let snippet_chars = Self::bounded_limit(
            input.snippet_chars,
            DEFAULT_SNIPPET_CHARS,
            300,
            "snippet_chars",
        )?;
        if snippet_chars < 40 {
            anyhow::bail!("snippet_chars must be between 40 and 300")
        }
        if let Some(min_score) = input.min_score {
            if !min_score.is_finite() || !(0.0..=1.0).contains(&min_score) {
                anyhow::bail!("min_score must be between 0.0 and 1.0")
            }
        }

        let temporal_weight = input.temporal_weight.unwrap_or(0.20);
        if !temporal_weight.is_finite() || !(0.0..=1.0).contains(&temporal_weight) {
            anyhow::bail!("temporal_weight must be between 0.0 and 1.0");
        }
        let temporal_mode = match input.temporal_mode.as_deref().unwrap_or("exponential") {
            "exponential" => spectral_cortex::temporal::TemporalMode::Exponential,
            "linear" | "linearwindow" => spectral_cortex::temporal::TemporalMode::LinearWindow,
            "step" => spectral_cortex::temporal::TemporalMode::Step,
            "buckets" => spectral_cortex::temporal::TemporalMode::Buckets,
            mode => anyhow::bail!("unsupported temporal_mode '{mode}'"),
        };
        let mut temporal = spectral_cortex::temporal::TemporalConfig {
            enabled: !input.no_temporal.unwrap_or(false),
            weight: temporal_weight,
            mode: temporal_mode,
            ..Default::default()
        };
        if let Some(days) = input.temporal_half_life_days {
            if !days.is_finite() || days <= 0.0 {
                anyhow::bail!("temporal_half_life_days must be greater than zero");
            }
            temporal.half_life_seconds = Some((days * 86_400.0).round() as u64);
        }
        let parse_time = |value: Option<&str>| -> Result<Option<u64>> {
            value
                .map(|text| {
                    chrono::DateTime::parse_from_rfc3339(text)
                        .map(|time| time.timestamp().max(0) as u64)
                        .with_context(|| format!("invalid RFC3339 timestamp '{text}'"))
                })
                .transpose()
        };
        let mut time_start = parse_time(input.time_start.as_deref())?;
        let time_end = parse_time(input.time_end.as_deref())?;
        if let Some(now_text) = input.temporal_now.as_deref() {
            temporal.now_seconds = parse_time(Some(now_text))?;
        }
        if let Some(days) = input.time_window_days {
            if !days.is_finite() || days < 0.0 {
                anyhow::bail!("time_window_days must be non-negative");
            }
            if time_start.is_some() {
                anyhow::bail!("time_window_days cannot be combined with time_start");
            }
            let now = match temporal.now_seconds {
                Some(now) => now,
                None => std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_secs(),
            };
            temporal.now_seconds = Some(now);
            let window = (days * 86_400.0).round().min(u64::MAX as f64) as u64;
            time_start = Some(now.saturating_sub(window));
        }
        if time_start
            .zip(time_end)
            .is_some_and(|(start, end)| start > end)
        {
            anyhow::bail!("time_start must be less than or equal to time_end");
        }
        let candidate_k = Self::bounded_limit(
            input.candidate_k,
            top_k.saturating_mul(5).max(top_k),
            100,
            "candidate_k",
        )?;
        let candidates = smg.retrieve_with_scores_config_filtered(
            &input.query,
            candidate_k,
            Some(temporal),
            time_start,
            time_end,
            input.file.as_deref(),
            input.symbol.as_deref(),
            0.3,
        )?;
        let mut hits = Vec::new();
        for (turn_id, score) in candidates {
            if input.min_score.is_some_and(|minimum| score < minimum) {
                continue;
            }
            if let Some(note) = smg
                .notes
                .values()
                .find(|note| note.source_turn_ids.contains(&turn_id))
            {
                hits.push((score, note.note_id));
            }
        }
        hits.truncate(top_k);

        let mut out = String::new();
        let mut structured = Vec::new();
        out.push_str(&format!("# Query Result: `{}`\n", input.query));
        out.push_str(&format!("- SMG: `{}`\n\n", self.smg_path));
        out.push_str(&format!(
            "- indexed_revision: `{}`\n\n",
            smg.source_revision.as_deref().unwrap_or("unknown")
        ));
        let mut groups: Vec<(String, Vec<(f32, u32)>)> = Vec::new();
        for (score, note_id) in hits {
            let note = &smg.notes[&note_id];
            let snippet = Self::compact_snippet(&note.context(), snippet_chars);
            let commit_id = note.source_commit_ids.iter().flatten().next().cloned();
            let commit_label = commit_id.as_deref().unwrap_or("unknown");
            if let Some((_, group)) = groups.iter_mut().find(|(key, _)| key == commit_label) {
                group.push((score, note_id));
            } else {
                groups.push((commit_label.to_owned(), vec![(score, note_id)]));
            }
            structured.push(json!({
                "note_id": note_id, "score": score, "commit_id": commit_id,
                "timestamp": note.source_timestamps.first(), "file_path": note.file_path,
                "symbol_id": note.symbol_id, "snippet": snippet
            }));
        }
        for (commit_label, group) in groups {
            out.push_str(&format!("\n## Commit `{commit_label}`\n"));
            for (score, note_id) in group {
                let note = &smg.notes[&note_id];
                let snippet = Self::compact_snippet(&note.context(), snippet_chars);
                out.push_str(&format!(
                    "- **Score {:.3}** [Note {}] commit={} time={} file={} symbol={}: {}\n",
                    score,
                    note_id,
                    commit_label,
                    note.source_timestamps
                        .first()
                        .map(u64::to_string)
                        .unwrap_or_else(|| "unknown".into()),
                    note.file_path.as_deref().unwrap_or("unknown"),
                    note.symbol_id.as_deref().unwrap_or("unknown"),
                    snippet
                ));

                if let Some(links_k) = input.links_k {
                    let related = smg.get_related_note_links(note_id, Some(links_k));
                    if !related.is_empty() {
                        let compact = {
                            related
                                .into_iter()
                                .map(|(nid, sim)| format!("{}({:.3})", nid, sim))
                                .collect::<Vec<_>>()
                                .join(", ")
                        };
                        out.push_str(&format!("    - related nodes: {}\n", compact));
                    }
                }
            }
        }

        Ok((out, structured))
    }

    fn get_commit_diff_impl(&self, input: CommitDiffInput) -> Result<String> {
        let snapshot = self
            .smg
            .read()
            .map_err(|_| anyhow::anyhow!("SMG snapshot lock poisoned"))?
            .clone();
        let repo_path = snapshot
            .source_repo
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("SMG has no source repository metadata"))?;
        let repo = git2::Repository::open(repo_path)
            .with_context(|| format!("opening source repository {repo_path}"))?;
        let commit = repo.revparse_single(&input.commit_id)?.peel_to_commit()?;
        let tree = commit.tree()?;
        let parent = if commit.parent_count() > 0 {
            Some(commit.parent(0)?.tree()?)
        } else {
            None
        };
        let mut diff = repo.diff_tree_to_tree(parent.as_ref(), Some(&tree), None)?;
        if let Some(file) = input.file.as_deref() {
            diff = repo.diff_tree_to_tree(
                parent.as_ref(),
                Some(&tree),
                Some(git2::DiffOptions::new().pathspec(file)),
            )?;
        }
        let mut output = String::new();
        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            if output.len() < 8000 {
                output.push(line.origin());
                output.push_str(
                    std::str::from_utf8(line.content()).unwrap_or("<non-UTF8 diff content>"),
                );
            }
            true
        })?;
        let truncated = output.len() >= 8000;
        if truncated {
            output.truncate(8000);
            output.push_str("\n[diff excerpt truncated]");
        }
        Ok(format!("# Commit diff `{}`\n\n{}", commit.id(), output))
    }

    fn inspect_note_impl(&self, input: InspectNoteInput) -> Result<String> {
        let smg = self
            .smg
            .read()
            .map_err(|_| anyhow::anyhow!("SMG snapshot lock poisoned"))?
            .clone();
        let links_k = Self::bounded_limit(input.links_k, 10, 25, "links_k")?;
        let snippet_chars = Self::bounded_limit(
            input.snippet_chars,
            DEFAULT_SNIPPET_CHARS,
            300,
            "snippet_chars",
        )?;
        if snippet_chars < 40 {
            anyhow::bail!("snippet_chars must be between 40 and 300")
        }

        let note = smg
            .notes
            .get(&input.note_id)
            .ok_or_else(|| anyhow::anyhow!("note {} not found", input.note_id))?;

        let mut out = String::new();
        out.push_str(&format!("# Note {}\n", note.note_id));
        out.push_str(&format!("- SMG: `{}`\n", self.smg_path));
        out.push_str(&format!(
            "- indexed_revision: `{}`\n",
            smg.source_revision.as_deref().unwrap_or("unknown")
        ));
        out.push_str(&format!(
            "- source_repository: `{}`\n",
            smg.source_repo.as_deref().unwrap_or("unknown")
        ));
        out.push_str(&format!("- symbol_id: {:?}\n", note.symbol_id));
        out.push_str(&format!("- ast_node_type: {:?}\n", note.ast_node_type));
        out.push_str(&format!(
            "- context: {}\n\n",
            Self::compact_snippet(&note.context(), snippet_chars)
        ));

        let related = smg.get_related_note_links(note.note_id, Some(links_k));
        if related.is_empty() {
            out.push_str("No related notes.\n");
            return Ok(out);
        }

        out.push_str("| related_note_id | spectral_similarity | snippet |\n");
        out.push_str("|-----------------|---------------------|---------|\n");
        for (related_id, sim) in related {
            let snippet = smg
                .notes
                .get(&related_id)
                .map(|n| Self::compact_snippet(&n.context(), snippet_chars).replace('|', "\\|"))
                .unwrap_or_else(|| String::from("<missing note payload>"));
            out.push_str(&format!("| {} | {:.4} | {} |\n", related_id, sim, snippet));
        }

        Ok(out)
    }

    fn long_range_links_impl(&self, input: LongRangeLinksInput) -> Result<String> {
        let smg = self
            .smg
            .read()
            .map_err(|_| anyhow::anyhow!("SMG snapshot lock poisoned"))?
            .clone();
        let top_k = Self::bounded_limit(input.top_k, 20, 100, "top_k")?;
        let links = smg.get_long_range_links(Some(top_k));

        let mut out = String::new();
        out.push_str("# Long-Range Links\n");
        out.push_str(&format!("- SMG: `{}`\n", self.smg_path));
        out.push_str(&format!("- links: {}\n\n", links.len()));
        out.push_str("| # | note_a | note_b | spectral_similarity |\n");
        out.push_str("|---|--------|--------|---------------------|\n");

        for (idx, (a, b, sim)) in links.into_iter().enumerate() {
            out.push_str(&format!("| {} | {} | {} | {:.4} |\n", idx + 1, a, b, sim));
        }

        Ok(out)
    }

    pub fn get_structural_hotspots_impl(&self, input: StructuralHotspotsInput) -> Result<String> {
        let smg = self
            .smg
            .read()
            .map_err(|_| anyhow::anyhow!("SMG snapshot lock poisoned"))?
            .clone();
        let mut hotspots: HashMap<String, (usize, String)> = HashMap::new();

        for note in smg.notes.values() {
            if let (Some(sid), Some(typ)) = (&note.symbol_id, &note.ast_node_type) {
                let entry = hotspots.entry(sid.clone()).or_insert((0, typ.clone()));
                entry.0 += note.source_turn_ids.len();
            }
        }

        let mut sorted: Vec<(String, usize, String)> = hotspots
            .into_iter()
            .map(|(sid, (count, typ))| (sid, count, typ))
            .collect();
        sorted.sort_by_key(|entry| std::cmp::Reverse(entry.1));

        let top_k = Self::bounded_limit(input.top_k, 10, 50, "top_k")?;
        let results = sorted.into_iter().take(top_k);

        let mut out = String::new();
        out.push_str("# Structural Hotspots (Top Churn Symbols)\n");
        out.push_str("| Symbol ID | Category | Churn Count |\n");
        out.push_str("|-----------|----------|-------------|\n");
        for (sid, count, typ) in results {
            out.push_str(&format!("| `{}` | {} | {} |\n", sid, typ, count));
        }

        Ok(out)
    }

    pub fn inspect_symbol_history_impl(&self, input: SymbolHistoryInput) -> Result<String> {
        let smg = self
            .smg
            .read()
            .map_err(|_| anyhow::anyhow!("SMG snapshot lock poisoned"))?
            .clone();
        let mut history: Vec<(u64, u32, String)> = Vec::new();

        for note in smg.notes.values() {
            if note.symbol_id.as_deref() == Some(&input.symbol_id) {
                for &ts in &note.source_timestamps {
                    history.push((ts, note.note_id, note.context()));
                }
            }
        }

        history.sort_by_key(|&(ts, _, _)| ts);

        // Reverse so that we show most recent first if we are limiting
        history.reverse();

        let total_found = history.len();
        if let Some(limit) = input.limit {
            if !(1..=1_000).contains(&limit) {
                anyhow::bail!("limit must be between 1 and 1000")
            }
            if history.len() > limit {
                history.truncate(limit);
            }
        }

        let mut out = String::new();
        out.push_str(&format!("# Symbol History: `{}`\n", input.symbol_id));
        if let Some(_limit) = input.limit {
            out.push_str(&format!(
                "- showing last {} of {} entries\n",
                history.len(),
                total_found
            ));
        }

        if history.is_empty() {
            out.push_str("No history found for this symbol.\n");
            return Ok(out);
        }

        for (ts, nid, ctx) in history {
            let date = chrono::DateTime::from_timestamp(ts as i64, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| ts.to_string());
            out.push_str(&format!(
                "- **{}** [Note {}]: {}\n",
                date,
                nid,
                Self::compact_snippet(&ctx, 100)
            ));
        }

        Ok(out)
    }

    fn graph_summary_impl(&self, input: GraphSummaryInput) -> Result<String> {
        let smg = self
            .smg
            .read()
            .map_err(|_| anyhow::anyhow!("SMG snapshot lock poisoned"))?
            .clone();

        let links_count = smg.long_range_links.as_ref().map(|v| v.len()).unwrap_or(0);
        let cluster_labels = smg.cluster_labels.as_ref().map(|v| v.len()).unwrap_or(0);
        let centroid_count = smg.cluster_centroids.as_ref().map(|v| v.len()).unwrap_or(0);
        let topology_symbols: usize = smg
            .topology_snapshots
            .iter()
            .map(|snapshot| snapshot.symbols.len())
            .sum();
        let topology_edges: usize = smg
            .topology_snapshots
            .iter()
            .map(|snapshot| snapshot.edges.len())
            .sum();
        let selected_snapshot = input
            .topology_commit_id
            .as_deref()
            .map(|commit_id| {
                smg.topology_snapshots
                    .iter()
                    .find(|snapshot| snapshot.commit_id == commit_id)
                    .ok_or_else(|| anyhow::anyhow!("topology snapshot '{}' not found", commit_id))
            })
            .transpose()?;

        let mut out = String::new();
        out.push_str("# Graph Summary\n");
        out.push_str(&format!("- SMG: `{}`\n", self.smg_path));
        out.push_str(&format!(
            "- indexed_revision: `{}`\n",
            smg.source_revision.as_deref().unwrap_or("unknown")
        ));
        out.push_str(&format!(
            "- source_repository: `{}`\n",
            smg.source_repo.as_deref().unwrap_or("unknown")
        ));
        out.push_str(&format!("- notes: {}\n", smg.notes.len()));
        out.push_str(&format!("- next_id: {}\n", smg.next_id));
        out.push_str(&format!("- long_range_links: {}\n", links_count));
        out.push_str(&format!("- cluster_labels: {}\n", cluster_labels));
        out.push_str(&format!("- cluster_centroids: {}\n", centroid_count));
        out.push_str(&format!(
            "- topology_snapshots: {}\n",
            smg.topology_snapshots.len()
        ));
        out.push_str(&format!("- topology_symbols: {}\n", topology_symbols));
        out.push_str(&format!("- topology_edges: {}\n", topology_edges));
        if let Some(snapshot) = selected_snapshot {
            out.push_str(&format!("- topology_commit_id: `{}`\n", snapshot.commit_id));
            out.push_str(&format!(
                "- selected_topology_symbols: {}\n",
                snapshot.symbols.len()
            ));
            out.push_str(&format!(
                "- selected_topology_edges: {}\n",
                snapshot.edges.len()
            ));
        }
        out.push_str(&format!(
            "- similarity_matrix_cached: {}\n",
            smg.similarity_matrix.is_some()
        ));
        out.push_str(&format!(
            "- spectral_embeddings_cached: {}\n",
            smg.spectral_embeddings.is_some()
        ));

        Ok(out)
    }
}

pub fn run_mcp_server(smg_path: &Path, watch: bool) -> Result<()> {
    let smg_path = smg_path
        .to_path_buf()
        .canonicalize()
        .unwrap_or_else(|_| smg_path.to_path_buf());
    let smg = load_smg_json(&smg_path)
        .with_context(|| format!("failed to load SMG '{}'", smg_path.display()))?;
    let server = SpectralCortexMcpServer::new(smg_path.display().to_string(), smg);
    let shared_smg = Arc::clone(&server.smg);
    let watched_path = smg_path.clone();
    if watch {
        let stop = Arc::new(AtomicBool::new(false));
        std::thread::Builder::new()
            .name("smg-file-watch".into())
            .spawn(move || {
                watch_smg_file(
                    watched_path,
                    shared_smg,
                    stop,
                    std::time::Duration::from_millis(500),
                )
            })
            .context("starting SMG file watcher")?;
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to build tokio runtime for MCP server")?;

    runtime.block_on(async move {
        let service = server.serve(stdio()).await?;
        service.waiting().await?;
        Ok::<(), anyhow::Error>(())
    })
}

fn watch_smg_file(
    path: PathBuf,
    shared: Arc<RwLock<Arc<SpectralMemoryGraph>>>,
    stop: Arc<AtomicBool>,
    interval: std::time::Duration,
) {
    let mut last_loaded = file_stamp(&path);
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(interval);
        let current = file_stamp(&path);
        if current.is_none() || current == last_loaded {
            continue;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
        let stable = file_stamp(&path);
        if stable != current {
            continue;
        }
        match reload_snapshot(&path, &shared) {
            Ok(()) => last_loaded = stable,
            Err(error) => {
                eprintln!(
                    "level=error event=smg_reload path={} error={error:#}",
                    path.display()
                );
                last_loaded = stable;
            }
        }
    }
}

fn file_stamp(path: &Path) -> Option<(u64, Option<std::time::SystemTime>)> {
    std::fs::metadata(path)
        .ok()
        .map(|metadata| (metadata.len(), metadata.modified().ok()))
}

fn reload_snapshot(path: &Path, shared: &Arc<RwLock<Arc<SpectralMemoryGraph>>>) -> Result<()> {
    let replacement = Arc::new(load_smg_json(path)?);
    let mut guard = shared
        .write()
        .map_err(|_| anyhow::anyhow!("SMG snapshot lock poisoned"))?;
    *guard = replacement;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{reload_snapshot, watch_smg_file, QueryGraphInput, SpectralCortexMcpServer};
    use rmcp::{
        model::{
            CallToolRequest, CallToolRequestParams, ClientCapabilities, ClientJsonRpcMessage,
            ClientRequest, ErrorCode, Implementation, InitializeRequest, InitializeRequestParams,
            ListToolsRequest, ProtocolVersion, RequestId, RequestMetaObject, ServerJsonRpcMessage,
            ServerResult,
        },
        service::serve_directly,
        transport::{IntoTransport, Transport},
        ServiceExt,
    };

    fn query_input() -> QueryGraphInput {
        QueryGraphInput {
            query: "test".into(),
            top_k: None,
            links_k: None,
            snippet_chars: None,
            min_score: None,
            file: None,
            symbol: None,
            temporal_weight: None,
            candidate_k: None,
            no_temporal: None,
            temporal_mode: None,
            temporal_half_life_days: None,
            time_start: None,
            time_end: None,
            time_window_days: None,
            temporal_now: None,
        }
    }

    #[test]
    fn query_rejects_invalid_temporal_settings() {
        let server = SpectralCortexMcpServer::new(
            "fixture.json".into(),
            spectral_cortex::SpectralMemoryGraph::new().unwrap(),
        );
        let mut invalid_weight = query_input();
        invalid_weight.temporal_weight = Some(2.0);
        assert!(server.query_graph_impl(invalid_weight).is_err());

        let mut inverted_range = query_input();
        inverted_range.time_start = Some("2026-02-01T00:00:00Z".into());
        inverted_range.time_end = Some("2026-01-01T00:00:00Z".into());
        assert!(server.query_graph_impl(inverted_range).is_err());

        let mut conflicting_window = query_input();
        conflicting_window.time_start = Some("2026-01-01T00:00:00Z".into());
        conflicting_window.time_window_days = Some(7.0);
        assert!(server.query_graph_impl(conflicting_window).is_err());

        let mut invalid_now = query_input();
        invalid_now.temporal_now = Some("yesterday".into());
        assert!(server.query_graph_impl(invalid_now).is_err());
    }

    #[test]
    fn query_returns_structured_evidence_and_groups_repeated_commits() {
        spectral_cortex::embed::init(1, 0).unwrap();
        let text = "query target";
        let embedding = spectral_cortex::embed::get_embedding(text).unwrap();
        let norm = embedding.iter().map(|v| v * v).sum::<f32>().sqrt();
        let mut graph = spectral_cortex::SpectralMemoryGraph::new().unwrap();
        for (id, turn_id, commit_id, content) in [
            (0, 1, "commit-a", "first related note"),
            (1, 2, "commit-a", "second related note"),
            (2, 3, "commit-b", "third related note"),
        ] {
            graph.notes.insert(
                id,
                spectral_cortex::SMGNote {
                    note_id: id,
                    raw_content: content.into(),
                    embedding: embedding.clone(),
                    norm,
                    source_turn_ids: vec![turn_id],
                    source_commit_ids: vec![Some(commit_id.into())],
                    source_timestamps: vec![1_700_000_000],
                    spectral_coords: None,
                    related_note_links: Vec::new(),
                    symbol_id: Some(format!("symbol_{id}")),
                    ast_node_type: None,
                    file_path: Some(format!("src/{id}.rs")),
                    history_links: Vec::new(),
                },
            );
        }
        graph.next_id = 3;
        let server = SpectralCortexMcpServer::new("fixture.json".into(), graph);
        let mut input = query_input();
        input.query = text.into();
        input.top_k = Some(3);
        let (markdown, results) = server.query_graph_impl(input).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0]["note_id"].as_u64(), Some(0));
        assert_eq!(results[0]["commit_id"].as_str(), Some("commit-a"));
        assert!(results[0]["timestamp"].is_number());
        assert!(markdown.contains("## Commit `commit-a`"));
        assert_eq!(markdown.matches("## Commit `commit-a`").count(), 1);
        assert!(markdown.contains("## Commit `commit-b`"));
        let _ = spectral_cortex::embed::shutdown();
    }

    #[test]
    fn reload_swaps_valid_graph_and_preserves_snapshot_on_invalid_file() {
        let fixture = tempfile::NamedTempFile::new().unwrap();
        let mut original = spectral_cortex::SpectralMemoryGraph::new().unwrap();
        original.source_revision = Some("old".into());
        let shared = std::sync::Arc::new(std::sync::RwLock::new(std::sync::Arc::new(original)));

        let replacement = spectral_cortex::SpectralMemoryGraph::new().unwrap();
        spectral_cortex::save_smg_json(&replacement, fixture.path()).unwrap();
        reload_snapshot(fixture.path(), &shared).unwrap();
        assert_eq!(shared.read().unwrap().source_revision, None);

        std::fs::write(fixture.path(), "not json").unwrap();
        assert!(reload_snapshot(fixture.path(), &shared).is_err());
        assert_eq!(shared.read().unwrap().source_revision, None);
    }

    #[test]
    fn running_watcher_observes_atomic_graph_replacement() {
        let fixture = tempfile::NamedTempFile::new().unwrap();
        let mut initial = spectral_cortex::SpectralMemoryGraph::new().unwrap();
        initial.source_revision = Some("initial".into());
        spectral_cortex::save_smg_json(&initial, fixture.path()).unwrap();
        let shared = std::sync::Arc::new(std::sync::RwLock::new(std::sync::Arc::new(initial)));
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let watcher_path = fixture.path().to_path_buf();
        let watcher_shared = std::sync::Arc::clone(&shared);
        let watcher_stop = std::sync::Arc::clone(&stop);
        let watcher = std::thread::spawn(move || {
            watch_smg_file(
                watcher_path,
                watcher_shared,
                watcher_stop,
                std::time::Duration::from_millis(20),
            )
        });
        std::thread::sleep(std::time::Duration::from_millis(40));
        let mut replacement = spectral_cortex::SpectralMemoryGraph::new().unwrap();
        replacement.source_revision = Some("replacement-revision".into());
        spectral_cortex::save_smg_json(&replacement, fixture.path()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if shared.read().unwrap().source_revision.as_deref() == Some("replacement-revision") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        watcher.join().unwrap();
        assert_eq!(
            shared.read().unwrap().source_revision.as_deref(),
            Some("replacement-revision")
        );
    }
    use serde_json::json;

    #[test]
    fn successful_results_include_markdown_and_structured_content() {
        let result = SpectralCortexMcpServer::success_result("# Graph Summary".into());

        assert_eq!(result.is_error, Some(false));
        assert_eq!(result.content.len(), 1);
        assert_eq!(
            result
                .structured_content
                .and_then(|value| value.get("markdown").cloned())
                .and_then(|value| value.as_str().map(str::to_owned)),
            Some("# Graph Summary".into())
        );
    }

    #[test]
    fn tool_catalog_is_complete_and_schemaful() {
        let tools = SpectralCortexMcpServer::tool_router().list_all();
        let names: Vec<_> = tools.iter().map(|tool| tool.name.as_ref()).collect();

        assert_eq!(
            names,
            vec![
                "get_commit_diff",
                "get_structural_hotspots",
                "graph_summary",
                "inspect_note",
                "inspect_symbol_history",
                "long_range_links",
                "query_graph",
            ]
        );
        assert!(tools.iter().all(|tool| {
            tool.description
                .as_ref()
                .is_some_and(|description| !description.is_empty())
                && tool.input_schema.get("type") == Some(&json!("object"))
                && tool.output_schema.is_some()
        }));
    }

    #[tokio::test]
    async fn modern_wire_round_trip_lists_and_calls_tools() {
        let (server_transport, client_transport) = tokio::io::duplex(16_384);
        let fixture = tempfile::NamedTempFile::new().expect("create SMG fixture");
        std::fs::write(
            fixture.path(),
            r#"{"metadata":{"format_version":"spectral-cortex-v2"},"notes":[],"cluster_labels":null,"cluster_centroids":null,"cluster_centroid_norms":null,"long_range_links":null}"#,
        )
        .expect("write SMG fixture");
        let smg = spectral_cortex::load_smg_json(fixture.path()).expect("load SMG fixture");
        let server = SpectralCortexMcpServer::new(fixture.path().display().to_string(), smg);
        let server = serve_directly::<rmcp::RoleServer, _, _, _, _>(server, server_transport, None);
        let server_task = tokio::spawn(async move {
            server.waiting().await.expect("wait for MCP server");
        });
        let mut client = IntoTransport::<rmcp::RoleClient, _, _>::into_transport(client_transport);

        let mut list = ListToolsRequest {
            method: Default::default(),
            params: None,
            extensions: Default::default(),
        };
        let mut meta = RequestMetaObject::new();
        meta.set_protocol_version(ProtocolVersion::V_2026_07_28);
        meta.set_client_info(Implementation::new("spectral-cortex-test", "1.0.0"));
        meta.set_client_capabilities(ClientCapabilities::default());
        list.extensions.insert(meta);
        client
            .send(ClientJsonRpcMessage::request(
                ClientRequest::ListToolsRequest(list),
                RequestId::Number(1),
            ))
            .await
            .expect("send tools/list");
        let listed = client.receive().await.expect("receive tools/list response");
        let tools = match listed {
            ServerJsonRpcMessage::Response(response) => match response.result {
                ServerResult::ListToolsResult(result) => result.tools,
                other => panic!("expected tools/list result, got {other:?}"),
            },
            other => panic!("expected tools/list response, got {other:?}"),
        };
        assert_eq!(tools.len(), 7);
        assert!(tools.iter().all(|tool| tool.output_schema.is_some()));

        let mut call = CallToolRequest::new(
            CallToolRequestParams::new("graph_summary").with_arguments(serde_json::Map::new()),
        );
        let mut call_meta = RequestMetaObject::new();
        call_meta.set_protocol_version(ProtocolVersion::V_2026_07_28);
        call_meta.set_client_info(Implementation::new("spectral-cortex-test", "1.0.0"));
        call_meta.set_client_capabilities(ClientCapabilities::default());
        call.extensions.insert(call_meta);
        client
            .send(ClientJsonRpcMessage::request(
                ClientRequest::CallToolRequest(call),
                RequestId::Number(2),
            ))
            .await
            .expect("send tools/call");
        let called = client.receive().await.expect("receive tools/call response");
        match called {
            ServerJsonRpcMessage::Response(response) => match response.result {
                ServerResult::CallToolResult(result) => {
                    assert_eq!(result.is_error, Some(false));
                    assert!(result.structured_content.is_some());
                }
                other => panic!("expected tools/call result, got {other:?}"),
            },
            other => panic!("expected tools/call response, got {other:?}"),
        }

        let mut failed_call = CallToolRequest::new(
            CallToolRequestParams::new("inspect_note").with_arguments(
                serde_json::json!({ "note_id": 999 })
                    .as_object()
                    .cloned()
                    .expect("object arguments"),
            ),
        );
        let mut failed_meta = RequestMetaObject::new();
        failed_meta.set_protocol_version(ProtocolVersion::V_2026_07_28);
        failed_meta.set_client_info(Implementation::new("spectral-cortex-test", "1.0.0"));
        failed_meta.set_client_capabilities(ClientCapabilities::default());
        failed_call.extensions.insert(failed_meta);
        client
            .send(ClientJsonRpcMessage::request(
                ClientRequest::CallToolRequest(failed_call),
                RequestId::Number(3),
            ))
            .await
            .expect("send failing tools/call");
        let failed = client.receive().await.expect("receive failing tools/call");
        match failed {
            ServerJsonRpcMessage::Error(error) => {
                assert_eq!(error.error.code, ErrorCode::INTERNAL_ERROR)
            }
            other => panic!("expected protocol error, got {other:?}"),
        }
        drop(client);
        server_task.await.expect("join MCP server");
    }

    #[tokio::test]
    async fn legacy_initialize_negotiates_and_returns_server_info() {
        let (server_transport, client_transport) = tokio::io::duplex(16_384);
        let fixture = tempfile::NamedTempFile::new().expect("create SMG fixture");
        std::fs::write(
            fixture.path(),
            r#"{"metadata":{"format_version":"spectral-cortex-v2"},"notes":[],"cluster_labels":null,"cluster_centroids":null,"cluster_centroid_norms":null,"long_range_links":null}"#,
        )
        .expect("write SMG fixture");
        let smg = spectral_cortex::load_smg_json(fixture.path()).expect("load SMG fixture");
        let server = SpectralCortexMcpServer::new(fixture.path().display().to_string(), smg);
        let server_task = tokio::spawn(async move {
            server
                .serve(server_transport)
                .await
                .expect("start MCP server")
                .waiting()
                .await
                .expect("wait for MCP server");
        });
        let mut client = IntoTransport::<rmcp::RoleClient, _, _>::into_transport(client_transport);
        let initialize = InitializeRequest::new(
            InitializeRequestParams::new(
                ClientCapabilities::default(),
                Implementation::new("spectral-cortex-legacy-test", "1.0.0"),
            )
            .with_protocol_version(ProtocolVersion::V_2025_11_25),
        );
        client
            .send(ClientJsonRpcMessage::request(
                ClientRequest::InitializeRequest(initialize),
                RequestId::Number(1),
            ))
            .await
            .expect("send initialize");
        let response = client.receive().await.expect("receive initialize");
        match response {
            ServerJsonRpcMessage::Response(response) => match response.result {
                ServerResult::InitializeResult(result) => {
                    assert_eq!(result.protocol_version, ProtocolVersion::V_2025_11_25);
                    assert!(result.capabilities.tools.is_some());
                }
                other => panic!("expected initialize result, got {other:?}"),
            },
            other => panic!("expected initialize response, got {other:?}"),
        }
        drop(client);
        server_task.await.expect("join MCP server");
    }
}
