# Reduce per-note query allocations

## Objective

Remove repeated embedding clones and temporary `Array1` allocations from query scoring.

## Scope

- Unfiltered and filtered candidate retrieval.
- Query correctness must remain unchanged.

## Implementation

1. Keep the query embedding in a reusable slice or view.
2. Compute dot products directly over embedding slices, or use a shared normalized representation.
3. Avoid cloning each note embedding during scoring.
4. Benchmark representative graph sizes before and after the change.
5. Add a correctness test comparing old/reference cosine calculations with the optimized path.

## Acceptance criteria

- The hot scoring loop performs no per-note embedding clone.
- Retrieval ranking remains unchanged within floating-point tolerance.
- The benchmark records allocation and latency changes for 1k and 10k notes.
