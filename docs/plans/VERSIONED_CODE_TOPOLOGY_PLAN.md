# Plan: Versioned Code Topology

## Objective

Add a third graph dimension for code topology without confusing it with
history-links or semantic links. Topology must describe the code as it existed
at a particular commit, so callers can inspect call-graph evolution rather
than only the repository's current state.

## Definitions

- **History-link:** a relationship between notes that describe revisions of the
  same qualified symbol identity.
- **Topology edge:** a typed relationship between symbols in one code snapshot
  (`call` or `import` in the first release).
- **Topology snapshot:** the symbols and topology edges extracted from one
  commit tree, identified by commit ID and commit timestamp.
- **Semantic link:** an embedding-cosine relationship between notes.
- **Spectral link:** a relationship inferred from the spectral graph transform.

These are distinct persisted concepts and must remain distinct in the UI,
MCP output, filtering, and documentation.

## Grounded current state

The codebase already has:

1. Qualified note identities and `history_links`.
2. A top-level `topology_snapshots` field in the SMG model.
3. A Tree-sitter topology collector that walks a commit tree and extracts
   declarations and local call relationships.
4. Initial UI rendering for the latest available topology snapshot.

The remaining work is to make the snapshot model authoritative, complete the
commit-history workflow, and remove the old note-level `topology_links`
assumption.

## Design decisions

1. **Topology is versioned.** Never parse only repository HEAD when ingesting
   history. For every ingested commit with supported source files, parse that
   commit's tree using the repository object database.
2. **Breaking schema rename is accepted.** The old `structural_links` field is
   not read as topology. Persisted output uses a bumped format version and
   tells users to regenerate older SMGs.
3. **First edge types are calls and imports.** External, unresolved, and
   ambiguous references are omitted rather than represented as false edges.
4. **Edges are symbol-to-symbol.** File-level edges are not a substitute for
   a call graph. Import edges may conservatively connect symbols across files
   when the language syntax and module path resolve unambiguously.
5. **Snapshots are immutable and deduplicated by commit ID.** Re-running or
   incrementally extending ingestion must not duplicate snapshots or edges.
6. **Selection is explicit.** The UI and MCP must identify which commit
   snapshot is being inspected; “latest” is a convenience default, not an
   implicit claim that all historical topology is current.

## Implementation phases

### Phase 1 — Make the persisted model authoritative

- Remove note-level `topology_links` from `SMGNote` and its serializable form.
- Keep `TopologySnapshot`, `TopologySymbol`, and `TopologyEdge` as the sole
  persisted topology representation.
- Add snapshot validation: unique commit IDs, unique symbol IDs within a
  snapshot, valid edge endpoints, and supported relation values.
- Bump the SMG format version and return an actionable regeneration error for
  pre-topology files.
- Preserve deterministic ordering of snapshots, symbols, and edges.

### Phase 2 — Extract topology per commit

- Collect the unique commit IDs represented by ingested turns.
- Parse each commit's tree, never the checkout working tree.
- Extract declarations with qualified, path-aware identities.
- Extract local calls and unambiguous imports for Rust, Python, JavaScript,
  JSX, TypeScript, and TSX as supported by the existing parser registry.
- Resolve relative modules and language-specific module paths conservatively.
- Skip external and ambiguous targets; record no invented topology edge.
- Add fixture tests covering calls, imports, renamed files, deleted files,
  merge commits, unresolved imports, and duplicate declarations.

### Phase 3 — Make ingestion incremental and idempotent

- Preserve snapshots already loaded from an existing SMG.
- Append only commit IDs not already represented, or deterministically rebuild
  the selected set when explicitly requested.
- Ensure repeated ingestion produces byte-stable topology output.
- Ensure a partial history does not silently replace previously persisted
  snapshots.

### Phase 4 — Expose historical topology

- Add a UI snapshot selector keyed by commit ID, timestamp, and short subject.
- Default to the newest snapshot, while clearly labeling that choice.
- Render topology edges with relation-specific labels and styling.
- Keep history-links, semantic links, spectral links, and topology edges
  independently filterable.
- Show useful empty states: no snapshots, no topology in the selected commit,
  and no matching relation.
- Extend MCP inspection/summary output with snapshot counts, selected commit,
  and typed topology edges without calling them structural links.

### Phase 5 — Spectral integration and documentation

- If topology influences spectral weighting, derive the adjacency from the
  selected/current snapshot rather than from note-local fields.
- Keep semantic embedding cosine and spectral cosine separately named and
  separately thresholded.
- Update the user guide, schema documentation, and regeneration guidance.
- Remove or clearly mark older plans that describe note-local structural links.

## Acceptance criteria

1. A two-commit fixture with a changed call site produces two distinct,
   selectable snapshots with different edges.
2. A historical call edge remains inspectable after the symbol or file changes
   in a later commit.
3. Incremental ingestion preserves old snapshots and does not duplicate them.
4. Invalid snapshot references fail validation with a useful error.
5. The UI and MCP distinguish history-links, semantic links, spectral links,
   call edges, and import edges.
6. No code path describes a HEAD-only topology graph as the repository's full
   call-graph history.

## Verification commands

```text
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
```

The topology fixture tests must also be run through the CLI ingestion path,
because direct collector tests alone cannot verify snapshot persistence and
incremental merge behavior.
