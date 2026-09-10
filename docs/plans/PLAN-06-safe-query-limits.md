# Validate and bound query limits

## Objective

Prevent arithmetic overflow and define behavior for zero and extreme query limits.

## Scope

- `top_k` handling in graph retrieval.
- MCP and CLI input boundaries where applicable.

## Implementation

1. Replace `top_k * 4` with checked or saturating arithmetic.
2. Return an empty result for `top_k == 0` without embedding the query.
3. Define a maximum accepted limit for externally supplied requests.
4. Return a clear validation error when the maximum is exceeded.
5. Add tests for zero, normal, maximum, and overflow-sized limits.

## Acceptance criteria

- No user-controlled limit can overflow.
- Zero-limit requests perform no unnecessary work.
- The same limit rules apply to direct, filtered, and MCP retrieval.
