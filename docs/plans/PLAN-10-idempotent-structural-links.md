# Make structural-link resolution idempotent

## Objective

Make repeated structural-link resolution produce the same graph as one invocation.

## Scope

- `resolve_structural_links`.
- Structural fusion input preparation.

## Implementation

1. Clear each note’s existing structural links before rebuilding them.
2. Deduplicate note IDs per symbol before generating links.
3. Sort generated links for deterministic persistence and tests.
4. Add a test that calls resolution twice and compares the complete link sets.

## Acceptance criteria

- Repeated resolution does not duplicate links.
- Structural links are symmetric in meaning and deterministic in order.
