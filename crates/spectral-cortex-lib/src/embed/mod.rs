/*
Conditional embedder implementation:

- The project now intends for the "real" MiniLM-based embedder to be the default.
  To support this at the crate-feature level, the workspace/Cargo.toml should
  mark `real-embed` as a default feature (for example: `default = ["real-embed"]`).

- For tests and CI where deterministic, fast embeddings are desired, the
  `fake-embed` feature is provided. Enabling `fake-embed` will select the
  deterministic fake embedder implementation.

Notes:
- The real embedder implementation is enabled when the `real-embed` feature is
  active and depends on the optional `rust_embed` dependency (this is unchanged).
- If neither `real-embed` nor `fake-embed` is selected at compile time this
  module emits a helpful compile-time error directing you to enable the
  appropriate feature or make `real-embed` the crate default.
*/

#[cfg(not(any(test, feature = "fake-embed")))]
mod real {
    use anyhow::Result;
    use once_cell::sync::Lazy;
    use rust_embed::pool::{EmbeddingPool, ModelType, PoolConfig};
    use std::sync::Mutex;
    use std::time::Instant;

    /// Global embedding pool guarded by a mutex for thread‑safety.
    static POOL: Lazy<Mutex<Option<EmbeddingPool>>> = Lazy::new(|| Mutex::new(None));

    /// Initialize the embedding pool with specified configuration.
    /// Must be called before any embedding operations.
    ///
    /// # Arguments
    ///
    /// * `workers` - Number of parallel worker threads (recommended: 4 for typical use)
    /// * `cache_size` - Cache size per worker (0 = no caching, recommended for unique commits)
    pub fn init(workers: usize, cache_size: usize) -> Result<()> {
        let start = Instant::now();

        let config = PoolConfig {
            cpu_workers: workers,
            gpu_workers: 0, // Phase 1: CPU only
            model: ModelType::MiniLM,
            cache_size_per_worker: cache_size,
            routing_config: None, // Not needed for MiniLM (CPU-only model)
        };

        let pool = EmbeddingPool::new(config)?;

        let mut guard = POOL.lock().unwrap();
        *guard = Some(pool);

        eprintln!(
            "Embedding pool initialized with {} workers in {:?}",
            workers,
            start.elapsed()
        );
        Ok(())
    }

    /// Embed a single piece of text, returning a plain `Vec<f32>`.
    pub fn get_embedding(text: &str) -> Result<Vec<f32>> {
        let guard = POOL.lock().unwrap();
        let pool = guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Pool not initialized. Call init() first."))?;

        let results = pool.embed_batch(vec![text.to_string()])?;
        Ok(results.into_iter().next().unwrap().to_vec())
    }

    /// Embed a batch of texts using the worker pool with progress reporting.
    /// This is now parallel and much faster than the sequential version.
    ///
    /// # Arguments
    ///
    /// * `texts` - Slice of texts to embed
    /// * `progress` - Optional progress callback that receives a message and progress fraction (0.0..1.0)
    pub fn get_embeddings(
        texts: &[String],
        progress: Option<crate::graph::ProgressCallback>,
    ) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let guard = POOL.lock().unwrap();
        let pool = guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Pool not initialized. Call init() first."))?;

        // Process in chunks to provide progress updates
        // Use an atomic counter to track completed items for thread-safe progress
        let mut completed = 0; // Arc::new(AtomicUsize::new(0));
        let total = texts.len();

        // Clone the progress callback for use in parallel context
        let progress_clone = progress.clone();

        // Use Rayon to process chunks in parallel for progress tracking
        // The pool handles the actual embedding parallelism internally
        let results: Vec<Vec<Vec<f32>>> = texts
            .chunks(32)
            .map(|chunk| {
                // Embed this chunk using the pool (which has its own worker threads)
                let chunk_results = pool.embed_batch(chunk.to_vec())?;

                if let Some(ref cb) = progress_clone {
                    let chunk_len = chunk.len();
                    completed += chunk_len;
                    let fraction = (completed.min(total) as f32) / (total as f32);
                    cb("Embedding".to_string(), fraction);
                }

                Ok(chunk_results.into_iter().map(|arr| arr.to_vec()).collect())
            })
            .collect::<Result<Vec<_>>>()?;

        // Flatten the chunk results into a single vector
        let mut flattened = Vec::with_capacity(total);
        for chunk_results in results {
            flattened.extend(chunk_results);
        }

        Ok(flattened)
    }

    /// Shutdown the pool gracefully.
    pub fn shutdown() -> Result<()> {
        let mut guard = POOL.lock().unwrap();
        if let Some(pool) = guard.take() {
            pool.shutdown()?;
        }
        Ok(())
    }
}

#[cfg(any(test, feature = "fake-embed"))]
mod fake {
    use anyhow::Result;
    use once_cell::sync::Lazy;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::sync::Mutex;

    // Keep the fake embedding dimension compatible with common MiniLM dims (384).
    // This keeps downstream code shapes stable for development and tests.
    const FAKE_EMBED_DIM: usize = 384;

    // Simple mutex to mirror the initialization semantics of the real embedder.
    static FAKE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    /// Initialise the fake embedder (no-op but mirrors the real init API).
    pub fn init(_workers: usize, _cache_size: usize) -> Result<()> {
        // Acquire and immediately release the lock to mimic any potential setup cost.
        let _g = FAKE_LOCK.lock().unwrap();
        Ok(())
    }

    /// Deterministic, fast embedding: hash the input together with the index to
    /// produce stable floats in [-1.0, 1.0]. This is sufficient for development,
    /// testing, and CI where real model assets are unnecessary.
    fn deterministic_embedding(text: &str) -> Vec<f32> {
        let mut out = Vec::with_capacity(FAKE_EMBED_DIM);
        for i in 0..FAKE_EMBED_DIM {
            let mut hasher = DefaultHasher::new();
            text.hash(&mut hasher);
            i.hash(&mut hasher);
            let h = hasher.finish();
            // Map u64 -> f64 in [0,1], then to [-1,1]
            // Use the associated constant `u64::MAX` instead of the legacy `std::u64::MAX`.
            let v = (h as f64) / (u64::MAX as f64);
            let v = (v * 2.0) - 1.0;
            out.push(v as f32);
        }
        out
    }

    /// Embed a single string deterministically.
    pub fn get_embedding(text: &str) -> Result<Vec<f32>> {
        let _g = FAKE_LOCK.lock().unwrap();
        Ok(deterministic_embedding(text))
    }

    /// Embed a batch of texts using the same deterministic function with progress reporting.
    pub fn get_embeddings(
        texts: &[String],
        progress: Option<crate::graph::ProgressCallback>,
    ) -> Result<Vec<Vec<f32>>> {
        let _g = FAKE_LOCK.lock().unwrap();
        let mut res = Vec::with_capacity(texts.len());
        let total = texts.len();

        for (idx, t) in texts.iter().enumerate() {
            res.push(deterministic_embedding(t));

            // Update progress
            if let Some(ref cb) = progress {
                let fraction = (idx + 1) as f32 / total as f32;
                cb(format!("Embedding {}/{}", idx + 1, total), fraction);
            }
        }
        Ok(res)
    }

    /// Shutdown the pool gracefully (no-op for fake embedder).
    pub fn shutdown() -> Result<()> {
        Ok(())
    }
}

// Re-export a uniform API according to selection.
// Behavior:
// - In tests (`cfg(test)`) or when the `fake-embed` feature is enabled the fake,
//   deterministic embedder is used. This keeps CI and unit tests stable.
// - Otherwise the real MiniLM embedder is used by default (no feature flag
//   required).
#[cfg(any(test, feature = "fake-embed"))]
pub use fake::{get_embedding, get_embeddings, init, shutdown};

#[cfg(not(any(test, feature = "fake-embed")))]
pub use real::{get_embedding, get_embeddings, init, shutdown};
