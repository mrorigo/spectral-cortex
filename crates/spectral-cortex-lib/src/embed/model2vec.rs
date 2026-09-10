//! Model2Vec embedding backend and bounded result cache.

use anyhow::Result;
use model2vec_rs::model::StaticModel;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

/// A bounded FIFO cache for text embeddings.
#[derive(Debug, Default)]
pub struct Model2VecCache {
    embeddings: HashMap<String, Vec<f32>>,
    order: VecDeque<String>,
    capacity: usize,
}

impl Model2VecCache {
    /// Creates a cache with the supplied maximum number of entries.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            ..Self::default()
        }
    }

    /// Returns a cached embedding, if present.
    pub fn get(&self, text: &str) -> Option<Vec<f32>> {
        self.embeddings.get(text).cloned()
    }

    /// Inserts an embedding and evicts the oldest entry when full.
    pub fn insert(&mut self, text: String, embedding: Vec<f32>) {
        if self.capacity == 0 {
            return;
        }
        self.embeddings.insert(text.clone(), embedding);
        self.order.push_back(text);
        while self.order.len() > self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.embeddings.remove(&oldest);
            }
        }
    }
}

/// A Model2Vec encoder with an optional bounded cache of single-text results.
pub struct Model2VecEmbedder {
    model: StaticModel,
    batch_size: usize,
    cache: Mutex<Model2VecCache>,
}

impl Model2VecEmbedder {
    /// Loads a Model2Vec model from a Hugging Face identifier or local directory.
    pub fn new(
        model_name: &str,
        normalize: Option<bool>,
        batch_size: usize,
        cache_capacity: usize,
        subfolder: Option<&str>,
    ) -> Result<Self> {
        if batch_size == 0 {
            anyhow::bail!("Model2Vec batch size must be greater than zero");
        }
        Ok(Self {
            model: StaticModel::from_pretrained(model_name, None, normalize, subfolder)?,
            batch_size,
            cache: Mutex::new(Model2VecCache::new(cache_capacity)),
        })
    }

    /// Encodes one text, consulting and updating the bounded cache.
    pub fn embed(&self, text: &str) -> Vec<f32> {
        let mut cache = match self.cache.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(embedding) = cache.get(text) {
            return embedding.clone();
        }
        let embedding = self
            .model
            .encode_with_args(&[text.to_string()], None, self.batch_size)
            .into_iter()
            .next()
            .unwrap_or_default();
        cache.insert(text.to_string(), embedding.clone());
        embedding
    }

    /// Encodes a batch of texts using Model2Vec's native batching.
    pub fn embed_batch(&self, texts: &[String]) -> Vec<Vec<f32>> {
        self.model.encode_with_args(texts, None, self.batch_size)
    }

    /// Returns the backend's stable name.
    pub const fn get_name(&self) -> &'static str {
        "model2vec"
    }
}

impl crate::embed::Embedder for Model2VecEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        Ok(Model2VecEmbedder::embed(self, text))
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(Model2VecEmbedder::embed_batch(self, texts))
    }

    fn get_name(&self) -> &'static str {
        Model2VecEmbedder::get_name(self)
    }
}

#[cfg(test)]
mod tests {
    use super::Model2VecCache;

    #[test]
    fn cache_evicts_oldest_entry() {
        let mut cache = Model2VecCache::new(2);
        cache.insert("a".into(), vec![1.0]);
        cache.insert("b".into(), vec![2.0]);
        cache.insert("c".into(), vec![3.0]);
        assert_eq!(cache.get("a"), None);
        assert_eq!(cache.get("b"), Some(vec![2.0]));
        assert_eq!(cache.get("c"), Some(vec![3.0]));
    }

    #[test]
    fn zero_capacity_does_not_retain_entries() {
        let mut cache = Model2VecCache::new(0);
        cache.insert("a".into(), vec![1.0]);
        assert_eq!(cache.get("a"), None);
    }
}
