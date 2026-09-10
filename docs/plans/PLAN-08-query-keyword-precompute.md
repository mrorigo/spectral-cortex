# Precompute normalized keyword query state

## Objective

Compute query normalization once per retrieval instead of once per note.

## Scope

- Keyword matching in both candidate retrieval methods.
- Case-insensitive matching behavior.

## Implementation

1. Lowercase the query once before entering the parallel scoring closure.
2. Reuse that value for symbol and file matching.
3. Preserve Unicode behavior of `to_lowercase`.
4. Add a test proving identical scores for mixed-case metadata and query text.

## Acceptance criteria

- No query lowercasing occurs inside the per-note loop.
- Case-insensitive matching remains unchanged.
