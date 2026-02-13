// Rust guideline compliant 2026-02-13

use std::path::Path;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    schemars,
};
use serde::Deserialize;
use spectral_cortex::{load_smg_json, SpectralMemoryGraph};

const DEFAULT_TOP_K: usize = 5;
const DEFAULT_LINKS_K: usize = 3;
const DEFAULT_SNIPPET_CHARS: usize = 140;

/// Input for querying the graph with a text query.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct QueryGraphInput {
    #[schemars(description = "Path to an SMG JSON file")]
    pub smg_path: String,
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
    #[schemars(description = "Path to an SMG JSON file")]
    pub smg_path: String,
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
    #[schemars(description = "Path to an SMG JSON file")]
    pub smg_path: String,
    #[schemars(description = "Number of long-range links to include (default: 20)")]
    pub top_k: Option<usize>,
}

/// Input for quick graph summary.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GraphSummaryInput {
    #[schemars(description = "Path to an SMG JSON file")]
    pub smg_path: String,
}

/// MCP server that provides compact tools for SMG query and inspection.
#[derive(Debug, Clone)]
pub struct SpectralCortexMcpServer {
    pub tool_router: ToolRouter<Self>,
}

#[rmcp::tool_router]
impl SpectralCortexMcpServer {
    /// Construct a new server instance.
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
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
}

impl SpectralCortexMcpServer {
    fn load_graph(path: &str) -> anyhow::Result<SpectralMemoryGraph> {
        let p = Path::new(path);
        let smg = load_smg_json(p)
            .map_err(|e| anyhow::anyhow!("failed to load SMG '{}': {e}", p.display()))?;
        Ok(smg)
    }

    fn clamp_top_k(top_k: Option<usize>, default_k: usize, max_k: usize) -> usize {
        top_k.unwrap_or(default_k).max(1).min(max_k)
    }

    fn compact_snippet(text: &str, max_chars: usize) -> String {
        let single_line = text.replace('\n', " ").replace("  ", " ");
        if single_line.chars().count() <= max_chars {
            single_line
        } else {
            let mut out = String::new();
            for ch in single_line.chars().take(max_chars) {
                out.push(ch);
            }
            out.push_str("...");
            out
        }
    }

    fn find_note_by_turn_id(smg: &SpectralMemoryGraph, turn_id: u64) -> Option<u32> {
        smg.notes.iter().find_map(|(nid, note)| {
            if note.source_turn_ids.contains(&turn_id) {
                Some(*nid)
            } else {
                None
            }
        })
    }

    fn query_graph_impl(&self, input: QueryGraphInput) -> anyhow::Result<String> {
        let smg = Self::load_graph(&input.smg_path)?;
        let top_k = Self::clamp_top_k(input.top_k, DEFAULT_TOP_K, 50);
        let links_k = Self::clamp_top_k(input.links_k, DEFAULT_LINKS_K, 10);
        let snippet_chars = input
            .snippet_chars
            .unwrap_or(DEFAULT_SNIPPET_CHARS)
            .max(40)
            .min(300);

        let mut scored = smg
            .retrieve_with_scores(&input.query, top_k.saturating_mul(4))
            .map_err(|e| anyhow::anyhow!("query failed: {e}"))?;

        if let Some(min_score) = input.min_score {
            scored.retain(|(_, score)| *score >= min_score);
        }

        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        scored.truncate(top_k);

        let mut out = String::new();
        out.push_str(&format!("# Query: `{}`\n", input.query));
        out.push_str(&format!("- SMG: `{}`\n", input.smg_path));
        out.push_str(&format!("- hits: {}\n\n", scored.len()));

        out.push_str("| # | turn_id | note_id | score | snippet |\n");
        out.push_str("|---|---------|---------|-------|---------|\n");

        for (idx, (turn_id, score)) in scored.iter().enumerate() {
            if let Some(note_id) = Self::find_note_by_turn_id(&smg, *turn_id) {
                if let Some(note) = smg.notes.get(&note_id) {
                    let snip =
                        Self::compact_snippet(&note.context, snippet_chars).replace('|', "\\|");
                    out.push_str(&format!(
                        "| {} | {} | {} | {:.4} | {} |\n",
                        idx + 1,
                        turn_id,
                        note_id,
                        score,
                        snip
                    ));
                }
            }
        }

        if !scored.is_empty() {
            out.push_str("\n## Related Notes\n");
            for (turn_id, _) in scored {
                if let Some(note_id) = Self::find_note_by_turn_id(&smg, turn_id) {
                    let related = smg.get_related_note_links(note_id, Some(links_k));
                    let compact = if related.is_empty() {
                        String::from("none")
                    } else {
                        related
                            .into_iter()
                            .map(|(nid, sim)| format!("{}({:.3})", nid, sim))
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    out.push_str(&format!("- note {}: {}\n", note_id, compact));
                }
            }
        }

        Ok(out)
    }

    fn inspect_note_impl(&self, input: InspectNoteInput) -> anyhow::Result<String> {
        let smg = Self::load_graph(&input.smg_path)?;
        let links_k = Self::clamp_top_k(input.links_k, 10, 25);
        let snippet_chars = input
            .snippet_chars
            .unwrap_or(DEFAULT_SNIPPET_CHARS)
            .max(40)
            .min(300);

        let note = smg
            .notes
            .get(&input.note_id)
            .ok_or_else(|| anyhow::anyhow!("note {} not found", input.note_id))?;

        let mut out = String::new();
        out.push_str(&format!("# Note {}\n", note.note_id));
        out.push_str(&format!("- SMG: `{}`\n", input.smg_path));
        out.push_str(&format!("- source_turn_ids: {:?}\n", note.source_turn_ids));
        out.push_str(&format!(
            "- source_commit_ids: {:?}\n",
            note.source_commit_ids
        ));
        out.push_str(&format!(
            "- context: {}\n\n",
            Self::compact_snippet(&note.context, snippet_chars)
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
                .map(|n| Self::compact_snippet(&n.context, snippet_chars).replace('|', "\\|"))
                .unwrap_or_else(|| String::from("<missing note payload>"));
            out.push_str(&format!("| {} | {:.4} | {} |\n", related_id, sim, snippet));
        }

        Ok(out)
    }

    fn long_range_links_impl(&self, input: LongRangeLinksInput) -> anyhow::Result<String> {
        let smg = Self::load_graph(&input.smg_path)?;
        let top_k = Self::clamp_top_k(input.top_k, 20, 100);
        let links = smg.get_long_range_links(Some(top_k));

        let mut out = String::new();
        out.push_str("# Long-Range Links\n");
        out.push_str(&format!("- SMG: `{}`\n", input.smg_path));
        out.push_str(&format!("- links: {}\n\n", links.len()));
        out.push_str("| # | note_a | note_b | spectral_similarity |\n");
        out.push_str("|---|--------|--------|---------------------|\n");

        for (idx, (a, b, sim)) in links.into_iter().enumerate() {
            out.push_str(&format!("| {} | {} | {} | {:.4} |\n", idx + 1, a, b, sim));
        }

        Ok(out)
    }

    fn graph_summary_impl(&self, input: GraphSummaryInput) -> anyhow::Result<String> {
        let smg = Self::load_graph(&input.smg_path)?;

        let links_count = smg.long_range_links.as_ref().map(|v| v.len()).unwrap_or(0);
        let cluster_labels = smg.cluster_labels.as_ref().map(|v| v.len()).unwrap_or(0);
        let centroid_count = smg.cluster_centroids.as_ref().map(|v| v.len()).unwrap_or(0);

        let mut out = String::new();
        out.push_str("# Graph Summary\n");
        out.push_str(&format!("- SMG: `{}`\n", input.smg_path));
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
