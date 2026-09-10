/// Common interface implemented by embedding backends.
pub trait Embedder: Send + Sync {
    /// Embed one text and return its vector.
    fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>>;

    /// Embed multiple texts and preserve input ordering.
    fn embed_batch(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>>;

    /// Return the stable backend name.
    fn get_name(&self) -> &'static str;
}

use serde::{Deserialize, Serialize};

/// Embedding backend parameters persisted alongside an SMG.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EmbeddingConfig {
    /// Backend identifier, such as `minilm` or `model2vec`.
    pub backend: String,
    /// Model identifier when the backend requires one.
    pub model_name: Option<String>,
    /// Normalization override used by the backend.
    pub normalize: Option<bool>,
    /// Batch size used for encoding.
    pub batch_size: Option<usize>,
}

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

#[cfg(feature = "model2vec-embedder")]
pub mod model2vec;

#[cfg(not(any(test, feature = "fake-embed")))]
mod real {
    use crate::embed::EmbeddingConfig;
    use anyhow::Result;
    use once_cell::sync::Lazy;
    use rust_embed::pool::{EmbeddingPool, ModelType, PoolConfig};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    #[cfg(feature = "model2vec-embedder")]
    use model2vec_rs::model::StaticModel;

    /// Global embedding pool guarded by a mutex for thread‑safety.
    static POOL: Lazy<Mutex<Option<Arc<EmbeddingPool>>>> = Lazy::new(|| Mutex::new(None));
    static CONFIG: Lazy<Mutex<Option<EmbeddingConfig>>> = Lazy::new(|| Mutex::new(None));

    #[cfg(feature = "model2vec-embedder")]
    type Model2VecRuntime = (Arc<StaticModel>, usize);

    #[cfg(feature = "model2vec-embedder")]
    static MODEL2VEC: Lazy<Mutex<Option<Model2VecRuntime>>> = Lazy::new(|| Mutex::new(None));

    /// Configuration for the optional Model2Vec backend.
    #[cfg(feature = "model2vec-embedder")]
    #[derive(Clone, Debug)]
    pub struct Model2VecConfig {
        /// Hugging Face model identifier or local model directory.
        pub model_name: String,
        /// Overrides the model's normalization setting.
        pub normalize: Option<bool>,
        /// Maximum number of texts encoded in one call.
        pub batch_size: usize,
        /// Optional subdirectory containing model files.
        pub subfolder: Option<String>,
    }

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

        let mut guard = POOL
            .lock()
            .map_err(|_| anyhow::anyhow!("embedding pool lock is poisoned"))?;
        *guard = Some(Arc::new(pool));
        *CONFIG
            .lock()
            .map_err(|_| anyhow::anyhow!("embedding config lock is poisoned"))? =
            Some(EmbeddingConfig {
                backend: "minilm".into(),
                model_name: None,
                normalize: None,
                batch_size: None,
            });

        eprintln!(
            "Embedding pool initialized with {} workers in {:?}",
            workers,
            start.elapsed()
        );
        Ok(())
    }

    /// Initialize the Model2Vec backend for subsequent embedding operations.
    #[cfg(feature = "model2vec-embedder")]
    pub fn init_model2vec(config: Model2VecConfig) -> Result<()> {
        if config.batch_size == 0 {
            anyhow::bail!("Model2Vec batch size must be greater than zero");
        }
        let model = StaticModel::from_pretrained(
            &config.model_name,
            None,
            config.normalize,
            config.subfolder.as_deref(),
        )?;
        let mut guard = MODEL2VEC
            .lock()
            .map_err(|_| anyhow::anyhow!("Model2Vec lock is poisoned"))?;
        *guard = Some((Arc::new(model), config.batch_size));
        *CONFIG
            .lock()
            .map_err(|_| anyhow::anyhow!("embedding config lock is poisoned"))? =
            Some(EmbeddingConfig {
                backend: "model2vec".into(),
                model_name: Some(config.model_name),
                normalize: config.normalize,
                batch_size: Some(config.batch_size),
            });
        Ok(())
    }

    /// Returns the active backend configuration for persistence.
    pub fn current_config() -> Option<EmbeddingConfig> {
        CONFIG.lock().ok().and_then(|guard| guard.clone())
    }

    /// Embed a single piece of text, returning a plain `Vec<f32>`.
    pub fn get_embedding(text: &str) -> Result<Vec<f32>> {
        #[cfg(feature = "model2vec-embedder")]
        if let Some(model) = MODEL2VEC
            .lock()
            .map_err(|_| anyhow::anyhow!("Model2Vec lock is poisoned"))?
            .as_ref()
            .cloned()
        {
            return model
                .0
                .encode_with_args(&[text.to_string()], None, model.1)
                .into_iter()
                .next()
                .ok_or_else(|| anyhow::anyhow!("Model2Vec returned no embedding"));
        }
        let pool: Arc<EmbeddingPool> = {
            let guard = POOL
                .lock()
                .map_err(|_| anyhow::anyhow!("embedding pool lock is poisoned"))?;
            guard
                .as_ref()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Pool not initialized. Call init() first."))?
        };

        let results = pool.embed_batch(vec![text.to_string()])?;
        results
            .into_iter()
            .next()
            .map(|embedding| embedding.to_vec())
            .ok_or_else(|| anyhow::anyhow!("embedding service returned no result"))
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

        #[cfg(feature = "model2vec-embedder")]
        if let Some(model) = MODEL2VEC
            .lock()
            .map_err(|_| anyhow::anyhow!("Model2Vec lock is poisoned"))?
            .as_ref()
            .cloned()
        {
            let result = model.0.encode_with_args(texts, None, model.1);
            if let Some(ref cb) = progress {
                cb("Embedding".to_string(), 1.0);
            }
            return Ok(result);
        }

        let pool: Arc<EmbeddingPool> = {
            let guard = POOL
                .lock()
                .map_err(|_| anyhow::anyhow!("embedding pool lock is poisoned"))?;
            guard
                .as_ref()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Pool not initialized. Call init() first."))?
        };

        use rayon::prelude::*;
        use std::sync::atomic::{AtomicUsize, Ordering};

        // Use a smaller chunk size for more frequent progress updates.
        // Concurrency from rayon and the underlying pool will still ensure high throughput.
        let total = texts.len();
        let chunk_size = 32;
        let chunks: Vec<_> = texts.chunks(chunk_size).collect();

        let completed = Arc::new(AtomicUsize::new(0));
        let progress_cb = progress.clone();

        let results: Result<Vec<Vec<Vec<f32>>>> = chunks
            .into_par_iter()
            .map(|chunk| {
                let pool = pool.clone();
                // Embed this chunk using the pool (which has its own internal worker threads)
                let chunk_results = pool.embed_batch(chunk.to_vec())?;

                let converted: Vec<Vec<f32>> = chunk_results
                    .into_iter()
                    .map(|arr| arr.iter().copied().collect::<Vec<f32>>())
                    .collect();

                if let Some(ref cb) = progress_cb {
                    let current = completed.fetch_add(chunk.len(), Ordering::SeqCst) + chunk.len();
                    let fraction = (current as f32) / (total as f32);
                    cb("Embedding".to_string(), fraction);
                }

                Ok(converted)
            })
            .collect();

        let mut all_results = Vec::with_capacity(total);
        for chunk_res in results? {
            all_results.extend(chunk_res);
        }

        Ok(all_results)
    }

    /// Shutdown the pool gracefully.
    pub fn shutdown() -> Result<()> {
        #[cfg(feature = "model2vec-embedder")]
        {
            let mut model_guard = MODEL2VEC
                .lock()
                .map_err(|_| anyhow::anyhow!("Model2Vec lock is poisoned"))?;
            model_guard.take();
        }
        let mut guard = POOL
            .lock()
            .map_err(|_| anyhow::anyhow!("embedding pool lock is poisoned"))?;
        if let Some(arc_pool) = guard.take() {
            // Attempt to gain ownership of the pool to call its `shutdown(self)` method.
            // If other Arcs still exist, we can't call `shutdown(self)` directly,
            // but in the context of this CLI, we shouldn't have leaked other Arcs.
            match Arc::try_unwrap(arc_pool) {
                Ok(pool) => pool.shutdown()?,
                Err(_) => {
                    // Fallback: If for some reason other references exist, we can't
                    // call the ownership-taking shutdown.
                    anyhow::bail!("Cannot shutdown EmbeddingPool: other references still exist.");
                }
            }
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
        let _g = FAKE_LOCK
            .lock()
            .map_err(|_| anyhow::anyhow!("fake embedding lock is poisoned"))?;
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
        let _g = FAKE_LOCK
            .lock()
            .map_err(|_| anyhow::anyhow!("fake embedding lock is poisoned"))?;
        Ok(deterministic_embedding(text))
    }

    /// Embed a batch of texts using the same deterministic function with progress reporting.
    pub fn get_embeddings(
        texts: &[String],
        progress: Option<crate::graph::ProgressCallback>,
    ) -> Result<Vec<Vec<f32>>> {
        let _g = FAKE_LOCK
            .lock()
            .map_err(|_| anyhow::anyhow!("fake embedding lock is poisoned"))?;
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
pub use real::current_config;
#[cfg(not(any(test, feature = "fake-embed")))]
pub use real::{get_embedding, get_embeddings, init, shutdown};

#[cfg(all(not(any(test, feature = "fake-embed")), feature = "model2vec-embedder"))]
pub use real::{init_model2vec, Model2VecConfig};
