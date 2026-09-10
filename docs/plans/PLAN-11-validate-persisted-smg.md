# Validate persisted SMG data

## Objective

Reject malformed or inconsistent JSON before it reaches query and spectral code.

## Scope

- `load_smg_json` and `validate_serial_smg`.
- Parallel source vectors, embeddings, IDs, labels, centroids, and links.

## Implementation

1. Reject duplicate note IDs.
2. Require equal lengths for source turn IDs, commit IDs, and timestamps, or normalize an explicitly supported legacy format.
3. Validate finite embedding values, embedding dimensions, and stored norms.
4. Validate cluster-label length against the sorted note count.
5. Validate centroid dimensions and centroid norm consistency.
6. Validate that link endpoints refer to existing notes and that link scores are finite.
7. Add malformed-fixture tests for every rejected invariant.

## Acceptance criteria

- Invalid files return descriptive errors from `load_smg_json`.
- Valid files round-trip without changing query results.
- Loading malformed data never causes a later index panic.
