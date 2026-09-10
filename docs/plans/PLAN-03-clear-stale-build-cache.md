# Clear stale spectral state on rebuild

## Objective

Ensure every spectral build leaves cached state consistent with the current notes.

## Scope

- Handle graphs with fewer than three notes.
- Handle failed rebuilds without exposing a partially new and partially old graph.

## Implementation

1. Clear spectral caches before the small-graph early return.
2. Define whether a failed build preserves the previous valid state or clears it; use one policy consistently.
3. Prefer building into local values and committing all caches after every stage succeeds.
4. Add tests that build a large graph, remove notes, rebuild, and assert all caches are empty for a small graph.
5. Add a failure-path test for invalid configuration or decomposition failure.

## Acceptance criteria

- A build for fewer than three notes has no stale matrix, embeddings, labels, centroids, or links.
- Failed builds do not expose inconsistent cache combinations.
