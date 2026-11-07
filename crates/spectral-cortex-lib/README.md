# spectral-cortex-lib

`spectral-cortex-lib` is the Rust library for building and querying Spectral
Memory Graphs (SMGs). It provides graph construction, embeddings, temporal
ranking, retrieval, and JSON persistence. The `spectral-cortex` command-line
tool uses this library to index and search Git history.

## Install

Add the crate to your `Cargo.toml`:

```toml
[dependencies]
spectral-cortex-lib = "0.1.1"
```

Import its Rust API with the crate name `spectral_cortex`:

```rust
use spectral_cortex::{load_smg_json, save_smg_json, SpectralMemoryGraph};
```

See the [API documentation](https://docs.rs/spectral-cortex-lib) and the
[project repository](https://github.com/mrorigo/spectral-cortex) for examples
and implementation details.
