// Rust guideline compliant 2026-02-15

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool_handler,
    transport::stdio,
    ServerHandler, ServiceExt,
};
use serde::Deserialize;
use spectral_cortex::{load_smg_json, SpectralMemoryGraph};

const DEFAULT_TOP_K: usize = 5;
// const DEFAULT_LINKS_K: usize = 3;
const DEFAULT_SNIPPET_CHARS: usize = 140;

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
pub struct GraphSummaryInput {}

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
}

/// MCP server that provides compact tools for SMG query and inspection.
#[derive(Clone)]
pub struct SpectralCortexMcpServer {
    pub tool_router: ToolRouter<Self>,
    pub smg_path: String,
    pub smg: Arc<SpectralMemoryGraph>,
}

#[tool_handler]
impl ServerHandler for SpectralCortexMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Spectral Cortex MCP Server: compact markdown tools for querying a preloaded SMG file. Prefer small top_k values to keep responses token efficient.".into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

#[rmcp::tool_router]
impl SpectralCortexMcpServer {
    /// Construct a new server instance.
    pub fn new(smg_path: String, smg: SpectralMemoryGraph) -> Self {
        Self {
            tool_router: Self::tool_router(),
            smg_path,
            smg: Arc::new(smg),
        }
    }

    /// Query the graph and return a compact markdown table.
    #[rmcp::tool(
        description = "Run semantic query against an SMG and return token-efficient markdown results"
    )]
    fn query_graph(&self, Parameters(input): Parameters<QueryGraphInput>) -> String {
        match self.query_graph_impl(input) {
            Ok(output) => output,
            Err(err) => format!("Error: {err}"),
        }
    }

    /// Inspect a single note and show top related notes with similarity.
    #[rmcp::tool(
        description = "Inspect one note by id and return compact markdown with related notes"
    )]
    fn inspect_note(&self, Parameters(input): Parameters<InspectNoteInput>) -> String {
        match self.inspect_note_impl(input) {
            Ok(output) => output,
            Err(err) => format!("Error: {err}"),
        }
    }

    /// List long-range links from the graph.
    #[rmcp::tool(description = "List long-range spectral links as compact markdown")]
    fn long_range_links(&self, Parameters(input): Parameters<LongRangeLinksInput>) -> String {
        match self.long_range_links_impl(input) {
            Ok(output) => output,
            Err(err) => format!("Error: {err}"),
        }
    }

    /// Return a small summary of graph size and available structures.
    #[rmcp::tool(description = "Return compact summary metadata for an SMG")]
    fn graph_summary(&self, Parameters(input): Parameters<GraphSummaryInput>) -> String {
        match self.graph_summary_impl(input) {
            Ok(output) => output,
            Err(err) => format!("Error: {err}"),
        }
    }

    /// Retrieve the most churn-heavy structural hotspots.
    #[rmcp::tool(description = "Report top churn-heavy AST symbols")]
    fn get_structural_hotspots(&self, Parameters(input): Parameters<StructuralHotspotsInput>) -> String {
        match self.get_structural_hotspots_impl(input) {
            Ok(output) => output,
            Err(err) => format!("Error: {err}"),
        }
    }

    /// Retrieve chronological change history for a single symbol.
    #[rmcp::tool(description = "Retrieve timeline of a symbol's evolution")]
    fn inspect_symbol_history(&self, Parameters(input): Parameters<SymbolHistoryInput>) -> String {
        match self.inspect_symbol_history_impl(input) {
            Ok(output) => output,
            Err(err) => format!("Error: {err}"),
        }
    }
}

impl SpectralCortexMcpServer {
    fn clamp_top_k(top_k: Option<usize>, default_k: usize, max_k: usize) -> usize {
        top_k.unwrap_or(default_k).max(1).min(max_k)
    }

    fn compact_snippet(text: &str, max_chars: usize) -> String {
        let single_line = text.replace('\n', " ").replace("  ", " ");
        if single_line.len() > max_chars {
            format!("{}...", &single_line[..max_chars])
        } else {
            single_line
        }
    }

    fn query_graph_impl(&self, input: QueryGraphInput) -> Result<String> {
        let smg = &self.smg;
        let top_k = Self::clamp_top_k(input.top_k, DEFAULT_TOP_K, 20);
        let snippet_chars = input
            .snippet_chars
            .unwrap_or(DEFAULT_SNIPPET_CHARS)
            .clamp(40, 300);

        let hits = smg.search(&input.query, top_k, input.min_score)?;
        
        let mut out = String::new();
        out.push_str(&format!("# Query Result: `{}`\n", input.query));
        out.push_str(&format!("- SMG: `{}`\n\n", self.smg_path));

        for (score, note_id) in hits {
            let note = &smg.notes[&note_id];
            let snippet = Self::compact_snippet(&note.context(), snippet_chars);
            out.push_str(&format!("- **Score {:.3}** [Note {}]: {}\n", score, note_id, snippet));
            
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

        Ok(out)
    }

    fn inspect_note_impl(&self, input: InspectNoteInput) -> Result<String> {
        let smg = &self.smg;
        let links_k = Self::clamp_top_k(input.links_k, 10, 25);
        let snippet_chars = input
            .snippet_chars
            .unwrap_or(DEFAULT_SNIPPET_CHARS)
            .clamp(40, 300);

        let note = smg
            .notes
            .get(&input.note_id)
            .ok_or_else(|| anyhow::anyhow!("note {} not found", input.note_id))?;

        let mut out = String::new();
        out.push_str(&format!("# Note {}\n", note.note_id));
        out.push_str(&format!("- SMG: `{}`\n", self.smg_path));
        out.push_str(&format!("- symbol_id: {:?}\n", note.symbol_id));
        out.push_str(&format!("- ast_node_type: {:?}\n", note.ast_node_type));
        out.push_str(&format!("- context: {}\n\n", Self::compact_snippet(&note.context(), snippet_chars)));

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
        let smg = &self.smg;
        let top_k = Self::clamp_top_k(input.top_k, 20, 100);
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

    fn get_structural_hotspots_impl(&self, input: StructuralHotspotsInput) -> Result<String> {
        let smg = &self.smg;
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
        sorted.sort_by(|a, b| b.1.cmp(&a.1));

        let top_k = input.top_k.unwrap_or(10).min(50);
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

    fn inspect_symbol_history_impl(&self, input: SymbolHistoryInput) -> Result<String> {
        let smg = &self.smg;
        let mut history: Vec<(u64, u32, String)> = Vec::new();

        for note in smg.notes.values() {
            if note.symbol_id.as_deref() == Some(&input.symbol_id) {
                for &ts in &note.source_timestamps {
                    history.push((ts, note.note_id, note.context()));
                }
            }
        }

        history.sort_by_key(|&(ts, _, _)| ts);

        let mut out = String::new();
        out.push_str(&format!("# Symbol History: `{}`\n", input.symbol_id));
        if history.is_empty() {
            out.push_str("No history found for this symbol.\n");
            return Ok(out);
        }

        for (ts, nid, ctx) in history {
            let date = chrono::DateTime::from_timestamp(ts as i64, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| ts.to_string());
            out.push_str(&format!("- **{}** [Note {}]: {}\n", date, nid, Self::compact_snippet(&ctx, 100)));
        }

        Ok(out)
    }

    fn graph_summary_impl(&self, _input: GraphSummaryInput) -> Result<String> {
        let smg = &self.smg;

        let links_count = smg.long_range_links.as_ref().map(|v| v.len()).unwrap_or(0);
        let cluster_labels = smg.cluster_labels.as_ref().map(|v| v.len()).unwrap_or(0);
        let centroid_count = smg.cluster_centroids.as_ref().map(|v| v.len()).unwrap_or(0);

        let mut out = String::new();
        out.push_str("# Graph Summary\n");
        out.push_str(&format!("- SMG: `{}`\n", self.smg_path));
        out.push_str(&format!("- notes: {}\n", smg.notes.len()));
        out.push_str(&format!("- next_id: {}\n", smg.next_id));
        out.push_str(&format!("- long_range_links: {}\n", links_count));
        out.push_str(&format!("- cluster_labels: {}\n", cluster_labels));
        out.push_str(&format!("- cluster_centroids: {}\n", centroid_count));
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

pub fn run_mcp_server(smg_path: &Path) -> Result<()> {
    let smg_path = smg_path
        .to_path_buf()
        .canonicalize()
        .unwrap_or_else(|_| smg_path.to_path_buf());
    let smg = load_smg_json(&smg_path)
        .with_context(|| format!("failed to load SMG '{}'", smg_path.display()))?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to build tokio runtime for MCP server")?;

    runtime.block_on(async move {
        let service = SpectralCortexMcpServer::new(smg_path.display().to_string(), smg)
            .serve(stdio())
            .await?;
        service.waiting().await?;
        Ok::<(), anyhow::Error>(())
    })
}
