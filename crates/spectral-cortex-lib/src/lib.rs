//! Library entry point for the Spectral Memory Graph implementation.
//!
//! This file re-exports the core types and provides convenient helpers to
//! persist and restore a `SpectralMemoryGraph` to/from JSON. The helpers use
//! internal, well-typed serialisable representations to avoid modifying the
//! model source files directly while providing stable interchange formats.
//
// Public modules
pub mod embed;
pub mod graph;
pub mod lanzcos;
pub mod model;
pub mod temporal;
pub mod utils;

// Re‑export primary types for ergonomic use.
pub use graph::{SpectralBuildConfig, SpectralMemoryGraph};
pub use model::{conversation_turn::ConversationTurn, smg_note::SMGNote};

use anyhow::Result;
use ndarray::Array1;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;
// use std::time::Instant;

/// Serializable representation of a single note stored in the SMG.
///
/// This struct is intentionally independent of the internal `SMGNote` type so
/// we can evolve persistence without touching the internal model files.
///
/// # Fields
/// - `note_id`: internal numeric id
/// - `raw_content`: original raw text
/// - `context`: cleaned/shortened context
/// - `embedding`: the stored embedding vector (Vec<f32>)
/// - `source_turn_ids`: list of source turn ids (u64)
/// - `spectral_coords`: optional spectral coordinates (if present)
/// - `related_note_links`: optional adjacency list with similarity score
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SerializableNote {
    pub note_id: u32,
    pub raw_content: String,
    pub embedding: Vec<f32>,
    /// Precomputed L2 norm of the embedding for fast cosine similarity computation.
    pub norm: f32,
    pub source_turn_ids: Vec<u64>,
    /// List of source commit ids (hex strings) parallel to `source_turn_ids`.
    /// Each entry inside the inner Vec is itself optional to represent synthetic/non-git turns.
    pub source_commit_ids: Vec<Option<String>>,
    /// List of source timestamps (unix epoch seconds) parallel to `source_turn_ids`.
    pub source_timestamps: Vec<u64>,
    /// Adjacency list with similarity scores.
    /// Tuple shape: `(related_note_id, spectral_similarity)`.
    pub related_note_links: Vec<(u32, f32)>,
    /// Stable AST symbol identifier (e.g., "fn:calculate_tax").
    pub symbol_id: Option<String>,
    /// Type of the AST node (e.g., "API_DEFINITION", "IMPLEMENTATION").
    pub ast_node_type: Option<String>,
    pub file_path: Option<String>,
    /// Structural link neighbors (note_ids).
    pub structural_links: Vec<u32>,
}

/// Top-level serialisable SMG container.
///
/// `similarity_matrix` and `spectral_embeddings` are omitted because they are
/// large and can be recomputed from embeddings; we persist cluster information
/// and centroids to speed up query-time boosts.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SerializableSMG {
    pub metadata: HashMap<String, String>,
    pub notes: Vec<SerializableNote>,
    pub cluster_labels: Option<Vec<usize>>,
    pub cluster_centroids: Option<HashMap<usize, Vec<f32>>>,
    pub cluster_centroid_norms: Option<HashMap<usize, f32>>,
    pub long_range_links: Option<Vec<(u32, u32, f32)>>,
}

impl SerializableSMG {
    pub fn from_smg(smg: &SpectralMemoryGraph) -> Self {
        // Prepare serialisable notes in stable order (sort by note_id).
        let mut notes: Vec<SerializableNote> =
            smg.notes.values().map(SerializableNote::from).collect();
        notes.sort_by_key(|n| n.note_id);

        // Convert cluster labels (ndarray::Array1) to Vec<usize> if present.
        let cluster_labels = smg.cluster_labels.as_ref().map(|arr| arr.to_vec());

        // Clone centroids map if present.
        let cluster_centroids = smg.cluster_centroids.clone();

        // Basic metadata for versioning and provenance.
        let mut metadata = HashMap::new();
        metadata.insert(
            "format_version".to_string(),
            "spectral-cortex-v1".to_string(),
        );

        Self {
            metadata,
            notes,
            cluster_labels,
            cluster_centroids,
            cluster_centroid_norms: smg.cluster_centroid_norms.clone(),
            long_range_links: smg.long_range_links.clone(),
        }
    }
}

impl From<&SMGNote> for SerializableNote {
    fn from(n: &SMGNote) -> Self {
        // Convert internal SMGNote into a serialisable form.
        SerializableNote {
            note_id: n.note_id,
            raw_content: n.raw_content.clone(),
            embedding: n.embedding.clone(),
            norm: n.norm,
            source_turn_ids: n.source_turn_ids.clone(),
            source_commit_ids: n.source_commit_ids.clone(),
            source_timestamps: n.source_timestamps.clone(),
            related_note_links: n.related_note_links.clone(),
            symbol_id: n.symbol_id.clone(),
            ast_node_type: n.ast_node_type.clone(),
            file_path: n.file_path.clone(),
            structural_links: n.structural_links.clone(),
        }
    }
}

/// Save the provided `SpectralMemoryGraph` to a JSON file.
pub fn save_smg_json(smg: &SpectralMemoryGraph, path: &Path) -> Result<()> {
    let serial = SerializableSMG::from_smg(smg);
    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    serde_json::to_writer(writer, &serial)?;
    Ok(())
}

/// Load an SMG from a JSON file previously written with `save_smg_json`.
pub fn load_smg_json(path: &Path) -> Result<SpectralMemoryGraph> {
    let file = BufReader::new(File::open(path)?);
    let serial: SerializableSMG = serde_json::from_reader(file)?;
    validate_serial_smg(serial)
}

fn validate_serial_smg(serial: SerializableSMG) -> Result<SpectralMemoryGraph> {
    let format_version = serial
        .metadata
        .get("format_version")
        .map(String::as_str)
        .unwrap_or("unknown");
    if format_version != "spectral-cortex-v1" {
        return Err(anyhow::anyhow!(
            "unsupported SMG format_version '{}'; expected 'spectral-cortex-v1'",
            format_version
        ));
    }

    // Create a fresh graph (this also initialises logging/embedder per existing API).
    let mut smg = SpectralMemoryGraph::new()?;

    // Insert notes back into the graph.
    for sn in serial.notes.into_iter() {
        // Extract the id first to avoid using `note` after it has been moved into the map.
        let nid = sn.note_id;
        let note = SMGNote {
            note_id: nid,
            raw_content: sn.raw_content,
            embedding: sn.embedding,
            norm: sn.norm,
            source_turn_ids: sn.source_turn_ids,
            source_commit_ids: sn.source_commit_ids,
            source_timestamps: sn.source_timestamps,
            spectral_coords: None,
            related_note_links: sn.related_note_links,
            symbol_id: sn.symbol_id,
            ast_node_type: sn.ast_node_type,
            file_path: sn.file_path,
            structural_links: sn.structural_links,
        };
        smg.notes.insert(nid, note);
        // Keep next_id ahead of the highest assembled note id.
        if smg.next_id <= nid {
            smg.next_id = nid + 1;
        }
    }

    // Restore cluster labels if present.
    smg.cluster_labels = serial.cluster_labels.map(Array1::from);

    // Restore centroids if present.
    smg.cluster_centroids = serial.cluster_centroids;

    // Restore centroid norms if present.
    smg.cluster_centroid_norms = serial.cluster_centroid_norms;

    // similarity_matrix and spectral_embeddings are intentionally left None to avoid
    // storing very large matrices; callers should call `build_spectral_structure`
    // if they need a fully-built SMG.
    smg.similarity_matrix = None;
    smg.spectral_embeddings = None;

    // Restore long-range links if present.
    smg.long_range_links = serial.long_range_links;

    Ok(smg)
}
