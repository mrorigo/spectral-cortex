/*!
Spectral analysis utilities for the Spectral Memory Graph (SMG).

This module implements the numerical building blocks:

- Embedding matrix assembly
- Cosine similarity computation and sparsification
- Degree and normalized Laplacian construction
- Eigen-decomposition and eigengap heuristic (uses Lanczos for speed, with SymmetricEigen fallback)
- Spectral embedding extraction and normalization
- K‑Means clustering wrapper and centroid computation
- Long-range link detection

All public functions include `# Arguments`, `# Returns`, and `# Errors` sections
in their docstrings to comply with the project's documentation guidelines.
*/

use crate::lanzcos::{Hermitian, Order};
use anyhow::Result;
use linfa::prelude::*;
use linfa_clustering::KMeans;
use nalgebra::linalg::SymmetricEigen;
use nalgebra::DMatrix;
use ndarray::{s, Array1, Array2, Axis};
use std::collections::HashMap;

use crate::model::smg_note::SMGNote;

/// Assemble an embedding matrix (n × d) from the provided notes and an explicit ordering.
///
/// # Arguments
///
/// * `notes` - map from note id to `SMGNote`
/// * `order` - vector of note ids specifying the row order in the returned matrix
///
/// # Returns
///
/// An `Array2<f32>` with shape `(n, d)` where `n = order.len()` and `d` is the embedding dim.
///
/// # Panics
///
/// Panics if `order` references a `note_id` that is not present in `notes`.
pub fn assemble_embedding_matrix(notes: &HashMap<u32, SMGNote>, order: &[u32]) -> Array2<f32> {
    let n = order.len();
    if n == 0 {
        return Array2::<f32>::zeros((0, 0));
    }
    // Determine embedding dimension from the first note in the specified order.
    let first = &notes[&order[0]];
    let d = first.embedding.len();
    let mut mat = Array2::<f32>::zeros((n, d));
    for (i, nid) in order.iter().enumerate() {
        let emb = &notes[nid].embedding;
        // assign requires an Array1; construct one from the Vec<f32>.
        mat.slice_mut(s![i, ..]).assign(&Array1::from(emb.clone()));
    }
    mat
}

/// Compute pairwise cosine similarity matrix from an embedding matrix `X` (n × d).
///
/// # Arguments
///
/// * `x` - embedding matrix (n × d)
///
/// # Returns
///
/// `W` — an `Array2<f32>` of shape (n, n) containing pairwise cosine similarities.
///
/// # Notes
///
/// This function computes the full dense similarity matrix. For large `n` you may
/// want to replace this with a sparse or approximate approach.
pub fn cosine_similarity_matrix(x: &Array2<f32>) -> Array2<f32> {
    use rayon::prelude::*;

    let n = x.nrows();

    // Compute upper triangle in parallel
    let upper_tri: Vec<Vec<(usize, f32)>> = (0..n)
        .into_par_iter()
        .map(|i| {
            let vi = x.slice(s![i, ..]);
            let norm_i = vi.iter().map(|v| v * v).sum::<f32>().sqrt();

            let mut row = Vec::new();
            for j in i..n {
                let vj = x.slice(s![j, ..]);
                let dot: f32 = vi.iter().zip(vj.iter()).map(|(a, b)| a * b).sum();
                let norm_j = vj.iter().map(|v| v * v).sum::<f32>().sqrt();
                let cosine = if norm_i == 0.0 || norm_j == 0.0 {
                    0.0
                } else {
                    dot / (norm_i * norm_j)
                };
                row.push((j, cosine));
            }
            row
        })
        .collect();

    // Build symmetric matrix
    let mut sim = Array2::<f32>::zeros((n, n));
    for (i, row) in upper_tri.iter().enumerate() {
        for &(j, val) in row.iter() {
            sim[(i, j)] = val;
            sim[(j, i)] = val;
        }
    }

    sim
}
// pub fn cosine_similarity_matrix(x: &Array2<f32>) -> Array2<f32> {
//     let n = x.nrows();
//     let mut sim = Array2::<f32>::zeros((n, n));
//     for i in 0..n {
//         let vi = x.slice(s![i, ..]);
//         let norm_i = vi.iter().map(|v| v * v).sum::<f32>().sqrt();
//         for j in i..n {
//             let vj = x.slice(s![j, ..]);
//             let dot: f32 = vi.iter().zip(vj.iter()).map(|(a, b)| a * b).sum();
//             let norm_j = vj.iter().map(|v| v * v).sum::<f32>().sqrt();
//             let cosine = if norm_i == 0.0 || norm_j == 0.0 {
//                 0.0
//             } else {
//                 dot / (norm_i * norm_j)
//             };
//             sim[(i, j)] = cosine;
//             sim[(j, i)] = cosine;
//         }
//     }
//     sim
// }

/// Sparsify adjacency matrix in-place by zeroing entries below `threshold` and forcing diagonal zero.
///
/// # Arguments
///
/// * `w` - mutable adjacency matrix (n × n)
/// * `threshold` - edges with value < threshold will be zeroed
pub fn sparsify_adj(w: &mut Array2<f32>, threshold: f32) {
    let n = w.nrows();

    for i in 0..n {
        for j in 0..n {
            if i == j || w[(i, j)] < threshold {
                w[(i, j)] = 0.0;
            }
        }
    }
}
// pub fn sparsify_adj(w: &mut Array2<f32>, threshold: f32) {
//     let n = w.nrows();
//     for i in 0..n {
//         for j in 0..n {
//             if i == j || w[(i, j)] < threshold {
//                 w[(i, j)] = 0.0;
//             }
//         }
//     }
// }

/// Compute degree vector `d = W · 1` (length n).
///
/// # Arguments
///
/// * `w` - adjacency matrix (n × n)
///
/// # Returns
///
/// Degree vector as `Array1<f32>`.
pub fn degree_vector(w: &Array2<f32>) -> Array1<f32> {
    w.sum_axis(Axis(1))
}

/// Compute the normalized Laplacian `L = I - D^{-1/2} W D^{-1/2}`.
///
/// # Arguments
///
/// * `w` - adjacency matrix (n × n)
///
/// # Returns
///
/// Normalized Laplacian as `Array2<f32>`.
pub fn normalized_laplacian(w: &Array2<f32>) -> Array2<f32> {
    let n = w.nrows();
    let degree = degree_vector(w);
    let mut d_inv_sqrt = degree.clone();
    for v in d_inv_sqrt.iter_mut() {
        *v = if *v > 0.0 { 1.0 / v.sqrt() } else { 0.0 };
    }
    let mut norm = Array2::<f32>::zeros((n, n));
    for i in 0..n {
        for j in 0..n {
            norm[(i, j)] = d_inv_sqrt[i] * w[(i, j)] * d_inv_sqrt[j];
        }
    }
    // L = I - norm
    for i in 0..n {
        norm[(i, i)] = 1.0 - norm[(i, i)];
    }
    norm
}

/// Compute eigenvalues and eigenvectors of a symmetric matrix using `nalgebra`.
///
/// # Arguments
///
/// * `l` - symmetric matrix (n × n)
/// * `_k` - requested number of spectral components (kept for API compatibility)
///
/// # Returns
///
/// Tuple `(eigenvalues, eigenvectors)` where `eigenvalues` is length `n` and
/// `eigenvectors` is an `Array2<f32>` with shape `(n, n)` whose columns are eigenvectors.
///
/// # Errors
///
/// Returns an error if the underlying library fails (propagates nalgebra errors via `anyhow`).
pub fn spectral_decomposition(l: &Array2<f32>, k: usize) -> Result<(Array1<f32>, Array2<f32>)> {
    let n = l.nrows();
    if n == 0 {
        return Ok((Array1::<f32>::zeros(0), Array2::<f32>::zeros((0, 0))));
    }

    // Try Lanczos for fast eigen-decomposition (only compute k eigenvectors)
    // Fall back to full eigen-decomposition if Lanczos fails
    let k_for_closure = k;
    spectral_decomposition_lanczos(l, k_for_closure).or_else(|_| {
        eprintln!("Lanczos failed, falling back to full eigen-decomposition");
        // Fallback to full eigen-decomposition
        spectral_decomposition_full(l)
    })
}

/// Fast eigen-decomposition using Lanczos algorithm (computes only k eigenvectors).
///
/// # Arguments
///
/// * `l` - symmetric matrix (n × n)
/// * `k` - requested number of spectral components
///
/// # Returns
///
/// Tuple `(eigenvalues, eigenvectors)` where `eigenvalues` is length `k` and
/// `eigenvectors` is an `Array2<f32>` with shape `(n, k)` whose columns are eigenvectors.
///
/// # Errors
///
/// Returns an error if Lanczos fails or if the matrix is too small.
pub fn spectral_decomposition_lanczos(
    l: &Array2<f32>,
    k: usize,
) -> Result<(Array1<f32>, Array2<f32>)> {
    let n = l.nrows();
    if n == 0 {
        return Err(anyhow::anyhow!("Matrix is empty"));
    }
    if n < k {
        return Err(anyhow::anyhow!(
            "Matrix size {} is smaller than requested k {}",
            n,
            k
        ));
    }

    // Convert ndarray to nalgebra DMatrix (f32 -> f64 for lanczos compatibility)
    let dm_f32 = DMatrix::from_iterator(n, n, l.iter().cloned());
    let dm_f64: DMatrix<f64> = dm_f32.map(|x| x as f64);

    // Use Lanczos to compute k smallest eigenvalues/eigenvectors
    // Use the Hermitian trait method from local lanczos implementation
    let eigen = dm_f64.eigsh(k, Order::Smallest);

    // Convert eigenvalues to Array1 (f64 -> f32)
    let eigvals_vec: Vec<f32> = eigen
        .eigenvalues
        .as_slice()
        .iter()
        .map(|x| *x as f32)
        .collect();
    let eigvals = Array1::from(eigvals_vec);

    // Convert eigenvectors to Array2 (n x k, f64 -> f32)
    let mut evecs = Array2::<f32>::zeros((n, k));
    for i in 0..n {
        for j in 0..k {
            evecs[(i, j)] = eigen.eigenvectors[(i, j)] as f32;
        }
    }

    Ok((eigvals, evecs))
}

/// Full eigen-decomposition using SymmetricEigen (fallback).
///
/// # Arguments
///
/// * `l` - symmetric matrix (n × n)
///
/// # Returns
///
/// Tuple `(eigenvalues, eigenvectors)` where `eigenvalues` is length `n` and
/// `eigenvectors` is an `Array2<f32>` with shape `(n, n)` whose columns are eigenvectors.
///
/// # Errors
///
/// Returns an error if the underlying library fails.
pub fn spectral_decomposition_full(l: &Array2<f32>) -> Result<(Array1<f32>, Array2<f32>)> {
    let n = l.nrows();
    if n == 0 {
        return Ok((Array1::<f32>::zeros(0), Array2::<f32>::zeros((0, 0))));
    }
    // Convert ndarray -> nalgebra DMatrix row-major iterator.
    // DMatrix::from_iterator expects column-major order; from_iterator fills column-major by rows,
    // but using the same iterator as earlier code is acceptable for symmetric matrices as long as we
    // treat layout consistently. We use from_iterator(n, n, l.iter().cloned()) as the original code.
    let dm = DMatrix::from_iterator(n, n, l.iter().cloned());
    let sym = SymmetricEigen::new(dm);
    let eigvals_vec = sym.eigenvalues.as_slice().to_vec();
    let eigvals = Array1::from(eigvals_vec);
    // Convert eigenvectors (DMatrix) into ndarray with the same layout (rows x cols).
    let mut evecs = Array2::<f32>::zeros((n, n));
    for i in 0..n {
        for j in 0..n {
            evecs[(i, j)] = sym.eigenvectors[(i, j)];
        }
    }
    Ok((eigvals, evecs))
}

/// Decide the number of clusters/dimensions using a simple eigengap heuristic.
///
/// # Arguments
///
/// * `eigenvalues` - array of eigenvalues (length n), assumed sorted ascending as returned by `SymmetricEigen`.
///
/// # Returns
///
/// `k` — suggested number of clusters (at least 2).
pub fn eigengap_heuristic(eigenvalues: &Array1<f32>) -> usize {
    let n = eigenvalues.len();
    if n <= 2 {
        return 2usize;
    }
    // Find the largest gap between consecutive eigenvalues.
    let mut best_idx = 1usize;
    let mut best_gap = 0.0_f32;
    for i in 1..n {
        let gap = eigenvalues[i] - eigenvalues[i - 1];
        if gap > best_gap {
            best_gap = gap;
            best_idx = i;
        }
    }
    let mut k = best_idx;
    if k < 2 {
        k = 2;
    }
    k
}

/// Extract the first `k` spectral embedding columns and optionally row-normalize each vector.
///
/// # Arguments
///
/// * `evecs` - matrix of eigenvectors (n × n) where columns are eigenvectors
/// * `k` - number of spectral dimensions to extract
/// * `row_normalize` - whether to normalize each row to unit L2 norm
///
/// # Returns
///
/// An `Array2<f32>` with shape `(n, k)`.
pub fn compute_spectral_embeddings(
    evecs: &Array2<f32>,
    k: usize,
    row_normalize: bool,
) -> Array2<f32> {
    let n = evecs.nrows();
    let k = std::cmp::min(k, evecs.ncols());
    let mut spec = Array2::<f32>::zeros((n, k));
    for i in 0..n {
        for j in 0..k {
            spec[(i, j)] = evecs[(i, j)];
        }
    }
    if row_normalize {
        for i in 0..n {
            let mut norm = 0.0_f32;
            for j in 0..k {
                norm += spec[(i, j)] * spec[(i, j)];
            }
            norm = norm.sqrt();
            if norm > 0.0 {
                for j in 0..k {
                    spec[(i, j)] /= norm;
                }
            }
        }
    }
    spec
}

/// Run K‑Means on the spectral embeddings and return labels.
///
/// # Arguments
///
/// * `spec` - spectral embeddings matrix (n × k)
/// * `n_clusters` - requested number of clusters
///
/// # Returns
///
/// `Array1<usize>` containing a label per row.
///
/// # Errors
///
/// Returns an error if the clustering algorithm fails.
pub fn run_kmeans_on_spectral(spec: &Array2<f32>, n_clusters: usize) -> Result<Array1<usize>> {
    // Provide an empty target array to satisfy Dataset typing.
    let targets = Array1::<usize>::zeros(0);
    let dataset = linfa::Dataset::new(spec.clone(), targets);
    let kmeans = KMeans::params(n_clusters)
        .max_n_iterations(100)
        .fit(&dataset)?;
    let labels = kmeans.predict(&dataset);
    Ok(labels)
}

/// Compute centroids in the original embedding space (Vec<f32> per cluster).
///
/// # Arguments
///
/// * `labels` - cluster labels for each row (length n)
/// * `note_ids` - ordering of note ids corresponding to the rows in `labels` (length n)
/// * `notes` - map from note id to `SMGNote` containing `embedding`
///
/// # Returns
///
/// `HashMap<usize, Vec<f32>>` mapping cluster id to centroid vector.
pub fn compute_centroids_in_embedding_space(
    labels: &Array1<usize>,
    note_ids: &[u32],
    notes: &HashMap<u32, SMGNote>,
) -> HashMap<usize, Vec<f32>> {
    let n = labels.len();
    let mut centroids: HashMap<usize, Vec<f32>> = HashMap::new();
    let mut counts: HashMap<usize, usize> = HashMap::new();
    if n == 0 {
        return centroids;
    }
    let dim = notes[&note_ids[0]].embedding.len();
    for (i, lbl) in labels.iter().enumerate() {
        let nid = note_ids[i];
        let emb = &notes[&nid].embedding;
        let entry = centroids.entry(*lbl).or_insert_with(|| vec![0.0_f32; dim]);
        for (k, v) in emb.iter().enumerate() {
            entry[k] += *v;
        }
        *counts.entry(*lbl).or_insert(0) += 1usize;
    }
    for (c, cent) in centroids.iter_mut() {
        if let Some(cnt) = counts.get(c) {
            let f = *cnt as f32;
            for v in cent.iter_mut() {
                *v /= f;
            }
        }
    }
    centroids
}

/// Detect long-range links: pairs of note ids where spectral similarity is high but embedding similarity is low.
///
/// # Arguments
///
/// * `spec` - spectral embeddings matrix (n × k)
/// * `emb_sim` - precomputed embedding similarity matrix (n × n)
/// * `spectral_sim_thr` - threshold above which spectral similarity is considered high
/// * `embed_sim_thr` - threshold below which embedding similarity is considered low
/// * `note_ids` - ordering of note ids corresponding to rows in `spec` and indices in `emb_sim`
///
/// # Returns
///
/// Vector of `(note_i, note_j, spectral_similarity)` tuples (by id) that should be linked.
pub fn detect_long_range_links(
    spec: &Array2<f32>,
    emb_sim: &Array2<f32>,
    spectral_sim_thr: f32,
    embed_sim_thr: f32,
    note_ids: &[u32],
    top_k: Option<usize>,
) -> Vec<(u32, u32, f32)> {
    use rayon::prelude::*;

    let n = spec.nrows();

    let pairs: Vec<(u32, u32, f32)> = (0..n)
        .into_par_iter()
        .flat_map(|i| {
            let vi = spec.slice(s![i, ..]);
            let norm_i = vi.iter().map(|v| v * v).sum::<f32>().sqrt();

            let mut row_pairs = Vec::new();
            for j in (i + 1)..n {
                let vj = spec.slice(s![j, ..]);
                let norm_j = vj.iter().map(|v| v * v).sum::<f32>().sqrt();
                let dot: f32 = vi.iter().zip(vj.iter()).map(|(a, b)| a * b).sum();
                let sp_sim = if norm_i == 0.0 || norm_j == 0.0 {
                    0.0
                } else {
                    dot / (norm_i * norm_j)
                };
                let emb_s = emb_sim[(i, j)];

                if sp_sim > spectral_sim_thr && emb_s < embed_sim_thr {
                    row_pairs.push((note_ids[i], note_ids[j], sp_sim));
                }
            }
            row_pairs.into_par_iter()
        })
        .collect();

    let mut pairs = pairs;
    // Deterministic ordering: higher similarity first, then id order.
    pairs.sort_by(|a, b| {
        b.2.total_cmp(&a.2)
            .then_with(|| a.0.cmp(&b.0))
            .then_with(|| a.1.cmp(&b.1))
    });
    if let Some(k) = top_k {
        pairs.truncate(k);
    }

    pairs
}

/// Placeholder API for incremental spectral updates. This function is intentionally
/// left as a documented stub for Phase 4 where approximation and local updates
/// will be implemented.
///
/// # Returns
///
/// Currently returns `Ok(())`. Will return structured errors in future implementations.
pub fn incremental_spectral_update() -> Result<()> {
    // TODO: implement incremental spectral update heuristics in Phase 4.
    Ok(())
}
