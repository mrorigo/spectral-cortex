# Safe parallel similarity matrix construction

## Objective

Replace the raw-pointer writer in `compute_fused_similarity_matrix` with a safe parallel implementation.

## Scope

- Preserve the symmetric cosine matrix and structural-link fusion behavior.
- Remove `unsafe` from `crates/spectral-cortex-lib/src/graph/spectral.rs`.
- Keep progress reporting and deterministic results within floating-point tolerance.

## Implementation

1. Compute each upper-triangle row into owned storage in parallel.
2. Assemble the symmetric `Array2` after worker completion, or use disjoint mutable row chunks with safe APIs.
3. Keep diagonal values and structural fusion semantics unchanged.
4. Add a regression test that compares the result with `cosine_similarity_matrix` for embeddings with and without structural links.
5. Run `cargo fmt --all -- --check`, `cargo clippy --all --all-targets -- -D warnings`, and `cargo test --all`.

## Acceptance criteria

- No `unsafe` remains in this matrix construction path.
- No data race or aliasing assumption is required by the implementation.
- The matrix is symmetric, has the expected diagonal, and matches the reference implementation within a documented tolerance.
