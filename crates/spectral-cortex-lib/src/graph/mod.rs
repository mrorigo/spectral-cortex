//! Minimal SMG graph implementation that delegates spectral computations to
//! `graph/spectral.rs` utilities. This file wires the numerical building blocks
//! into the `SpectralMemoryGraph::build_spectral_structure` pipeline.
//
//! The implementation keeps the external behaviour and persisted shape intact
//! while replacing the inline numerical code with calls to the tested helpers.
//
// Rust guideline compliant 2026-02-11

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use anyhow::{Context, Result};
use ndarray::{Array1, Array2};

use crate::embed;
use crate::model::{conversation_turn::ConversationTurn, smg_note::SMGNote};
use crate::utils::logging;

/// Progress callback type for long-running operations.
/// The callback receives a message describing the current step and a progress fraction (0.0..1.0).
pub type ProgressCallback = Arc<dyn Fn(String, f32) + Send + Sync>;

/// Submodules
pub mod spectral;

/// Spectral Memory Graph: in-memory notes + cached structures used for
/// retrieval and clustering.
///
/// Public fields are intentionally simple to make the structure easy to test.
pub struct SpectralMemoryGraph {
    pub notes: HashMap<u32, SMGNote>,
    pub next_id: u32,
    pub construction_time: Duration,
    // Cached structures for spectral processing
    pub similarity_matrix: Option<Array2<f32>>, // cosine similarity of embeddings
    pub spectral_embeddings: Option<Array2<f32>>, // eigenvectors (n x k)
    pub cluster_labels: Option<Array1<usize>>,  // optional K‑Means labels
    pub cluster_centroids: Option<HashMap<usize, Vec<f32>>>, // optional mean embeddings per cluster
    pub cluster_centroid_norms: Option<HashMap<usize, f32>>, // precomputed L2 norms of centroids for fast cosine similarity
    pub long_range_links: Option<Vec<(u32, u32, f32)>>, // (note_id_a, note_id_b, spectral_similarity)
}

impl SpectralMemoryGraph {
    /// Create a new, empty SMG.
    pub fn new() -> Result<Self> {
        logging::init();
        Ok(Self {
            notes: HashMap::new(),
            next_id: 0,
            construction_time: Duration::new(0, 0),
            similarity_matrix: None,
            spectral_embeddings: None,
            cluster_labels: None,
            cluster_centroids: None,
            cluster_centroid_norms: None,
            long_range_links: None,
        })
    }

    /// Get long-range links with optional top-k limit.
    ///
    /// Returns pairs of (note_id_a, note_id_b, spectral_similarity) for notes that are
    /// spectrally similar but semantically distant.
    pub fn get_long_range_links(&self, top_k: Option<usize>) -> Vec<(u32, u32, f32)> {
        match &self.long_range_links {
            Some(links) => {
                let mut links = links.clone();
                // Deterministic ordering: higher similarity first, then id order.
                links.sort_by(|a, b| {
                    b.2.total_cmp(&a.2)
                        .then_with(|| a.0.cmp(&b.0))
                        .then_with(|| a.1.cmp(&b.1))
                });
                if let Some(k) = top_k {
                    links.truncate(k);
                }
                links
            }
            None => Vec::new(),
        }
    }

    /// Get related notes for a specific note using long-range link scores.
    ///
    /// If `long_range_links` are available, this returns neighbors with their spectral
    /// similarity scores ranked by descending similarity and limited by `top_k` when provided.
    /// If they are not available, this falls back to the stored `related_note_ids` list on
    /// the note with a default score of `0.0`.
    pub fn get_related_note_links(&self, note_id: u32, top_k: Option<usize>) -> Vec<(u32, f32)> {
        if let Some(links) = &self.long_range_links {
            let mut neighbors: HashMap<u32, f32> = HashMap::new();
            for (a, b, score) in links.iter() {
                if *a == note_id {
                    neighbors
                        .entry(*b)
                        .and_modify(|curr| *curr = curr.max(*score))
                        .or_insert(*score);
                } else if *b == note_id {
                    neighbors
                        .entry(*a)
                        .and_modify(|curr| *curr = curr.max(*score))
                        .or_insert(*score);
                }
            }

            let mut ranked: Vec<(u32, f32)> = neighbors.into_iter().collect();
            ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

            if let Some(k) = top_k {
                ranked.truncate(k);
            }
            return ranked;
        }

        if let Some(note) = self.notes.get(&note_id) {
            let mut ids: Vec<(u32, f32)> = note
                .related_note_ids
                .iter()
                .map(|nid| (*nid, 0.0))
                .collect();
            if let Some(k) = top_k {
                ids.truncate(k);
            }
            return ids;
        }

        Vec::new()
    }

    /// Get related note ids for a specific note using long-range link scores.
    pub fn get_related_note_ids(&self, note_id: u32, top_k: Option<usize>) -> Vec<u32> {
        self.get_related_note_links(note_id, top_k)
            .into_iter()
            .map(|(nid, _)| nid)
            .collect()
    }

    /// Ingest a conversation turn into the SMG as a single `SMGNote`.
    pub fn ingest_turn(&mut self, turn: &ConversationTurn) -> Result<()> {
        let emb = embed::get_embedding(&turn.content)?;
        let norm = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
        let note = SMGNote {
            note_id: self.next_id,
            raw_content: turn.content.clone(),
            context: turn.clean_context(),
            embedding: emb,
            norm,
            source_turn_ids: vec![turn.turn_id],
            source_commit_ids: vec![turn.commit_id.clone()],
            source_timestamps: vec![turn.timestamp],
            spectral_coords: None,
            related_note_ids: Vec::new(),
        };
        self.notes.insert(self.next_id, note);
        self.next_id += 1;
        Ok(())
    }

    /// Ingest multiple conversation turns using linear embedding.
    ///
    /// This method processes turns one at a time using the individual embedding API.
    /// An optional progress callback can be provided to track progress.
    ///
    /// # Arguments
    ///
    /// * `turns` - slice of `ConversationTurn` to ingest
    /// * `progress` - optional progress callback that receives a message and progress fraction (0.0..1.0)
    ///
    /// # Returns
    ///
    /// `Ok(())` on success, or an error if embedding fails.
    pub fn ingest_turns_batch(
        &mut self,
        turns: &[ConversationTurn],
        progress: Option<ProgressCallback>,
    ) -> Result<()> {
        if turns.is_empty() {
            return Ok(());
        }

        // Extract all texts for batch embedding (parallel processing)
        let texts: Vec<String> = turns.iter().map(|turn| turn.content.clone()).collect();

        // Batch embed all texts in parallel using the worker pool
        let embedding_progress: Option<ProgressCallback> = progress.clone().map(|cb| {
            Arc::new(move |msg: String, fraction: f32| {
                cb(msg, fraction);
            }) as ProgressCallback
        });
        let embeddings = embed::get_embeddings(&texts, embedding_progress)
            .with_context(|| "batch embedding turns")?;

        // Reconstruct notes with embeddings in correct order
        for (turn, emb) in turns.iter().zip(embeddings.iter()) {
            let norm: f32 = emb.iter().map(|x: &f32| x * x).sum::<f32>().sqrt();
            let note = SMGNote {
                note_id: self.next_id,
                raw_content: turn.content.clone(),
                context: turn.clean_context(),
                embedding: emb.clone(),
                norm,
                source_turn_ids: vec![turn.turn_id],
                source_commit_ids: vec![turn.commit_id.clone()],
                source_timestamps: vec![turn.timestamp],
                spectral_coords: None,
                related_note_ids: Vec::new(),
            };
            self.notes.insert(self.next_id, note);
            self.next_id += 1;

            // Update progress callback if provided
            // Reconstruction takes 50% of total progress (0.5 to 1.0)
            if let Some(ref cb) = progress {
                let fraction = 0.5 + ((self.next_id - 1) as f32 / turns.len() as f32) * 0.5;
                cb(format!("Ingested turn {}", self.next_id - 1), fraction);
            }
        }

        Ok(())
    }

    /// Build spectral structures using the helper functions in `graph::spectral`.
    ///
    /// Pipeline:
    /// 1. assemble embedding matrix
    /// 2. cosine similarity matrix
    /// 3. sparsify adjacency
    /// 4. normalized Laplacian
    /// 5. eigen-decomposition
    /// 6. spectral embedding extraction
    /// 7. k-means clustering
    /// 8. centroid computation (in original embedding space)
    /// 9. long-range link detection
    ///
    /// The optional `progress` callback is called with a message and progress fraction (0.0..1.0)
    /// for each major step.
    pub fn build_spectral_structure(&mut self, progress: Option<ProgressCallback>) -> Result<()> {
        use crate::graph::spectral::{
            assemble_embedding_matrix, compute_centroids_in_embedding_space,
            compute_spectral_embeddings, cosine_similarity_matrix, detect_long_range_links,
            eigengap_heuristic, normalized_laplacian, run_kmeans_on_spectral, sparsify_adj,
            spectral_decomposition,
        };

        // Tunable constants.
        const SMG_NUM_SPECTRAL_DIMS: usize = 8;
        const ADJ_SPARSE_THRESHOLD: f32 = 0.2;
        const SPECTRAL_LINK_SIM: f32 = 0.7;
        const EMBED_LINK_SIM: f32 = 0.5;
        const MAX_CLUSTERS: usize = 12;
        const MIN_CLUSTERS: usize = 2;

        let n = self.notes.len();
        if n < 3 {
            // Nothing meaningful to do for very small graphs.
            if let Some(ref cb) = progress {
                cb("Graph too small for spectral analysis".to_string(), 1.0);
            }
            return Ok(());
        }

        // Helper to call progress callback if present
        let report_progress = |step: usize, total_steps: usize, msg: String| {
            if let Some(ref cb) = progress {
                let fraction = (step as f32) / (total_steps as f32);
                cb(msg, fraction);
            }
        };

        const TOTAL_STEPS: usize = 10;

        // Stable ordering of notes (sort by note_id to make behaviour deterministic).
        let mut note_ids: Vec<u32> = self.notes.keys().cloned().collect();
        note_ids.sort_unstable();

        // 1) Assemble embedding matrix (n x d).
        report_progress(1, TOTAL_STEPS, "Assembling embedding matrix".to_string());
        let embed_mat = assemble_embedding_matrix(&self.notes, &note_ids);

        // 2) Cosine similarity matrix (dense).
        report_progress(
            2,
            TOTAL_STEPS,
            "Computing cosine similarity matrix".to_string(),
        );
        let mut sim = cosine_similarity_matrix(&embed_mat);

        // 3) Sparsify adjacency in-place (zero diagonal + threshold).
        report_progress(3, TOTAL_STEPS, "Sparsifying adjacency matrix".to_string());
        sparsify_adj(&mut sim, ADJ_SPARSE_THRESHOLD);
        self.similarity_matrix = Some(sim.clone());

        // 4) Normalized Laplacian (L = I - D^{-1/2} W D^{-1/2}).
        report_progress(4, TOTAL_STEPS, "Computing normalized Laplacian".to_string());
        let lap = normalized_laplacian(&sim);

        // 5) Eigen-decomposition.
        report_progress(5, TOTAL_STEPS, "Performing eigen-decomposition".to_string());
        let (eigenvalues, eigenvectors) = spectral_decomposition(&lap, SMG_NUM_SPECTRAL_DIMS)?;

        // 6) Spectral embeddings: take leading `k` eigenvectors and row-normalize.
        report_progress(6, TOTAL_STEPS, "Extracting spectral embeddings".to_string());
        let n_components = std::cmp::min(SMG_NUM_SPECTRAL_DIMS, n.saturating_sub(1));
        let spectral_emb = compute_spectral_embeddings(&eigenvectors, n_components, true);
        self.spectral_embeddings = Some(spectral_emb.clone());

        // 7) Decide number of clusters.
        report_progress(
            7,
            TOTAL_STEPS,
            "Determining optimal cluster count".to_string(),
        );
        // The eigengap heuristic expects eigenvalues sorted ascending as produced by nalgebra.
        let mut suggested_k = eigengap_heuristic(&eigenvalues);
        // Clamp into sensible bounds using the standard library `clamp`.
        suggested_k = suggested_k.clamp(MIN_CLUSTERS, MAX_CLUSTERS);
        // Also ensure we don't ask for more clusters than points.
        let n_clusters = std::cmp::min(suggested_k, std::cmp::max(MIN_CLUSTERS, n));

        // 8) K-Means on spectral embeddings.
        report_progress(8, TOTAL_STEPS, "Running K-Means clustering".to_string());
        let labels = run_kmeans_on_spectral(&spectral_emb, n_clusters)?;
        self.cluster_labels = Some(labels.clone());

        // 9) Compute centroids in original embedding space.
        report_progress(9, TOTAL_STEPS, "Computing cluster centroids".to_string());
        let centroids_map =
            compute_centroids_in_embedding_space(&labels, note_ids.as_slice(), &self.notes);
        self.cluster_centroids = Some(centroids_map.clone());

        // Precompute centroid norms for fast cosine similarity during queries
        let centroid_norms: HashMap<usize, f32> = centroids_map
            .iter()
            .map(|(c, vec)| (*c, vec.iter().map(|x| x * x).sum::<f32>().sqrt()))
            .collect();
        self.cluster_centroid_norms = Some(centroid_norms);

        // 10) Long-range spectral links: spectral similarity high && embedding similarity low.
        report_progress(
            10,
            TOTAL_STEPS,
            "Detecting long-range semantic links".to_string(),
        );
        let pairs = detect_long_range_links(
            &spectral_emb,
            self.similarity_matrix
                .as_ref()
                .expect("similarity matrix set"),
            SPECTRAL_LINK_SIM,
            EMBED_LINK_SIM,
            note_ids.as_slice(),
            None, // no top-k limit during build
        );
        // Store the links with scores for later retrieval
        self.long_range_links = Some(pairs.clone());

        // Also populate related_note_ids (for backward compatibility). Reset first
        // to prevent stale links from accumulating across repeated rebuilds.
        for note in self.notes.values_mut() {
            note.related_note_ids.clear();
        }
        for (a, b, _) in pairs.into_iter() {
            if let Some(note_a) = self.notes.get_mut(&a) {
                if !note_a.related_note_ids.contains(&b) {
                    note_a.related_note_ids.push(b);
                }
            }
            if let Some(note_b) = self.notes.get_mut(&b) {
                if !note_b.related_note_ids.contains(&a) {
                    note_b.related_note_ids.push(a);
                }
            }
        }

        Ok(())
    }

    /// Retrieve candidate per-turn records with raw semantic scores and timestamps.
    ///
    /// This method returns a flat list of `temporal::Candidate` where `raw_score` is the
    /// note-level semantic similarity (optionally cluster-boosted) and `timestamp` is the
    /// per-source-turn timestamp. Callers can pass these into the temporal re-ranker
    /// to compute final scores.
    pub fn retrieve_candidates(
        &self,
        query: &str,
        candidate_note_k: usize,
    ) -> Result<Vec<crate::temporal::Candidate>> {
        use rayon::prelude::*;

        // Embed query.
        let query_emb = embed::get_embedding(query)?;
        // Use ndarray operations for efficient norm computation
        let query_arr = Array1::from(query_emb);
        let norm_q = query_arr.dot(&query_arr).sqrt();

        // Stable ordering of notes (sort by note_id).
        let note_ids: Vec<u32> = {
            let mut v: Vec<u32> = self.notes.keys().cloned().collect();
            v.sort_unstable();
            v
        };

        // Compute raw cosine similarity per note (note-level score) using precomputed norms.
        // Use parallel iteration for better performance on multi-core systems.
        let mut scores: Vec<(usize, f32)> = note_ids
            .par_iter()
            .enumerate()
            .map(|(i, nid)| {
                let note = &self.notes[nid];
                // Use ndarray operations for efficient dot product computation
                let note_arr = Array1::from(note.embedding.clone());
                let dot = note_arr.dot(&query_arr);
                let raw_sim = if note.norm == 0.0 || norm_q == 0.0 {
                    0.0
                } else {
                    dot / (note.norm * norm_q)
                };
                (i, raw_sim)
            })
            .collect();

        // Apply centroid-based boosting if clusters exist.
        // Use precomputed centroid norms for fast cosine similarity.
        if let (Some(labels), Some(centroids), Some(centroid_norms)) = (
            &self.cluster_labels,
            &self.cluster_centroids,
            &self.cluster_centroid_norms,
        ) {
            // Compute centroid scores using precomputed norms and ndarray operations
            let mut centroid_scores: Vec<(usize, f32)> = Vec::new();
            for (c, centroid_vec) in centroids.iter() {
                let centroid_arr = Array1::from(centroid_vec.clone());
                let dot = centroid_arr.dot(&query_arr);
                // Use precomputed centroid norm
                let norm_c = centroid_norms.get(c).copied().unwrap_or(0.0);
                let c_sim = if norm_c == 0.0 || norm_q == 0.0 {
                    0.0
                } else {
                    dot / (norm_c * norm_q)
                };
                centroid_scores.push((*c, c_sim));
            }
            centroid_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            let top_clusters: std::collections::HashSet<usize> =
                centroid_scores.iter().take(3).map(|(c, _)| *c).collect();

            // Boost scores for notes in top clusters using parallel iteration
            scores.par_iter_mut().enumerate().for_each(|(i, entry)| {
                if let Some(lbl) = labels.get(i) {
                    if top_clusters.contains(lbl) {
                        entry.1 *= 1.2;
                    }
                }
            });
        }

        // Rank notes by score and take top candidate_note_k notes.
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        // Expand top notes into candidates using parallel iteration for better performance
        let candidates: Vec<crate::temporal::Candidate> = scores
            .par_iter()
            .take(candidate_note_k)
            .flat_map(|(idx, score)| {
                let nid = note_ids[*idx];
                if let Some(note) = self.notes.get(&nid) {
                    note.source_turn_ids
                        .iter()
                        .enumerate()
                        .map(move |(i, tid)| {
                            let ts = note.source_timestamps.get(i).cloned();
                            crate::temporal::Candidate {
                                turn_id: *tid,
                                note_id: nid,
                                raw_score: *score,
                                timestamp: ts,
                            }
                        })
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                }
            })
            .collect();
        Ok(candidates)
    }

    /// Retrieve candidates from a filtered set of note IDs.
    ///
    /// This method is similar to `retrieve_candidates` but only considers notes
    /// in the provided `filtered_note_ids` list. This is useful for time-based
    /// filtering or other pre-filtering scenarios.
    ///
    /// # Arguments
    ///
    /// * `query` - query string to search for
    /// * `candidate_note_k` - number of candidate notes to retrieve
    /// * `filtered_note_ids` - list of note IDs to consider (must be sorted for deterministic behavior)
    ///
    /// # Returns
    ///
    /// A list of `temporal::Candidate` with raw semantic scores and timestamps.
    fn retrieve_candidates_filtered(
        &self,
        query: &str,
        candidate_note_k: usize,
        filtered_note_ids: &[u32],
    ) -> Result<Vec<crate::temporal::Candidate>> {
        use rayon::prelude::*;

        // Embed query.
        let query_emb = embed::get_embedding(query)?;
        // Use ndarray operations for efficient norm computation
        let query_arr = Array1::from(query_emb);
        let norm_q = query_arr.dot(&query_arr).sqrt();

        // Use the provided filtered note IDs (assume they're already sorted)
        let note_ids: Vec<u32> = filtered_note_ids.to_vec();

        // Compute raw cosine similarity per note (note-level score) using precomputed norms.
        // Use parallel iteration for better performance on multi-core systems.
        let mut scores: Vec<(usize, f32)> = note_ids
            .par_iter()
            .enumerate()
            .map(|(i, nid)| {
                let note = &self.notes[nid];
                // Use ndarray operations for efficient dot product computation
                let note_arr = Array1::from(note.embedding.clone());
                let dot = note_arr.dot(&query_arr);
                let raw_sim = if note.norm == 0.0 || norm_q == 0.0 {
                    0.0
                } else {
                    dot / (note.norm * norm_q)
                };
                (i, raw_sim)
            })
            .collect();

        // Apply centroid-based boosting if clusters exist.
        // Use precomputed centroid norms for fast cosine similarity.
        if let (Some(labels), Some(centroids), Some(centroid_norms)) = (
            &self.cluster_labels,
            &self.cluster_centroids,
            &self.cluster_centroid_norms,
        ) {
            // Compute centroid scores using precomputed norms and ndarray operations
            let mut centroid_scores: Vec<(usize, f32)> = Vec::new();
            for (c, centroid_vec) in centroids.iter() {
                let centroid_arr = Array1::from(centroid_vec.clone());
                let dot = centroid_arr.dot(&query_arr);
                // Use precomputed centroid norm
                let norm_c = centroid_norms.get(c).copied().unwrap_or(0.0);
                let c_sim = if norm_c == 0.0 || norm_q == 0.0 {
                    0.0
                } else {
                    dot / (norm_c * norm_q)
                };
                centroid_scores.push((*c, c_sim));
            }
            centroid_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            let top_clusters: std::collections::HashSet<usize> =
                centroid_scores.iter().take(3).map(|(c, _)| *c).collect();

            // Boost scores for notes in top clusters using parallel iteration
            scores.par_iter_mut().enumerate().for_each(|(i, entry)| {
                if let Some(lbl) = labels.get(i) {
                    if top_clusters.contains(lbl) {
                        entry.1 *= 1.2;
                    }
                }
            });
        }

        // Rank notes by score and take top candidate_note_k notes.
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        // Expand top notes into candidates using parallel iteration for better performance
        let candidates: Vec<crate::temporal::Candidate> = scores
            .par_iter()
            .take(candidate_note_k)
            .flat_map(|(idx, score)| {
                let nid = note_ids[*idx];
                if let Some(note) = self.notes.get(&nid) {
                    note.source_turn_ids
                        .iter()
                        .enumerate()
                        .map(move |(i, tid)| {
                            let ts = note.source_timestamps.get(i).cloned();
                            crate::temporal::Candidate {
                                turn_id: *tid,
                                note_id: nid,
                                raw_score: *score,
                                timestamp: ts,
                            }
                        })
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                }
            })
            .collect();
        Ok(candidates)
    }

    /// Retrieve a list of (turn_id, score) pairs for the top-k matches to the query.
    ///
    /// The returned `score` is the final similarity used for ranking (semantic similarity
    /// combined with a temporal recency signal). This method applies default temporal
    /// re-ranking.
    pub fn retrieve_with_scores(&self, query: &str, top_k: usize) -> Result<Vec<(u64, f32)>> {
        self.retrieve_with_scores_config(query, top_k, None)
    }

    /// Retrieve with a specific temporal configuration.
    pub fn retrieve_with_scores_config(
        &self,
        query: &str,
        top_k: usize,
        temporal_cfg: Option<crate::temporal::TemporalConfig>,
    ) -> Result<Vec<(u64, f32)>> {
        let candidates = self.retrieve_candidates(query, top_k)?;
        let cfg = temporal_cfg.unwrap_or_default();
        let re_ranked = crate::temporal::re_rank_with_temporal(candidates, &cfg, None);

        let results: Vec<(u64, f32)> = re_ranked
            .into_iter()
            .map(|cws| (cws.candidate.turn_id, cws.final_score))
            .collect();

        Ok(results)
    }

    /// Retrieve with time-based filtering.
    ///
    /// This method allows filtering notes by timestamp range before computing similarity,
    /// which can significantly improve query performance when only a subset of the graph
    /// is needed.
    ///
    /// # Arguments
    ///
    /// * `query` - query string to search for
    /// * `top_k` - number of top results to return
    /// * `temporal_cfg` - optional temporal configuration
    /// * `time_start` - optional start time (unix epoch seconds) - only notes with timestamps >= this will be considered
    /// * `time_end` - optional end time (unix epoch seconds) - only notes with timestamps <= this will be considered
    ///
    /// # Returns
    ///
    /// A list of (turn_id, score) pairs for the top-k matches, filtered by time range.
    pub fn retrieve_with_scores_config_filtered(
        &self,
        query: &str,
        top_k: usize,
        temporal_cfg: Option<crate::temporal::TemporalConfig>,
        time_start: Option<u64>,
        time_end: Option<u64>,
    ) -> Result<Vec<(u64, f32)>> {
        let start_filter = Instant::now();
        // If no time filters are specified, use the standard unfiltered path
        if time_start.is_none() && time_end.is_none() {
            return self.retrieve_with_scores_config(query, top_k, temporal_cfg);
        }

        // Filter notes by time range before computing similarity
        let filtered_note_ids: Vec<u32> = self
            .notes
            .iter()
            .filter(|(_nid, note)| {
                // Check if note has any timestamps
                if note.source_timestamps.is_empty() {
                    return false;
                }

                // Get the earliest timestamp for this note
                let note_min_ts = *note.source_timestamps.iter().min().unwrap();
                let note_max_ts = *note.source_timestamps.iter().max().unwrap();

                // Apply time filters
                if let Some(start) = time_start {
                    if note_max_ts < start {
                        return false;
                    }
                }
                if let Some(end) = time_end {
                    if note_min_ts > end {
                        return false;
                    }
                }

                true
            })
            .map(|(nid, _)| *nid)
            .collect();

        // If no notes match the time filter, return empty results
        if filtered_note_ids.is_empty() {
            return Ok(Vec::new());
        }
        eprintln!(
            "Filtered {:?} note IDs in {:?}",
            filtered_note_ids.len(),
            start_filter.elapsed()
        );

        // Use the filtered note set for retrieval
        let candidates = self.retrieve_candidates_filtered(query, top_k, &filtered_note_ids)?;
        let cfg = temporal_cfg.unwrap_or_default();
        let re_ranked = crate::temporal::re_rank_with_temporal(candidates, &cfg, None);

        let results: Vec<(u64, f32)> = re_ranked
            .into_iter()
            .map(|cws| (cws.candidate.turn_id, cws.final_score))
            .collect();

        Ok(results)
    }

    /// Retrieve top-k matching source turn ids for the query string.
    ///
    /// This delegating method calls `retrieve_with_scores` and returns only the
    /// turn ids to preserve the previous `retrieve` behaviour for callers that
    /// expect just IDs.
    pub fn retrieve(&self, query: &str, top_k: usize) -> Result<Vec<u64>> {
        let scored = self.retrieve_with_scores(query, top_k)?;
        let ids = scored.into_iter().map(|(tid, _score)| tid).collect();
        Ok(ids)
    }
}
