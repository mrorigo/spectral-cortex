# Stream dense-to-CSR conversion

## Objective

Remove the temporary `Vec<Vec<(usize, f32)>>` allocation during CSR conversion.

## Scope

- `to_sparse`.
- No change to the input matrix or CSR semantics.

## Implementation

1. Count nonzero entries per row in parallel.
2. Compute row offsets from those counts.
3. Allocate final column and value arrays once.
4. Fill disjoint row ranges in parallel using safe slices.
5. Add tests for empty, diagonal-only, symmetric, and thresholded matrices.
6. Benchmark peak allocation and conversion time.

## Acceptance criteria

- Conversion does not retain a vector allocation for every row.
- CSR dimensions, offsets, columns, and values match the current implementation.
- The implementation remains safe and deterministic.
