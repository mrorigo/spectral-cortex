# Rust Implementation Plan for Spectral Memory Graph

## Overview
The goal is to provide a fully functional Rust version of the Spectral Memory Graph (SMG) that:
1. Uses **rust‑embed** to ship the MiniLM model files.
2. Generates real embeddings via the embedded model.
3. Builds the spectral similarity matrix, computes the normalized Laplacian, and extracts eigen‑vectors.
4. Performs K‑Means clustering on the spectral embeddings.
5. Stores cluster centroids for query‑time boost.
6. Retrieves turn IDs with optional cluster‑based score boosting.
7. Includes a small demo binary (`src/main.rs`).

The plan is broken into concrete steps, each verified with compilation, `cargo fmt`, and `cargo clippy`.

## Detailed Steps
1. **Add crate dependencies** – `rust_embed`, `once_cell`, `anyhow`, `ndarray`, `nalgebra`, `linfa`, `linfa‑clustering`.
2. **Force a single `ndarray` version** (`0.15.6`) to avoid version conflicts between `rust_embed` and `linfa`.
3. **Implement `embed` module** that initialises the MiniLM embedder on first use and exposes `get_embedding`/`get_embeddings` returning `Vec<f32>`.
4. **Extend `SpectralMemoryGraph`** with cached fields for similarity matrix, spectral embeddings, cluster labels, and centroids.
5. **Build spectral structures**:
   - Assemble an embedding matrix (`Array2<f32>`).
   - Compute cosine similarity, sparsify and zero the diagonal.
   - Build the normalized Laplacian and run eigen‑decomposition via `nalgebra`.
   - Choose a number of dimensions (`SMG_NUM_SPECTRAL_DIMS`).
   - Apply the eigengap heuristic to decide the number of clusters.
   - Run K‑Means clustering on the spectral embeddings (targets `()`).
   - Store labels and compute centroids in the original embedding space.
   - Add long‑range links based on spectral similarity thresholds.
6. **Implement retrieval**:
   - Embed the query.
   - Compute cosine similarity to each note.
   - If clustering information exists, boost scores for notes belonging to the top‑3 clusters.
   - Return the top‑k turn IDs.
7. **Create a demo binary** (`src/main.rs`) that ingests a couple of synthetic turns and runs a query.
8. **Run `cargo fmt`** to enforce formatting.
9. **Run `cargo clippy -- -D warnings`** to ensure lint‑free code.
10. **Write `STATUS.md`** to track progress.

Each step will be marked in the plan and reflected in `STATUS.md`.

## Acceptance Criteria
* `cargo build` succeeds without errors.
* The demo binary prints a list of retrieved turn IDs.
* `cargo fmt` and `cargo clippy` pass.
* `RUST_PLAN.md` and `STATUS.md` accurately describe the implemented state.

