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
use std::collections::HashSet;
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
    pub history_links: Vec<u32>,
    pub topology_links: Vec<crate::model::smg_note::TopologyLink>,
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
    #[serde(default)]
    pub topology_snapshots: Vec<crate::model::smg_note::TopologySnapshot>,
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

        if let Some(config) = &smg.last_build_config {
            metadata.insert(
                "num_spectral_dims".to_string(),
                config.num_spectral_dims.to_string(),
            );
            metadata.insert("min_clusters".to_string(), config.min_clusters.to_string());
            metadata.insert("max_clusters".to_string(), config.max_clusters.to_string());
        }
        if let Some(config) = &smg.embedding_config {
            metadata.insert("embedding_backend".into(), config.backend.clone());
            if let Some(model) = &config.model_name {
                metadata.insert("embedding_model_name".into(), model.clone());
            }
            if let Some(normalize) = config.normalize {
                metadata.insert("embedding_normalize".into(), normalize.to_string());
            }
            if let Some(batch_size) = config.batch_size {
                metadata.insert("embedding_batch_size".into(), batch_size.to_string());
            }
        }

        Self {
            metadata,
            notes,
            cluster_labels,
            cluster_centroids,
            cluster_centroid_norms: smg.cluster_centroid_norms.clone(),
            long_range_links: smg.long_range_links.clone(),
            topology_snapshots: smg.topology_snapshots.clone(),
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
            history_links: n.history_links.clone(),
            topology_links: n.topology_links.clone(),
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

    validate_serial_smg_data(&serial)?;

    // Create a fresh graph (this also initialises logging/embedder per existing API).
    let mut smg = SpectralMemoryGraph::new()?;
    if let Some(backend) = serial.metadata.get("embedding_backend") {
        smg.embedding_config = Some(embed::EmbeddingConfig {
            backend: backend.clone(),
            model_name: serial.metadata.get("embedding_model_name").cloned(),
            normalize: serial
                .metadata
                .get("embedding_normalize")
                .and_then(|v| v.parse().ok()),
            batch_size: serial
                .metadata
                .get("embedding_batch_size")
                .and_then(|v| v.parse().ok()),
        });
    }

    // Restore last build config if present in metadata
    let mut config = SpectralBuildConfig::default();
    let mut has_config = false;
    if let Some(val) = serial.metadata.get("num_spectral_dims") {
        if let Ok(n) = val.parse::<usize>() {
            config.num_spectral_dims = n;
            has_config = true;
        }
    }
    if let Some(val) = serial.metadata.get("min_clusters") {
        if let Ok(n) = val.parse::<usize>() {
            config.min_clusters = n;
            has_config = true;
        }
    }
    if let Some(val) = serial.metadata.get("max_clusters") {
        if let Ok(n) = val.parse::<usize>() {
            config.max_clusters = n;
            has_config = true;
        }
    }
    if has_config {
        smg.last_build_config = Some(config);
    }

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
            history_links: sn.history_links,
            topology_links: sn.topology_links,
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
    smg.topology_snapshots = serial.topology_snapshots;

    Ok(smg)
}

fn validate_serial_smg_data(serial: &SerializableSMG) -> Result<()> {
    let mut note_ids = HashSet::with_capacity(serial.notes.len());
    let mut embedding_dim = None;
    let mut previous_note_id = None;
    for note in &serial.notes {
        if !note_ids.insert(note.note_id) {
            anyhow::bail!("duplicate note ID {}", note.note_id);
        }
        if note.note_id == u32::MAX {
            anyhow::bail!("note ID {} cannot be loaded", u32::MAX);
        }
        if previous_note_id.is_some_and(|previous| note.note_id <= previous) {
            anyhow::bail!("persisted notes must be sorted by note ID");
        }
        previous_note_id = Some(note.note_id);
        if note.source_turn_ids.len() != note.source_commit_ids.len()
            || note.source_turn_ids.len() != note.source_timestamps.len()
        {
            anyhow::bail!("source vectors are not parallel for note {}", note.note_id);
        }
        if note.embedding.is_empty() || note.embedding.iter().any(|value| !value.is_finite()) {
            anyhow::bail!("embedding for note {} is empty or non-finite", note.note_id);
        }
        match embedding_dim {
            Some(expected) if expected != note.embedding.len() => {
                anyhow::bail!("embedding dimension mismatch at note {}", note.note_id)
            }
            None => embedding_dim = Some(note.embedding.len()),
            _ => {}
        }
        if !note.norm.is_finite() || note.norm < 0.0 {
            anyhow::bail!("invalid embedding norm for note {}", note.note_id);
        }
        let computed_norm = note
            .embedding
            .iter()
            .map(|value| value * value)
            .sum::<f32>()
            .sqrt();
        if (note.norm - computed_norm).abs() > 1e-4 * computed_norm.max(1.0) {
            anyhow::bail!("embedding norm mismatch for note {}", note.note_id);
        }
        for &(target, score) in &note.related_note_links {
            if !note_ids.contains(&target) {
                // Endpoint validation is repeated after all IDs are collected below.
                continue;
            }
            if !score.is_finite() {
                anyhow::bail!("non-finite related-link score on note {}", note.note_id);
            }
        }
    }

    for note in &serial.notes {
        for &target in &note.history_links {
            if !note_ids.contains(&target) {
                anyhow::bail!(
                    "history link from {} targets unknown note {}",
                    note.note_id,
                    target
                );
            }
        }
        let mut topology_keys = HashSet::new();
        for link in &note.topology_links {
            if !note_ids.contains(&link.target_note_id) {
                anyhow::bail!(
                    "topology link from {} targets unknown note {}",
                    note.note_id,
                    link.target_note_id
                );
            }
            if link.target_note_id == note.note_id {
                anyhow::bail!(
                    "topology link on note {} cannot target itself",
                    note.note_id
                );
            }
            if !topology_keys.insert((link.target_note_id, &link.relation)) {
                anyhow::bail!("duplicate topology link on note {}", note.note_id);
            }
        }
        for &(target, score) in &note.related_note_links {
            if !note_ids.contains(&target) {
                anyhow::bail!(
                    "related link from {} targets unknown note {}",
                    note.note_id,
                    target
                );
            }
            if !score.is_finite() {
                anyhow::bail!("non-finite related-link score on note {}", note.note_id);
            }
        }
    }

    if let Some(labels) = &serial.cluster_labels {
        if labels.len() != serial.notes.len() {
            anyhow::bail!("cluster-label length does not match note count");
        }
    }
    if let Some(centroids) = &serial.cluster_centroids {
        let dimension = embedding_dim.ok_or_else(|| anyhow::anyhow!("centroids require notes"))?;
        for (cluster, centroid) in centroids {
            if centroid.len() != dimension || centroid.iter().any(|value| !value.is_finite()) {
                anyhow::bail!("invalid centroid dimension or value for cluster {cluster}");
            }
        }
    }
    if let Some(norms) = &serial.cluster_centroid_norms {
        let centroids = serial
            .cluster_centroids
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("centroid norms require centroids"))?;
        for (cluster, norm) in norms {
            let centroid = centroids
                .get(cluster)
                .ok_or_else(|| anyhow::anyhow!("centroid norm has unknown cluster {cluster}"))?;
            let computed = centroid
                .iter()
                .map(|value| value * value)
                .sum::<f32>()
                .sqrt();
            if !norm.is_finite() || (*norm - computed).abs() > 1e-4 * computed.max(1.0) {
                anyhow::bail!("centroid norm mismatch for cluster {cluster}");
            }
        }
    }
    if let Some(links) = &serial.long_range_links {
        for &(source, target, score) in links {
            if !note_ids.contains(&source) || !note_ids.contains(&target) {
                anyhow::bail!("long-range link targets an unknown note");
            }
            if !score.is_finite() {
                anyhow::bail!("long-range link score is non-finite");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod persistence_tests {
    use super::{validate_serial_smg_data, SerializableNote, SerializableSMG};
    use std::collections::HashMap;

    fn note(note_id: u32) -> SerializableNote {
        SerializableNote {
            note_id,
            raw_content: String::new(),
            embedding: vec![1.0, 0.0],
            norm: 1.0,
            source_turn_ids: vec![1],
            source_commit_ids: vec![None],
            source_timestamps: vec![1],
            related_note_links: vec![],
            symbol_id: None,
            ast_node_type: None,
            file_path: None,
            history_links: vec![],
            topology_links: vec![],
        }
    }

    fn serial(notes: Vec<SerializableNote>) -> SerializableSMG {
        SerializableSMG {
            metadata: HashMap::from([(
                String::from("format_version"),
                String::from("spectral-cortex-v1"),
            )]),
            notes,
            cluster_labels: None,
            cluster_centroids: None,
            cluster_centroid_norms: None,
            long_range_links: None,
            topology_snapshots: vec![],
        }
    }

    #[test]
    fn rejects_duplicate_ids_and_misaligned_sources() {
        assert!(validate_serial_smg_data(&serial(vec![note(1), note(1)])).is_err());
        let mut invalid = note(1);
        invalid.source_timestamps.clear();
        assert!(validate_serial_smg_data(&serial(vec![invalid])).is_err());
        assert!(validate_serial_smg_data(&serial(vec![note(u32::MAX)])).is_err());
        assert!(validate_serial_smg_data(&serial(vec![note(2), note(1)])).is_err());
    }

    #[test]
    fn rejects_inconsistent_labels_links_and_centroids() {
        let mut invalid = serial(vec![note(1)]);
        invalid.cluster_labels = Some(vec![]);
        assert!(validate_serial_smg_data(&invalid).is_err());

        let mut invalid = serial(vec![note(1)]);
        invalid.cluster_centroids = Some(HashMap::from([(0, vec![1.0])]));
        assert!(validate_serial_smg_data(&invalid).is_err());

        let mut invalid = serial(vec![note(1)]);
        invalid.long_range_links = Some(vec![(1, 99, 1.0)]);
        assert!(validate_serial_smg_data(&invalid).is_err());
    }
}
