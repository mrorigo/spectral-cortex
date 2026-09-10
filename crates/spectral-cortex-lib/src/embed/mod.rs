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
    /// Backend identifier; production graphs use `model2vec`.
    pub backend: String,
    /// Model identifier when the backend requires one.
    pub model_name: Option<String>,
    /// Normalization override used by the backend.
    pub normalize: Option<bool>,
    /// Batch size used for encoding.
    pub batch_size: Option<usize>,
}

#[cfg(not(any(test, feature = "fake-embed")))]
mod real {
    use crate::embed::EmbeddingConfig;
    use anyhow::Result;
    use model2vec_rs::model::StaticModel;
    use once_cell::sync::Lazy;
    use std::sync::{Arc, Mutex};

    /// Default Model2Vec checkpoint used for all new graphs.
    pub const DEFAULT_MODEL_NAME: &str = "grybsh/m2v-MiniLM-L6-v2";
    const DEFAULT_BATCH_SIZE: usize = 1024;

    type Model2VecRuntime = (Arc<StaticModel>, usize);
    static MODEL: Lazy<Mutex<Option<Model2VecRuntime>>> = Lazy::new(|| Mutex::new(None));
    static CONFIG: Lazy<Mutex<Option<EmbeddingConfig>>> = Lazy::new(|| Mutex::new(None));

    /// Configuration for the Model2Vec backend.
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

    /// Initialize the default Model2Vec checkpoint.
    pub fn init(workers: usize, cache_size: usize) -> Result<()> {
        let _ = workers;
        let _ = cache_size;
        init_model2vec(Model2VecConfig {
            model_name: DEFAULT_MODEL_NAME.into(),
            normalize: Some(true),
            batch_size: DEFAULT_BATCH_SIZE,
            subfolder: None,
        })
    }

    /// Initialize the Model2Vec backend for subsequent embedding operations.
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
        let mut guard = MODEL
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
        if let Some(model) = MODEL
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
        Err(anyhow::anyhow!(
            "Model2Vec is not initialized. Call init() first."
        ))
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

        if let Some(model) = MODEL
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

        let model = MODEL
            .lock()
            .map_err(|_| anyhow::anyhow!("Model2Vec lock is poisoned"))?
            .as_ref()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Model2Vec is not initialized. Call init() first."))?;
        let result = model.0.encode_with_args(texts, None, model.1);
        if let Some(cb) = progress {
            cb("Embedding".into(), 1.0);
        }
        Ok(result)
    }

    /// Shutdown the pool gracefully.
    pub fn shutdown() -> Result<()> {
        {
            let mut model_guard = MODEL
                .lock()
                .map_err(|_| anyhow::anyhow!("Model2Vec lock is poisoned"))?;
            model_guard.take();
        }
        Ok(())
    }
}

#[cfg(any(test, feature = "fake-embed"))]
mod fake {
    use crate::embed::EmbeddingConfig;
    use anyhow::Result;
    use once_cell::sync::Lazy;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::sync::Mutex;

    // Keep the fake embedding dimension representative of the default checkpoint.
    const FAKE_EMBED_DIM: usize = 384;

    // Simple mutex to mirror the initialization semantics of the real embedder.
    static FAKE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    /// Default Model2Vec checkpoint used by the production backend.
    pub const DEFAULT_MODEL_NAME: &str = "grybsh/m2v-MiniLM-L6-v2";

    /// Configuration accepted by the production Model2Vec backend.
    #[derive(Clone, Debug)]
    pub struct Model2VecConfig {
        /// Hugging Face model identifier or local model directory.
        pub model_name: String,
        /// Whether to normalize output vectors.
        pub normalize: Option<bool>,
        /// Maximum batch size.
        pub batch_size: usize,
        /// Optional model subdirectory.
        pub subfolder: Option<String>,
    }

    /// Initializes the deterministic test backend in place of Model2Vec.
    pub fn init_model2vec(_config: Model2VecConfig) -> Result<()> {
        init(1, 0)
    }

    /// Returns the test backend configuration.
    pub fn current_config() -> Option<EmbeddingConfig> {
        Some(EmbeddingConfig {
            backend: "fake".into(),
            model_name: None,
            normalize: None,
            batch_size: None,
        })
    }

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
// - Otherwise the Model2Vec backend is used.
#[cfg(any(test, feature = "fake-embed"))]
pub use fake::{current_config, init_model2vec, Model2VecConfig, DEFAULT_MODEL_NAME};
#[cfg(any(test, feature = "fake-embed"))]
pub use fake::{get_embedding, get_embeddings, init, shutdown};

#[cfg(not(any(test, feature = "fake-embed")))]
pub use real::current_config;
#[cfg(not(any(test, feature = "fake-embed")))]
pub use real::{get_embedding, get_embeddings, init, shutdown};

#[cfg(not(any(test, feature = "fake-embed")))]
pub use real::{init_model2vec, Model2VecConfig, DEFAULT_MODEL_NAME};
