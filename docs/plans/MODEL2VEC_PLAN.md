# Plan: Adding Model2Vec Embeddings Support to Spectral Cortex
## Overview
Integrate model2vec-rs as an alternative embedding backend for Spectral Cortex, alongside the existing MiniLM embedder. This provides users with a choice between a more specialized model (model2vec) and the existing MiniLM while maintaining backward compatibility.
Phase 1: Architecture & Design
1.1 Plugin System Extension
Extend the embed module to support multiple embedding backends via a plugin pattern:
```rust
// In embed/mod.rs
pub trait Embedder: Send + Sync {
    fn embed(&self, text: &str) -> Vec<f32>;
    fn embed_batch(&self, texts: &[String]) -> Vec<Vec<f32>>;
    fn get_name(&self) -> &'static str;
}

// Implementations for MiniLM (default) and model2vec
pub mod embedder {
    pub mod miniLM;
    pub mod model2vec;
}
```
1.2 Feature Flag Design
Add configurable features to control which embedder is used:
```rust
#[cfg(feature = "model2vec-embedder")]
pub struct Model2VecConfig {
    pub model_name: &'static str,
    pub normalize: bool,
    pub batch_size: usize,
}

#[cfg(not(feature = "model2vec-embedder"))]
pub struct MiniLMConfig {
    pub model_path: Option<String>,
    pub normalize: bool;
}
```


Phase 2: Core Integration
2.1 Update Cargo.toml
Add model2vec-rs as an optional dependency:
```toml
[dependencies]
# ... existing dependencies ...

# Model2vec-rs (optional for embedder feature)
[features]
model2vec-embedder = [
    "model2vec-rs/hf-hub",
    "model2vec-rs/onig",
    "model2vec-rs/fancy-regex",
]

# Keep existing features
default = []
```

2.2 Implement Model2vec Embedder
Create a new embedder module that wraps model2vec-rs:
```rust
// In embed/model2vec.rs
use model2vec_rs::{model::StaticModel, EmbeddingPool};

pub struct Model2VecEmbedder {
    model: StaticModel,
    pool: EmbeddingPool,
}

impl Model2VecEmbedder {
    pub fn new(model_name: &str, normalize: bool, pool: EmbeddingPool) -> Result<Self> {
        let model = StaticModel::from_pretrained(model_name, None, Some(normalize), None)?;
        Ok(Model2VecEmbedder { model, pool })
    }

    pub fn embed(&self, text: &str) -> Vec<f32> {
        self.pool.embed_batch(vec![text.to_string()]).next().unwrap().to_vec()
    }

    pub fn embed_batch(&self, texts: &[String]) -> Vec<Vec<f32>> {
        self.pool.embed_batch(texts.to_vec()).into_iter().map(|arr| arr.into_iter().map(|v| v as f32).collect()).collect()
    }

    pub fn get_name(&self) -> &'static str { "model2vec" }
}
```

2.3 Integrate with SpectralMemoryGraph
Modify the embed module to support multiple backends:
```rust
// In lib.rs or embed/mod.rs
pub enum EmbedderType {
    MiniLM,
    Model2Vec {
        config: Model2VecConfig,
    },
}

pub struct SpectralEmbedder {
    embedder: Option<Box<dyn Embedder>>,
}

impl SpectralEmbedder {
    pub fn new(embedder_type: EmbedderType, config: Option<impl Into<impl EmbedderType>>) -> Result<Self> {
        match embedder_type {
            EmbedderType::MiniLM => MiniLMEmbedder::new(config)?,
            EmbedderType::Model2Vec { config } => Model2VecEmbedder::new(
                config.model_name,
                config.normalize,
                get_embedder_pool()?, // Get or create pool
            )?,
        }
    }

    pub fn embed(&self, text: &str) -> Vec<f32> {
        self.embedder.as_ref().unwrap().embed(text)
    }

    pub fn embed_batch(&self, texts: &[String]) -> Vec<Vec<f32>> {
        self.embedder.as_ref().unwrap().embed_batch(texts)
    }
}
```

Phase 3: Configuration & CLI
3.1 CLI Arguments
Add new arguments to the ingest command:
```rust
#[derive(Args, Debug)]
pub struct Model2VecIngestArgs {
    #[arg(long, value_name = "MODEL_NAME")]
    pub model_name: String,

    #[arg(long)]
    pub normalize: Option<bool>,

    #[arg(long, default_value = "1024")]
    pub batch_size: usize,

    #[arg(long)]
    pub subfolder: Option<String>,
}

impl Default for Model2VecIngestArgs {
    fn default() -> Self {
        Self {
            model_name: "minishlab/potion-base-8M".to_string(),
            normalize: Some(true),
            batch_size: 1024,
            subfolder: None,
        }
    }
}
```

3.2 Environment Variables
Support configuration via environment variables for easy deployment:
```rust
#[cfg(any(test, env("MODEL2VEC")))]
pub fn get_model2vec_config() -> Option<Model2VecConfig> {
    Option::from_else({
        std::env::var("MODEL2VEC_MODEL_NAME")
            .map(|name| Model2VecConfig {
                model_name: name,
                normalize: Some(std::env::var("MODEL2VEC_NORMALIZE").is_ok()),
                batch_size: 128,
                subfolder: None,
            })
    })
}
```
Phase 4: Testing & Validation
4.1 Unit Tests
Test model2vec-rs integration:
```rust
#[cfg(test)]
mod tests {
    use super::embedder::Model2VecEmbedder;

    #[test]
    fn test_model2vec_embedding() {
        let pool = EmbeddingPool::new(test_config)?;
        let model = Model2VecEmbedder::new("minishlab/potion-base-8M", true, pool).unwrap();
        
        let embedding = model.embed("test sentence");
        assert!(embedding.len() == 384);
        assert!(embedding[0] >= -1.0 && embedding[0] <= 1.0);
        
        // Test batch
        let embeddings = model.embed_batch(&vec!["hello".to_string(), "world".to_string()]);
        assert_eq!(embeddings.len(), 2);
    }
}
```

4.2 Integration Tests
Test end-to-end ingestion with model2vec:
```rust
#[cfg(test)]
mod integration {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_ingest_with_model2vec() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        // Initialize git repo
        // ...

        // Ingest with model2vec
        let mut graph = SpectralMemoryGraph::new().unwrap();
        graph.ingest_repository(repo_path, "test", None, None, None)?;
        
        // Verify structure
        assert!(graph.spectral_structure().is_ok());
    }
}
```

Phase 5: Performance Optimization
5.1 Batch Processing
Model2vec-rs is designed for efficient batch processing:
```rust
impl Model2VecEmbedder {
    pub fn embed_batch(&self, texts: &[String]) -> Vec<Vec<f32>> {
        // Use model2vec's efficient batching
        let embeddings = self.pool.embed_batch(texts.iter().map(|s| s.as_str()).collect());
        embeddings.into_iter().map(|arr| arr.into_iter().map(|v| v as f32).collect()).collect()
    }
}
```

5.2 Caching Strategy
Implement smart caching:
```rust
pub struct Model2VecCache {
    embeddings: HashMap<String, Vec<f32>>,
    cache_size: usize,
}

impl Model2VecCache {
    pub fn get_or_create(&mut self, text: &str) -> Vec<f32> {
        if let Some(embedding) = self.embeddings.get(text) {
            // Check cache size limit
            if self.size() >= self.max_size() {
                self.evict_lru();
            }
            return embedding.clone();
        }
        
        let embedding = self.generate(text)?;
        self.insert(text.to_string(), embedding);
        embedding
    }
}
```

Phase 6: Documentation & Examples
6.1 CLI Usage
Update documentation with new options:

```markdown
# Using model2vec with custom configuration
spectral-cortex ingest \
  --repo /path/to/repo \
  --out smg.json \
  --model-name minishlab/potion-base-8M \
  --normalize true \
  --batch-size 512
```
  
6.2 MCP Integration
Add model2vec support to MCP server:
```rust
// In mcp_server.rs
fn run_mcp_server(smg_path: &str) -> Result<()> {
    let smg = load_smg_json(smg_path)?;
    
    // Create embedder based on config
    let embedder = if let Some(config) = smg.get_config() {
        if config.embedder_type == "model2vec" {
            Model2VecEmbedder::new(
                config.model_name.to_string(),
                config.normalize,
                get_embedder_pool()?, // Get or create pool
            )?
        } else {
            MiniLMEmbedder::new(&config.model_path)?
        }
    } else {
        // Default to MiniLM
        MiniLMEmbedder::new(None, true)?
    };
    
    // Use embedder instead of default embedder
    ...
}
```

Phase 7: Performance Benchmarks
7.1 Benchmark Results
Expected performance characteristics:
Model	Embedding Size
MiniLM-Large	384 dims
model2vec-small	384 dims
model2vec-base	384 dims

7.2 Optimization Opportunities
- Use quantized model2vec (q4) for faster inference
- Implement batching for batch embedding operations
- Add GPU support for model2vec (CUDA)

Phase 8: Migration Path
8.1 Backward Compatibility
Maintain full backward compatibility:
```rust

// Old: embed::get_embedding("text") -> Vec<f32>
// New: embed::get_embedding("text") -> Vec<f32> (same API)
// But internally uses MiniLM by default

8.2 Feature Flag Degradation
If model2vec-rs is not enabled, fall back to MiniLM automatically:
```rust
#[cfg(not(feature = "model2vec-embedder"))]
impl SpectralEmbedder {
    fn new(_type: EmbedderType, _config: Option<_>) -> Self {
        MiniLMEmbedder::new(None, true).unwrap()
    }
}
```

Phase 9: Rollout Plan
9.1 Versioning Strategy
- Major version: Breaking change (new embedder API)
- Minor version: Backward compatible additions
- Patch version: Bug fixes
9.2 Deployment Considerations
- Provide clear upgrade instructions
- Document model download requirements for model2vec
- Consider caching model files to reduce initial download time
9.3 Monitoring & Observability
- Track which embedder is used per deployment
- Monitor embedding quality metrics
- Log embedding generation time and size
Phase 10: Future Enhancements
10.1 Model Selection Framework
Allow users to select from multiple models:
```rust
pub struct ModelSelector {
    models: HashMap<String, Box<dyn Embedder>>,
}

impl ModelSelector {
    pub fn register(&mut self, name: &str, embedder: Box<dyn Embedder>) {
        self.models.insert(name.to_string(), embedder);
    }

    pub fn select(&self, name: &str) -> Option<&dyn Embedder> {
        self.models.get(name)
    }
}
```

10.2 Dynamic Model Loading
Support runtime model switching for A/B testing:
```rust
pub struct DynamicModelLoader {
    models: HashMap<String, Box<dyn Embedder>>,
}

pub fn load_model(name: &str) -> Result<Box<dyn Embedder>> {
    // Lazy load from filesystem or network
}
```

## Risk Mitigation
Risk 1: Model2vec compatibility issues
Mitigation: Test against multiple model versions, provide fallback to MiniLM
Risk 2: Performance regression
Mitigation: Benchmark before/after, provide performance metrics, allow gradual rollout
Risk 3: Memory usage
Mitigation: Implement smart caching, monitor memory usage, provide memory-efficient options
Risk 4: Model quality inconsistency
Mitigation: Provide multiple model options, allow users to choose by performance/quality tradeoffs

## Success Criteria
5. Users can optionally use model2vec-rs for better semantic quality
6. No breaking changes to existing API
7. Performance meets or exceeds MiniLM baseline
8. Clear documentation and examples
9. Active maintenance and monitoring in place
This plan provides a comprehensive approach to integrating model2vec-rs while maintaining backward compatibility and offering users flexibility in their embedding backend choice.
