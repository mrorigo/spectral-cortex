# Define and enforce retrieval score semantics

## Objective

Make semantic, keyword, cluster, and temporal scores obey one documented range and ranking contract.

## Scope

- `retrieve_candidates` and its filtered variant.
- `re_rank_with_temporal` integration.
- Minimum-score behavior.

## Implementation

1. Decide whether semantic similarity is clamped to `[0, 1]` or transformed from `[-1, 1]`.
2. Apply the chosen normalization before keyword and cluster boosts.
3. Define whether boosts are additive or multiplicative and cap the combined semantic score.
4. Apply the same contract in both retrieval implementations.
5. Add tests for negative cosine, zero norms, keyword matches, cluster boosts, and score thresholds.

## Acceptance criteria

- Every returned raw and final score has a documented range.
- Equal inputs produce identical ordering and scores across filtered and unfiltered paths.
- `min_score` has predictable behavior at the boundaries.
