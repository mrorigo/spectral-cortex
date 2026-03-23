# Plan: Add Similarity Scores to Long-Range Links

## Current State

The `detect_long_range_links()` function in `spectral.rs` returns `Vec<(u32, u32)>` - pairs of similar node IDs, but does NOT report the similarity score.

Current signature:
```rust
pub fn detect_long_range_links(
    spec: &Array2<f32>,
    emb_sim: &Array2<f32>,
    spectral_sim_thr: f32,
    embed_sim_thr: f32,
    note_ids: &[u32],
) -> Vec<(u32, u32)>
```

The function already computes `sp_sim` (spectral similarity) internally:
```rust
let sp_sim = if norm_i == 0.0 || norm_j == 0.0 { 0.0 } else { dot / (norm_i * norm_j) };
```

But it only uses it for filtering via `spectral_sim_thr` and discards the value.

## Goals

1. **Return similarity scores** - Include the spectral similarity (`sp_sim`) in the results
2. **Support limiting results** - Add ability to return only top-K most similar pairs
3. **Query-time control** - Allow CLI to specify how many links to return

## Proposed Changes

### 1. Modify `detect_long_range_links()` return type

Change from:
```rust
Vec<(u32, u32)>
```

To:
```rust
Vec<(u32, u32, f32)>  // (note_id_a, note_id_b, spectral_similarity)
```

### 2. Add optional top-K parameter

```rust
pub fn detect_long_range_links(
    spec: &Array2<f32>,
    emb_sim: &Array2<f32>,
    spectral_sim_thr: f32,
    embed_sim_thr: f32,
    note_ids: &[u32],
    top_k: Option<usize>,  // NEW: limit results
) -> Vec<(u32, u32, f32)>
```

When `top_k` is `Some(k)`, return only the k pairs with highest spectral similarity.

### 3. Update callers

- `build_spectral_structure()` in `graph/mod.rs` - update to handle new return type
- Store links with scores in `SpectralMemoryGraph` struct

### 4. Store in SMG

Add a new field to `SpectralMemoryGraph`:
```rust
pub long_range_links: Option<Vec<(u32, u32, f32)>>,
```

### 5. Add CLI support

Add `--links-k` argument to query command to limit returned links:
```bash
spectral-cortex query --query "mcp" --smg smg.json --links-k 10
```

### 6. Include in query results

When returning query results, optionally include the long-range links for matched notes.

## Implementation Order

1. Modify `detect_long_range_links()` to return scores
2. Add top-K filtering
3. Update `build_spectral_structure()` caller
4. Add field to `SpectralMemoryGraph` struct
5. Update serialization if needed
6. Add CLI argument
7. Test and verify

## Notes

- The embedding similarity (`emb_s`) could also be returned if useful
- Need to ensure stable ordering for deterministic results (sort by score descending, then by note_id)
- Consider memory implications for large graphs (storing all link scores vs. just top-K)