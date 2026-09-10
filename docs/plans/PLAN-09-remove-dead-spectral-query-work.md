# Remove redundant query embedding and inactive spectral branch

## Objective

Remove duplicate model work and make the spectral-query behavior explicit.

## Scope

- `retrieve_with_scores_config`.
- Spectral polarity filtering documentation and tests.

## Implementation

1. Remove the second query embedding if spectral polarity remains inactive.
2. Replace the no-op branch with direct retrieval, or implement the documented projection algorithm as a separate feature.
3. Do not claim spectral polarity filtering is active until it changes candidate selection.
4. Add a test asserting one embedding operation per query path, using a test seam or fake embedder instrumentation.

## Acceptance criteria

- A query does not embed identical text twice.
- The code and documentation agree about whether spectral polarity filtering is active.
