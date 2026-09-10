# Construct sparse similarity without a dense matrix

## Objective

Avoid the dense `n × n` similarity allocation for production-scale graphs.

## Scope

- Spectral build similarity generation.
- Thresholding and structural-link fusion.
- Numerical behavior for sparse and disconnected graphs.

## Implementation

1. Define the supported graph size and memory target for the new path.
2. Generate only candidate neighbors using blockwise exact search or an approved approximate nearest-neighbor index.
3. Apply the similarity threshold before storing edges.
4. Add structural edges without requiring a dense intermediate.
5. Emit a symmetric CSR matrix with deterministic row ordering.
6. Compare eigenvalues, clusters, and links with the existing implementation on small fixtures.
7. Benchmark memory peak and build time on 1k, 10k, and the project’s largest available fixture.

## Acceptance criteria

- Peak memory no longer scales as `O(n²)` for the sparse path.
- Small-graph results match the dense reference within documented tolerance.
- Disconnected and zero-norm nodes remain valid inputs.
