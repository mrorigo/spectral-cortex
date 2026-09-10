# Replace recoverable panics with errors

## Objective

Remove `unwrap` and `expect` calls from recoverable library paths.

## Scope

- Embedding pool access and result handling.
- CSR construction.
- Similarity and ranking code.
- Spectral build invariants.

## Implementation

1. Replace lock poisoning unwraps with contextual errors.
2. Replace empty embedding result unwraps with an explicit error.
3. Propagate CSR construction errors instead of calling `expect`.
4. Replace partial-comparison unwraps with total ordering or explicit non-finite-score handling.
5. Replace the similarity-matrix expect with a local value or an error.
6. Add tests for empty results, poisoned/invalid inputs where testable, and non-finite scores.

## Acceptance criteria

- Recoverable input and state errors return `Result` errors.
- Remaining panics are documented as unreachable or reserved for process-fatal conditions.
- `cargo clippy --all --all-targets -- -D warnings` passes.
