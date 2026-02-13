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
pub use graph::SpectralMemoryGraph;
pub use model::{conversation_turn::ConversationTurn, smg_note::SMGNote};

use anyhow::Result;
use ndarray::Array1;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
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
/// - `related_note_ids`: adjacency list of related notes by id
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SerializableNote {
    pub note_id: u32,
    pub raw_content: String,
    pub context: String,
    pub embedding: Vec<f32>,
    /// Precomputed L2 norm of the embedding for fast cosine similarity computation.
    pub norm: f32,
    pub source_turn_ids: Vec<u64>,
    /// Optional list of source commit ids (hex strings) parallel to `source_turn_ids`.
    /// Each entry inside the inner Vec is itself optional to represent synthetic/non-git turns.
    pub source_commit_ids: Option<Vec<Option<String>>>,
    /// Optional list of source timestamps (unix epoch seconds) parallel to `source_turn_ids`.
    /// Persisted as `None` when empty to keep the JSON compact.
    pub source_timestamps: Option<Vec<u64>>,
    pub related_note_ids: Vec<u32>,
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

impl From<&SMGNote> for SerializableNote {
    fn from(n: &SMGNote) -> Self {
        // Convert internal SMGNote into a serialisable form. If commit ids or timestamps are present
        // we persist them as `Some(Vec<...>)`, otherwise we leave the fields as `None`
        // to keep the persisted format compact and compatible with older files.
        SerializableNote {
            note_id: n.note_id,
            raw_content: n.raw_content.clone(),
            context: n.context.clone(),
            embedding: n.embedding.clone(),
            norm: n.norm,
            source_turn_ids: n.source_turn_ids.clone(),
            // Persist commit ids if any are present.
            source_commit_ids: {
                if n.source_commit_ids.is_empty() {
                    None
                } else {
                    Some(n.source_commit_ids.clone())
                }
            },
            // Persist source timestamps if any are present.
            source_timestamps: {
                if n.source_timestamps.is_empty() {
                    None
                } else {
                    Some(n.source_timestamps.clone())
                }
            },
            related_note_ids: n.related_note_ids.clone(),
        }
    }
}

/// Save the provided `SpectralMemoryGraph` to a JSON file.
///
/// # Arguments
///
/// * `smg` - reference to the graph to persist
/// * `path` - filesystem path to write JSON to
///
/// # Returns
///
/// `Ok(())` on success, or an `anyhow::Error` on failure.
///
/// # Notes
///
/// - The function writes a pretty-printed JSON file. Large projects can compress
///   the output externally (e.g. `.gz`) if desired.
/// - `similarity_matrix` and large recomputable matrices are intentionally not
///   persisted to keep the file size reasonable; a subsequent `build_spectral_structure`
///   call will recompute them if needed.
pub fn save_smg_json(smg: &SpectralMemoryGraph, path: &Path) -> Result<()> {
    // Prepare serialisable notes in stable order (sort by note_id).
    let mut notes: Vec<SerializableNote> = smg.notes.values().map(SerializableNote::from).collect();
    notes.sort_by_key(|n| n.note_id);

    // Convert cluster labels (ndarray::Array1) to Vec<usize> if present.
    let cluster_labels = smg.cluster_labels.as_ref().map(|arr| arr.to_vec());

    // Clone centroids map if present.
    let cluster_centroids = smg.cluster_centroids.clone();

    // Basic metadata for versioning and provenance.
    let mut metadata = HashMap::new();
    metadata.insert(
        "format_version".to_string(),
        "spectral-cortex-1".to_string(),
    );

    let serial = SerializableSMG {
        metadata,
        notes,
        cluster_labels,
        cluster_centroids,
        cluster_centroid_norms: smg.cluster_centroid_norms.clone(),
        long_range_links: smg.long_range_links.clone(),
    };

    let file = File::create(path)?;
    serde_json::to_writer(file, &serial)?;
    Ok(())
}

/// Load an SMG from a JSON file previously written with `save_smg_json`.
///
/// # Arguments
///
/// * `path` - path to the JSON file
///
/// # Returns
///
/// A newly constructed `SpectralMemoryGraph` populated from the persisted data.
///
/// # Behavior
///
/// - The embedder is initialised via `SpectralMemoryGraph::new()` (to follow the
///   original construction semantics).
/// - Recomputable matrices (similarity, spectral embeddings) are left as `None`.
///   Callers can invoke `build_spectral_structure()` after `load` to rebuild them.
pub fn load_smg_json(path: &Path) -> Result<SpectralMemoryGraph> {
    let file = BufReader::new(File::open(path)?);
    let serial: SerializableSMG = serde_json::from_reader(file)?;

    // Create a fresh graph (this also initialises logging/embedder per existing API).
    let mut smg = SpectralMemoryGraph::new()?;

    // Insert notes back into the graph.
    // let notes_start = Instant::now();
    for sn in serial.notes.into_iter() {
        // Extract the id first to avoid using `note` after it has been moved into the map.
        let nid = sn.note_id;
        let note = SMGNote {
            note_id: nid,
            raw_content: sn.raw_content,
            context: sn.context,
            embedding: sn.embedding,
            norm: sn.norm,
            source_turn_ids: sn.source_turn_ids,
            // Restore persisted commit ids if present; otherwise use an empty vector.
            source_commit_ids: sn.source_commit_ids.unwrap_or_default(),
            // Restore persisted source timestamps if present; otherwise use an empty vector.
            source_timestamps: sn.source_timestamps.unwrap_or_default(),
            // We intentionally do NOT restore per-note spectral coordinates from the
            // persisted file. The global spectral matrices are recomputable and we
            // prefer to keep the JSON compact and avoid confusion by omitting this field.
            spectral_coords: None,
            related_note_ids: sn.related_note_ids,
        };
        smg.notes.insert(nid, note);
        // Keep next_id ahead of the highest assembled note id.
        if smg.next_id <= nid {
            smg.next_id = nid + 1;
        }
    }
    // eprintln!("  Inserted {} notes in {:?}", smg.notes.len(), notes_start.elapsed());

    // Restore cluster labels if present.
    // let cluster_start = Instant::now();
    smg.cluster_labels = serial.cluster_labels.map(Array1::from);
    // eprintln!("  Restored cluster labels in {:?}", cluster_start.elapsed());

    // Restore centroids if present.
    // let centroid_start = Instant::now();
    smg.cluster_centroids = serial.cluster_centroids;
    // eprintln!("  Restored {} centroids in {:?}", smg.cluster_centroids.as_ref().map(|c| c.len()).unwrap_or(0), centroid_start.elapsed());

    // Restore centroid norms if present.
    // let norm_start = Instant::now();
    smg.cluster_centroid_norms = serial.cluster_centroid_norms;
    // eprintln!("  Restored centroid norms in {:?}", norm_start.elapsed());

    // similarity_matrix and spectral_embeddings are intentionally left None to avoid
    // storing very large matrices; callers should call `build_spectral_structure`
    // if they need a fully-built SMG.
    smg.similarity_matrix = None;
    smg.spectral_embeddings = None;

    // Restore long-range links if present.
    smg.long_range_links = serial.long_range_links;

    Ok(smg)
}
