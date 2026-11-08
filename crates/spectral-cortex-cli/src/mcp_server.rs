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
const DEFAULT_LINKS_K: usize = 3;
const DEFAULT_SNIPPET_CHARS: usize = 140;

fn output_schema(schema: Value) -> Arc<JsonObject> {
    let mut schema = schema.as_object().cloned().unwrap_or_default();
    if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        properties.extend(provenance_schema().as_object().cloned().unwrap_or_default());
    }
    if let Some(required) = schema.get_mut("required").and_then(Value::as_array_mut) {
        required.extend([
            json!("source_repository"),
            json!("indexed_revision"),
            json!("current_revision"),
            json!("freshness"),
        ]);
    }
    Arc::new(schema)
}

fn provenance_schema() -> Value {
    json!({"source_repository":{"type":["string","null"]},"indexed_revision":{"type":["string","null"]},"current_revision":{"type":["string","null"]},"freshness":{"enum":["current","behind","ahead","diverged","unknown"]}})
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
struct InvalidToolInput(String);

fn invalid_input(message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(InvalidToolInput(message.into()))
}

fn query_output_schema() -> Arc<JsonObject> {
    output_schema(json!({
        "type": "object",
        "properties": {
            "results": { "type": "array", "items": {
                "type": "object",
                "properties": {
                    "note_id": { "type": "integer" }, "score": { "type": "number" },
                    "commit_id": { "type": ["string", "null"] }, "timestamp": { "type": ["integer", "null"] },
                    "file_path": { "type": ["string", "null"] }, "symbol_id": { "type": ["string", "null"] },
                    "snippet": { "type": "string" },
                    "related_notes": { "type": "array", "items": { "type": "object", "properties": {
                        "note_id": { "type": "integer" }, "similarity": { "type": "number" }
                    }, "required": ["note_id", "similarity"], "additionalProperties": false } }
                },
                "required": ["note_id", "score", "commit_id", "timestamp", "file_path", "symbol_id", "snippet", "related_notes"],
                "additionalProperties": false
            } }
        },
        "required": ["results"], "additionalProperties": false
    }))
}

fn inspect_note_output_schema() -> Arc<JsonObject> {
    output_schema(
        json!({"type":"object","properties":{"note":{"type":"object","properties":{"note_id":{"type":"integer"},"symbol_id":{"type":["string","null"]},"ast_node_type":{"type":["string","null"]},"file_path":{"type":["string","null"]},"context":{"type":"string"}},"required":["note_id","symbol_id","ast_node_type","file_path","context"],"additionalProperties":false},"related_notes":{"type":"array","items":{"type":"object","properties":{"note_id":{"type":"integer"},"similarity":{"type":"number"},"snippet":{"type":"string"}},"required":["note_id","similarity","snippet"],"additionalProperties":false}}},"required":["note","related_notes"],"additionalProperties":false}),
    )
}

fn long_range_output_schema() -> Arc<JsonObject> {
    output_schema(
        json!({"type":"object","properties":{"links":{"type":"array","items":{"type":"object","properties":{"rank":{"type":"integer"},"note_a":{"type":"integer"},"note_b":{"type":"integer"},"similarity":{"type":"number"}},"required":["rank","note_a","note_b","similarity"],"additionalProperties":false}}},"required":["links"],"additionalProperties":false}),
    )
}

fn hotspots_output_schema() -> Arc<JsonObject> {
    output_schema(
        json!({"type":"object","properties":{"hotspots":{"type":"array","items":{"type":"object","properties":{"symbol_id":{"type":"string"},"category":{"type":"string"},"churn_count":{"type":"integer"}},"required":["symbol_id","category","churn_count"],"additionalProperties":false}}},"required":["hotspots"],"additionalProperties":false}),
    )
}

fn history_output_schema() -> Arc<JsonObject> {
    output_schema(
        json!({"type":"object","properties":{"symbol_id":{"type":"string"},"total_entries":{"type":"integer"},"entries":{"type":"array","items":{"type":"object","properties":{"timestamp":{"type":"integer"},"note_id":{"type":"integer"},"context":{"type":"string"}},"required":["timestamp","note_id","context"],"additionalProperties":false}}},"required":["symbol_id","total_entries","entries"],"additionalProperties":false}),
    )
}

fn summary_output_schema() -> Arc<JsonObject> {
    output_schema(
        json!({"type":"object","properties":{"note_count":{"type":"integer"},"next_id":{"type":"integer"},"long_range_link_count":{"type":"integer"},"cluster_label_count":{"type":"integer"},"cluster_centroid_count":{"type":"integer"},"topology_snapshot_count":{"type":"integer"},"topology_symbol_count":{"type":"integer"},"topology_edge_count":{"type":"integer"},"selected_topology":{"type":["object","null"]},"similarity_matrix_cached":{"type":"boolean"},"spectral_embeddings_cached":{"type":"boolean"}},"required":["note_count","next_id","long_range_link_count","cluster_label_count","cluster_centroid_count","topology_snapshot_count","topology_symbol_count","topology_edge_count","selected_topology","similarity_matrix_cached","spectral_embeddings_cached"],"additionalProperties":false}),
    )
}

fn diff_output_schema() -> Arc<JsonObject> {
    output_schema(
        json!({"type":"object","properties":{"commit_id":{"type":"string"},"diff":{"type":"string"},"truncated":{"type":"boolean"}},"required":["commit_id","diff","truncated"],"additionalProperties":false}),
    )
}

/// Input for querying the graph with a text query.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct QueryGraphInput {
    #[schemars(description = "Text query to run against the graph")]
    pub query: String,
    #[schemars(
        description = "Number of rows to return (default: 5; range: 1–20)",
        range(min = 1, max = 20)
    )]
    pub top_k: Option<usize>,
    #[schemars(
        description = "Number of related notes per hit (default: 3; range: 1–25)",
        range(min = 1, max = 25)
    )]
    pub links_k: Option<usize>,
    #[schemars(
        description = "Maximum characters per snippet (default: 140; range: 40–300)",
        range(min = 40, max = 300)
    )]
    pub snippet_chars: Option<usize>,
    #[schemars(
        description = "Optional minimum score threshold from 0 to 1",
        range(min = 0.0, max = 1.0)
    )]
    pub min_score: Option<f32>,
    #[schemars(description = "Optional file path substring filter")]
    pub file: Option<String>,
    #[schemars(description = "Optional symbol ID substring filter")]
    pub symbol: Option<String>,
    #[schemars(
        description = "Temporal reranking weight, from 0 to 1 (default: 0.2)",
        range(min = 0.0, max = 1.0)
    )]
    pub temporal_weight: Option<f32>,
    #[schemars(
        description = "Candidate count before final ranking (default: max(top_k * 5, top_k); range: 1–100)",
        range(min = 1, max = 100)
    )]
    pub candidate_k: Option<usize>,
    #[schemars(description = "Disable temporal reranking")]
    pub no_temporal: Option<bool>,
    #[schemars(
        description = "Temporal mode: exponential, linear, linearwindow, step, or buckets (default: exponential)"
    )]
    pub temporal_mode: Option<TemporalModeInput>,
    #[schemars(description = "Temporal decay half-life in days; must be positive")]
    pub temporal_half_life_days: Option<f32>,
    #[schemars(description = "Inclusive RFC3339 lower timestamp bound")]
    pub time_start: Option<String>,
    #[schemars(description = "Inclusive RFC3339 upper timestamp bound")]
    pub time_end: Option<String>,
    #[schemars(
        description = "Relative time window in days; non-negative and cannot be combined with time_start",
        range(min = 0.0)
    )]
    pub time_window_days: Option<f64>,
    #[schemars(description = "RFC3339 reference time for temporal ranking")]
    pub temporal_now: Option<String>,
}

/// Supported temporal reranking modes for graph queries.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TemporalModeInput {
    Exponential,
    Linear,
    #[serde(rename = "linearwindow")]
    LinearWindow,
    Step,
    Buckets,
}

/// Input for inspecting one note and its related notes.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InspectNoteInput {
    #[schemars(description = "Note id to inspect")]
    pub note_id: u32,
    #[schemars(
        description = "Number of related notes to include (default: 10; range: 1–25)",
        range(min = 1, max = 25)
    )]
    pub links_k: Option<usize>,
    #[schemars(
        description = "Maximum characters per snippet (default: 140; range: 40–300)",
        range(min = 40, max = 300)
    )]
    pub snippet_chars: Option<usize>,
}

/// Input for listing long-range links.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LongRangeLinksInput {
    #[schemars(
        description = "Number of long-range links to include (default: 20; range: 1–100)",
        range(min = 1, max = 100)
    )]
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
    #[schemars(
        description = "Number of hotspots to return (default: 10; range: 1–50)",
        range(min = 1, max = 50)
    )]
    pub top_k: Option<usize>,
}

/// Input for inspecting symbol history.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SymbolHistoryInput {
    #[schemars(description = "Stable symbol_id to inspect")]
    pub symbol_id: String,
    #[schemars(
        description = "Maximum number of history entries to return (default: all; range: 1–1000)",
        range(min = 1, max = 1000)
    )]
    pub limit: Option<usize>,
}

/// Parameters for retrieving a bounded commit diff excerpt.
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
                "Spectral Cortex MCP Server for inspecting a preloaded Spectral Memory Graph. Results are structured data. Start with graph_summary, use query_graph to find evidence, inspect_note to expand a note, then get_commit_diff for source changes. Use get_structural_hotspots and inspect_symbol_history for structural or timeline questions. All tools are read-only.",
            )
    }
}

#[rmcp::tool_router]
impl SpectralCortexMcpServer {
    fn success_result(&self, mut data: Value) -> CallToolResult {
        if let Some(object) = data.as_object_mut() {
            let snapshot = self.smg.read().ok().map(|guard| guard.clone());
            let provenance = snapshot
                .as_deref()
                .map(Self::provenance)
                .unwrap_or((None, None, None, "unknown"));
            object.insert("source_repository".into(), json!(provenance.0));
            object.insert("indexed_revision".into(), json!(provenance.1));
            object.insert("current_revision".into(), json!(provenance.2));
            object.insert("freshness".into(), json!(provenance.3));
        }
        let mut result = CallToolResult::structured(data);
        result.content.clear();
        result
    }

    fn provenance(
        smg: &SpectralMemoryGraph,
    ) -> (Option<String>, Option<String>, Option<String>, &'static str) {
        let source_repository = smg.source_repo.clone();
        let indexed_revision = smg.source_revision.clone();
        let current = source_repository.as_deref().and_then(|path| {
            let repo = git2::Repository::open(path).ok()?;
            let head = repo.head().ok()?.peel_to_commit().ok()?.id();
            Some((repo, head))
        });
        let current_revision = current.as_ref().map(|(_, head)| head.to_string());
        let freshness = match (&indexed_revision, current) {
            (Some(indexed), Some((repo, head))) => match git2::Oid::from_str(indexed) {
                Ok(indexed) if indexed == head => "current",
                Ok(indexed) => match (
                    repo.graph_descendant_of(head, indexed),
                    repo.graph_descendant_of(indexed, head),
                ) {
                    (Ok(true), _) => "behind",
                    (_, Ok(true)) => "ahead",
                    (Ok(false), Ok(false)) => "diverged",
                    _ => "unknown",
                },
                Err(_) => "unknown",
            },
            _ => "unknown",
        };
        (
            source_repository,
            indexed_revision,
            current_revision,
            freshness,
        )
    }

    fn tool_error(err: anyhow::Error) -> McpError {
        let message = err.to_string();
        if err.downcast_ref::<InvalidToolInput>().is_some() {
            McpError::invalid_params(message, None)
        } else {
            McpError::internal_error(message, None)
        }
    }

    fn snapshot(&self) -> Result<Self, McpError> {
        let graph = self
            .smg
            .read()
            .map_err(|_| McpError::internal_error("SMG snapshot lock poisoned", None))?
            .clone();
        Ok(Self {
            tool_router: self.tool_router.clone(),
            smg_path: self.smg_path.clone(),
            smg: Arc::new(RwLock::new(graph)),
        })
    }

    /// Construct a new server instance.
    pub fn new(smg_path: String, smg: SpectralMemoryGraph) -> Self {
        Self {
            tool_router: Self::tool_router(),
            smg_path,
            smg: Arc::new(RwLock::new(Arc::new(smg))),
        }
    }

    /// Query the graph and return ranked evidence with related note identifiers.
    #[rmcp::tool(
        description = "Run a semantic query against the graph and return ranked note evidence as structured data",
        annotations(title = "Query graph", read_only_hint = true, destructive_hint = false, idempotent_hint = true),
        output_schema = query_output_schema()
    )]
    fn query_graph(
        &self,
        Parameters(input): Parameters<QueryGraphInput>,
    ) -> Result<CallToolResult, McpError> {
        let server = self.snapshot()?;
        match server.query_graph_impl(input) {
            Ok(results) => Ok(server.success_result(json!({"results": results}))),
            Err(err) => Err(Self::tool_error(err)),
        }
    }

    /// Inspect a single note and show top related notes with similarity.
    #[rmcp::tool(
        description = "Inspect one note by ID and return its context and related-note report as structured data",
        annotations(title = "Inspect note", read_only_hint = true, destructive_hint = false, idempotent_hint = true),
        output_schema = inspect_note_output_schema()
    )]
    fn inspect_note(
        &self,
        Parameters(input): Parameters<InspectNoteInput>,
    ) -> Result<CallToolResult, McpError> {
        let server = self.snapshot()?;
        match server.inspect_note_impl(input) {
            Ok(output) => Ok(server.success_result(output)),
            Err(err) => Err(Self::tool_error(err)),
        }
    }

    /// List long-range links from the graph.
    #[rmcp::tool(
        description = "List long-range spectral links as structured report data",
        annotations(title = "Long-range links", read_only_hint = true, destructive_hint = false, idempotent_hint = true),
        output_schema = long_range_output_schema()
    )]
    fn long_range_links(
        &self,
        Parameters(input): Parameters<LongRangeLinksInput>,
    ) -> Result<CallToolResult, McpError> {
        let server = self.snapshot()?;
        match server.long_range_links_impl(input) {
            Ok(output) => Ok(server.success_result(output)),
            Err(err) => Err(Self::tool_error(err)),
        }
    }

    /// Return a small summary of graph size and available structures.
    #[rmcp::tool(
        description = "Return graph summary metadata as structured report data",
        annotations(title = "Graph summary", read_only_hint = true, destructive_hint = false, idempotent_hint = true),
        output_schema = summary_output_schema()
    )]
    fn graph_summary(
        &self,
        Parameters(input): Parameters<GraphSummaryInput>,
    ) -> Result<CallToolResult, McpError> {
        let server = self.snapshot()?;
        match server.graph_summary_impl(input) {
            Ok(output) => Ok(server.success_result(output)),
            Err(err) => Err(Self::tool_error(err)),
        }
    }

    /// Retrieve the most churn-heavy structural hotspots.
    #[rmcp::tool(
        description = "Return the highest-churn AST symbols as structured report data",
        annotations(title = "Structural hotspots", read_only_hint = true, destructive_hint = false, idempotent_hint = true),
        output_schema = hotspots_output_schema()
    )]
    fn get_structural_hotspots(
        &self,
        Parameters(input): Parameters<StructuralHotspotsInput>,
    ) -> Result<CallToolResult, McpError> {
        let server = self.snapshot()?;
        match server.get_structural_hotspots_impl(input) {
            Ok(output) => Ok(server.success_result(output)),
            Err(err) => Err(Self::tool_error(err)),
        }
    }

    /// Retrieve chronological change history for a single symbol.
    #[rmcp::tool(
        description = "Retrieve a symbol's chronological history as structured report data",
        annotations(title = "Symbol history", read_only_hint = true, destructive_hint = false, idempotent_hint = true),
        output_schema = history_output_schema()
    )]
    fn inspect_symbol_history(
        &self,
        Parameters(input): Parameters<SymbolHistoryInput>,
    ) -> Result<CallToolResult, McpError> {
        let server = self.snapshot()?;
        match server.inspect_symbol_history_impl(input) {
            Ok(output) => Ok(server.success_result(output)),
            Err(err) => Err(Self::tool_error(err)),
        }
    }

    #[rmcp::tool(description = "Retrieve a bounded commit diff excerpt from the source Git repository", annotations(title = "Commit diff", read_only_hint = true, destructive_hint = false, idempotent_hint = true), output_schema = diff_output_schema())]
    fn get_commit_diff(
        &self,
        Parameters(input): Parameters<CommitDiffInput>,
    ) -> Result<CallToolResult, McpError> {
        let server = self.snapshot()?;
        match server.get_commit_diff_impl(input) {
            Ok(output) => Ok(server.success_result(output)),
            Err(err) => Err(Self::tool_error(err)),
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
            return Err(invalid_input(format!(
                "{name} must be between 1 and {maximum}"
            )));
        }
        Ok(value)
    }

    fn compact_snippet(text: &str, max_chars: usize) -> String {
        let single_line = text.replace('\n', " ").replace("  ", " ");
        let mut chars = single_line.chars();
        let snippet: String = chars.by_ref().take(max_chars).collect();
        if chars.next().is_some() {
            format!("{snippet}...")
        } else {
            single_line
        }
    }

    fn query_graph_impl(&self, input: QueryGraphInput) -> Result<Vec<Value>> {
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
            return Err(invalid_input("snippet_chars must be between 40 and 300"));
        }
        if let Some(min_score) = input.min_score {
            if !min_score.is_finite() || !(0.0..=1.0).contains(&min_score) {
                return Err(invalid_input("min_score must be between 0.0 and 1.0"));
            }
        }

        let temporal_weight = input.temporal_weight.unwrap_or(0.20);
        if !temporal_weight.is_finite() || !(0.0..=1.0).contains(&temporal_weight) {
            return Err(invalid_input("temporal_weight must be between 0.0 and 1.0"));
        }
        let temporal_mode = match input
            .temporal_mode
            .unwrap_or(TemporalModeInput::Exponential)
        {
            TemporalModeInput::Exponential => spectral_cortex::temporal::TemporalMode::Exponential,
            TemporalModeInput::Linear | TemporalModeInput::LinearWindow => {
                spectral_cortex::temporal::TemporalMode::LinearWindow
            }
            TemporalModeInput::Step => spectral_cortex::temporal::TemporalMode::Step,
            TemporalModeInput::Buckets => spectral_cortex::temporal::TemporalMode::Buckets,
        };
        let mut temporal = spectral_cortex::temporal::TemporalConfig {
            enabled: !input.no_temporal.unwrap_or(false),
            weight: temporal_weight,
            mode: temporal_mode,
            ..Default::default()
        };
        if let Some(days) = input.temporal_half_life_days {
            if !days.is_finite() || days <= 0.0 {
                return Err(invalid_input(
                    "temporal_half_life_days must be greater than zero",
                ));
            }
            temporal.half_life_seconds = Some((days * 86_400.0).round() as u64);
        }
        let parse_time = |value: Option<&str>| -> Result<Option<u64>> {
            value
                .map(|text| {
                    chrono::DateTime::parse_from_rfc3339(text)
                        .map(|time| time.timestamp().max(0) as u64)
                        .map_err(|_| invalid_input(format!("invalid RFC3339 timestamp '{text}'")))
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
                return Err(invalid_input("time_window_days must be non-negative"));
            }
            if time_start.is_some() {
                return Err(invalid_input(
                    "time_window_days cannot be combined with time_start",
                ));
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
            return Err(invalid_input(
                "time_start must be less than or equal to time_end",
            ));
        }
        let links_k = Self::bounded_limit(input.links_k, DEFAULT_LINKS_K, 25, "links_k")?;
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

        let mut structured = Vec::new();
        for (score, note_id) in hits {
            let note = &smg.notes[&note_id];
            let snippet = Self::compact_snippet(&note.context(), snippet_chars);
            let commit_id = note.source_commit_ids.iter().flatten().next().cloned();
            let related_notes = smg.get_related_note_links(note_id, Some(links_k)).into_iter().map(|(related_note_id, similarity)| json!({"note_id":related_note_id,"similarity":similarity})).collect::<Vec<_>>();
            structured.push(json!({
                "note_id": note_id, "score": score, "commit_id": commit_id,
                "timestamp": note.source_timestamps.first(), "file_path": note.file_path,
                "symbol_id": note.symbol_id, "snippet": snippet, "related_notes": related_notes
            }));
        }
        Ok(structured)
    }

    fn get_commit_diff_impl(&self, input: CommitDiffInput) -> Result<Value> {
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
        let commit = repo
            .revparse_single(&input.commit_id)
            .map_err(|_| invalid_input(format!("commit {} not found", input.commit_id)))?
            .peel_to_commit()
            .map_err(|_| invalid_input(format!("commit {} is not a commit", input.commit_id)))?;
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
        Ok(json!({"commit_id": commit.id().to_string(), "diff": output, "truncated": truncated}))
    }

    fn inspect_note_impl(&self, input: InspectNoteInput) -> Result<Value> {
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
            return Err(invalid_input("snippet_chars must be between 40 and 300"));
        }

        let note = smg
            .notes
            .get(&input.note_id)
            .ok_or_else(|| invalid_input(format!("note {} not found", input.note_id)))?;
        let related_notes = smg
            .get_related_note_links(note.note_id, Some(links_k))
            .into_iter()
            .map(|(related_id, similarity)| {
                let snippet = smg
                    .notes
                    .get(&related_id)
                    .map(|related| Self::compact_snippet(&related.context(), snippet_chars))
                    .unwrap_or_else(|| "<missing note payload>".into());
                json!({"note_id": related_id, "similarity": similarity, "snippet": snippet})
            })
            .collect::<Vec<_>>();
        Ok(
            json!({"note":{"note_id":note.note_id,"symbol_id":note.symbol_id,"ast_node_type":note.ast_node_type,"file_path":note.file_path,"context":Self::compact_snippet(&note.context(), snippet_chars)},"related_notes":related_notes}),
        )
    }

    fn long_range_links_impl(&self, input: LongRangeLinksInput) -> Result<Value> {
        let smg = self
            .smg
            .read()
            .map_err(|_| anyhow::anyhow!("SMG snapshot lock poisoned"))?
            .clone();
        let top_k = Self::bounded_limit(input.top_k, 20, 100, "top_k")?;
        let links = smg.get_long_range_links(Some(top_k));

        Ok(
            json!({"links": links.into_iter().enumerate().map(|(idx,(a,b,similarity))| json!({"rank":idx+1,"note_a":a,"note_b":b,"similarity":similarity})).collect::<Vec<_>>()}),
        )
    }

    /// Return structural symbols ranked by observed churn.
    pub fn get_structural_hotspots_impl(&self, input: StructuralHotspotsInput) -> Result<Value> {
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
        Ok(
            json!({"hotspots": sorted.into_iter().take(top_k).map(|(symbol_id,churn_count,category)| json!({"symbol_id":symbol_id,"category":category,"churn_count":churn_count})).collect::<Vec<_>>()}),
        )
    }

    /// Return chronological entries for a stable symbol identifier.
    pub fn inspect_symbol_history_impl(&self, input: SymbolHistoryInput) -> Result<Value> {
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
                return Err(invalid_input("limit must be between 1 and 1000"));
            }
            if history.len() > limit {
                history.truncate(limit);
            }
        }

        Ok(
            json!({"symbol_id":input.symbol_id,"total_entries":total_found,"entries":history.into_iter().map(|(timestamp,note_id,context)| json!({"timestamp":timestamp,"note_id":note_id,"context":Self::compact_snippet(&context,100)})).collect::<Vec<_>>()}),
        )
    }

    fn graph_summary_impl(&self, input: GraphSummaryInput) -> Result<Value> {
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
                    .ok_or_else(|| {
                        invalid_input(format!("topology snapshot '{}' not found", commit_id))
                    })
            })
            .transpose()?;

        let selected_topology = selected_snapshot.map(|snapshot| json!({"commit_id":snapshot.commit_id,"symbol_count":snapshot.symbols.len(),"edge_count":snapshot.edges.len()}));
        Ok(
            json!({"note_count":smg.notes.len(),"next_id":smg.next_id,"long_range_link_count":links_count,"cluster_label_count":cluster_labels,"cluster_centroid_count":centroid_count,"topology_snapshot_count":smg.topology_snapshots.len(),"topology_symbol_count":topology_symbols,"topology_edge_count":topology_edges,"selected_topology":selected_topology,"similarity_matrix_cached":smg.similarity_matrix.is_some(),"spectral_embeddings_cached":smg.spectral_embeddings.is_some()}),
        )
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
    use super::{
        reload_snapshot, watch_smg_file, GraphSummaryInput, LongRangeLinksInput, QueryGraphInput,
        SpectralCortexMcpServer, StructuralHotspotsInput, SymbolHistoryInput,
    };
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
    fn query_returns_structured_evidence_and_related_notes() {
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
        let results = server.query_graph_impl(input).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0]["note_id"].as_u64(), Some(0));
        assert_eq!(results[0]["commit_id"].as_str(), Some("commit-a"));
        assert!(results[0]["timestamp"].is_number());
        assert!(results[0]["related_notes"].is_array());
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
    use serde_json::Value;

    #[test]
    fn successful_results_include_typed_content_without_text_blocks() {
        let graph = spectral_cortex::SpectralMemoryGraph::new().unwrap();
        let server = SpectralCortexMcpServer::new("fixture.json".into(), graph);
        let result = server.success_result(json!({"note_count": 0}));

        assert_eq!(result.is_error, Some(false));
        assert!(result.content.is_empty());
        assert_eq!(
            result
                .structured_content
                .and_then(|value| value.get("note_count").cloned())
                .and_then(|value| value.as_u64()),
            Some(0)
        );
    }

    #[test]
    fn provenance_classifies_current_behind_ahead_diverged_and_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        let signature = git2::Signature::now("test", "test@example.com").unwrap();
        let tree_id = repo.treebuilder(None).unwrap().write().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let initial = repo
            .commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
            .unwrap();
        let initial_commit = repo.find_commit(initial).unwrap();
        let second = repo
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                "second",
                &tree,
                &[&initial_commit],
            )
            .unwrap();
        let initial_commit = repo.find_commit(initial).unwrap();
        let divergent = repo
            .commit(
                None,
                &signature,
                &signature,
                "divergent",
                &tree,
                &[&initial_commit],
            )
            .unwrap();
        let mut graph = spectral_cortex::SpectralMemoryGraph::new().unwrap();
        graph.source_repo = Some(dir.path().display().to_string());
        let mut freshness_for = |revision: Option<git2::Oid>| {
            graph.source_revision = revision.map(|oid| oid.to_string());
            SpectralCortexMcpServer::provenance(&graph)
        };

        let (_, indexed, current, freshness) = freshness_for(None);
        assert_eq!(indexed, None);
        assert_eq!(current.as_deref(), Some(second.to_string().as_str()));
        assert_eq!(freshness, "unknown");
        assert_eq!(freshness_for(Some(second)).3, "current");
        assert_eq!(freshness_for(Some(initial)).3, "behind");

        repo.set_head_detached(initial).unwrap();
        let (_, _, current, freshness) = freshness_for(Some(second));
        assert_eq!(current.as_deref(), Some(initial.to_string().as_str()));
        assert_eq!(freshness, "ahead");

        repo.set_head_detached(divergent).unwrap();
        let (_, _, current, freshness) = freshness_for(Some(second));
        assert_eq!(current.as_deref(), Some(divergent.to_string().as_str()));
        assert_eq!(freshness, "diverged");
    }

    #[test]
    fn typed_tool_implementations_return_named_fields() {
        let mut graph = spectral_cortex::SpectralMemoryGraph::new().unwrap();
        graph.next_id = 7;
        let server = SpectralCortexMcpServer::new("fixture.json".into(), graph);
        assert_eq!(
            server
                .graph_summary_impl(GraphSummaryInput {
                    topology_commit_id: None
                })
                .unwrap()["next_id"],
            7
        );
        assert!(server
            .long_range_links_impl(LongRangeLinksInput { top_k: None })
            .unwrap()["links"]
            .is_array());
        assert!(server
            .get_structural_hotspots_impl(StructuralHotspotsInput { top_k: None })
            .unwrap()["hotspots"]
            .is_array());
        assert_eq!(
            server
                .inspect_symbol_history_impl(SymbolHistoryInput {
                    symbol_id: "missing".into(),
                    limit: None
                })
                .unwrap()["symbol_id"],
            "missing"
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
        assert!(tools.iter().all(|tool| {
            let annotations = tool.annotations.as_ref().expect("tool annotations");
            annotations.read_only_hint == Some(true)
                && annotations.destructive_hint == Some(false)
                && annotations.idempotent_hint == Some(true)
        }));
        for tool in &tools {
            let properties = tool
                .output_schema
                .as_ref()
                .unwrap()
                .get("properties")
                .unwrap();
            assert!(properties.get("report").is_none());
            assert!(properties.get("source_repository").is_some());
            assert!(properties.get("indexed_revision").is_some());
            assert!(properties.get("current_revision").is_some());
            assert!(properties.get("freshness").is_some());
            let expected_field = match tool.name.as_ref() {
                "query_graph" => "results",
                "inspect_note" => "note",
                "long_range_links" => "links",
                "graph_summary" => "note_count",
                "get_structural_hotspots" => "hotspots",
                "inspect_symbol_history" => "entries",
                "get_commit_diff" => "diff",
                _ => unreachable!(),
            };
            assert!(
                properties.get(expected_field).is_some(),
                "{} schema missing {expected_field}",
                tool.name
            );
        }
        let query = tools
            .iter()
            .find(|tool| tool.name == "query_graph")
            .unwrap();
        assert_eq!(query.input_schema["properties"]["top_k"]["maximum"], 20);
        assert_eq!(
            query.input_schema["properties"]["snippet_chars"]["minimum"],
            40
        );
        let query_schema = Value::Object(query.input_schema.as_ref().clone()).to_string();
        assert!(query_schema.contains("linearwindow"));
        assert!(query_schema.contains("buckets"));
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
                assert_eq!(error.error.code, ErrorCode::INVALID_PARAMS)
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
