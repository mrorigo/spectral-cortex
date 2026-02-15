# Spectral Cortex User Guide

## Audience

This guide is for developers and agent builders using Spectral Cortex to ingest git history, build an SMG (Spectral Memory Graph), inspect and edit it, and query it for retrieval.

## What Spectral Cortex Does

Spectral Cortex turns commit history into a graph of semantically related notes.

1. It reads commits.
2. It optionally filters noisy commit-message lines.
3. It can split multi-change commit messages into multiple note segments.
4. It embeds each segment.
5. It builds spectral structures and long-range links.
6. It saves results to SMG JSON (`format_version: spectral-cortex-2`).
7. It supports query and note inspection from saved SMG files.

## Core Concepts

`Note`

1. A note is the primary unit in the graph.
2. One commit can produce one or many notes (depending on split settings).

`related_note_links`

1. Per-note adjacency list with scores.
2. Each entry is `[related_note_id, spectral_similarity]`.
3. This is now the canonical related-link representation.

`long_range_links`

1. Global list of links where spectral similarity is high but embedding similarity is low.
2. Each entry is `[note_id_a, note_id_b, spectral_similarity]`.

`Temporal Re-ranking`

1. Query-time recency boost combined with semantic score.
2. Controlled with `--no-temporal`, `--temporal-weight`, `--temporal-mode`, and `--temporal-half-life-days`.

## Install and Build

From repo root:

```bash
cargo build --release -p spectral-cortex
```

Binary path:

```bash
./target/release/spectral-cortex
```

## Command Overview

```bash
spectral-cortex <COMMAND>
```

Commands:

1. `ingest`: Build/update SMG from git history.
2. `update`: Incremental alias for append ingestion.
3. `query`: Retrieve relevant notes from SMG JSON.
4. `note`: Inspect one note and related links.
5. `mcp`: Run an MCP stdio server with a preloaded SMG file.

## MCP

Run MCP server over stdio using a preloaded graph:

```bash
./target/release/spectral-cortex mcp --smg smg.json
```

Options:

1. `--smg <PATH>`: path to SMG JSON file to preload once at startup.

MCP tool inputs (no `smg_path` required because graph is preloaded):

`graph_summary`

```json
{}
```

`query_graph`

```json
{
  "query": "mcp protocol",
  "top_k": 5,
  "links_k": 3
}
```

`inspect_note`

```json
{
  "note_id": 5071,
  "links_k": 10
}
```

`long_range_links`

```json
{
  "top_k": 20
}
```

## Ingest

### Basic

```bash
./target/release/spectral-cortex ingest \
  -r /path/to/repo \
  --out smg.json
```

### Options

`--repo <PATH>`

1. Git repository path.
2. Default `.`.

Spectral structures are always rebuilt during `ingest` and `update`.

`--out <PATH>`

1. Output SMG JSON path.
2. If omitted, runs in-memory only.

`--append`

1. Load existing `--out` SMG and append new ingest results.

`--incremental`

1. Skip commits already present (by commit id).
2. Requires `--out`.
3. Best paired with `--append`.

`--max-commits <N>`

1. Limit scanned commits.
2. Useful for smoke tests and tuning.

`--workers <N>`

1. Embedding worker count.
2. Default `4`.

`--cache-size <N>`

1. Cache size per embedding worker.
2. In ingest help text, default shown as `100`.

`--git-filter-preset git-noise`

1. Built-in message-line cleanup preset.

`--git-filter-drop <REGEX>`

1. Repeatable custom message-line drop regex.

`--git-filter-case-insensitive`

1. Apply case-insensitive regex matching for filters.

`--git-commit-split-mode <off|auto|strict>`

1. `off`: one note per commit message after filtering.
2. `auto`: split only when parser confidence threshold is met.
3. `strict`: split whenever parser detects boundaries.

`--git-commit-split-max-segments <N>`

1. Hard cap for segments emitted per commit.
2. Prevents pathological explosions.

`--git-commit-split-min-confidence <0..1>`

1. Confidence threshold used in `auto` mode.

### Ingest Output You’ll See

Typical output includes:

1. Filter summary: commits seen/kept/skipped, dropped lines, char retention.
2. Split summary: commits split, segments emitted, parser-mode distribution.
3. Final SMG summary: note count and cluster-label presence.

## Update (Incremental Alias)

`update` maps to ingest with append+incremental semantics.

```bash
./target/release/spectral-cortex update \
  -r /path/to/repo \
  -o smg.json
```

Update-specific notes:

1. `--out` is required.
2. Split settings are available here too.

## Query

### Basic

```bash
./target/release/spectral-cortex query \
  --query "dependency injection" \
  --smg smg.json \
  --json \
  --top-k 10
```

### Options

`--query <TEXT>`

1. Search text.

`--smg <PATH>`

1. SMG JSON file path.

`--top-k <N>`

1. Final result count.
2. Default `5`.

`--candidate-k <N>`

1. Candidate pool before final filtering.
2. Default behavior is `top_k * 5`.

`--min-score <FLOAT>`

1. Inclusive threshold on final score.
2. Default `0.7`.

`--no-temporal`

1. Disable temporal re-ranking.

`--temporal-weight <0..1>`

1. Blend weight of temporal score.
2. Higher values favor recency more strongly.

`--temporal-mode <exponential|linear|step|buckets>`

1. Temporal scoring function.

`--temporal-half-life-days <FLOAT>`

1. Exponential decay half-life in days.

`--temporal-now <RFC3339>`

1. Override current time for reproducible experiments/tests.

`--time-start <RFC3339>`

1. Filter out notes earlier than this.

`--time-end <RFC3339>`

1. Filter out notes later than this.

`--time-window-days <FLOAT>`

1. Alternative rolling window filter.

`--workers <N>`

1. Embedding workers for query embedding.

`--cache-size <N>`

1. Query-time embed cache size.

`--links-k <N>`

1. Limit long-range links/related links in output.

### Query JSON Shape

Top-level fields include:

1. `query`
2. `results`
3. `long_range_links`
4. timing/diagnostic fields depending on output path

Per-result note info includes scored related links under `related_notes`.

## Note Inspect

### Basic

```bash
./target/release/spectral-cortex note \
  --smg smg.json \
  --note-id 42 \
  --json
```

Options:

1. `--smg <PATH>`
2. `--note-id <ID>`
3. `--links-k <N>`
4. `--json`

## SMG JSON Format (Current)

Current format is strict and versioned:

1. `metadata.format_version` must be `spectral-cortex-2`.
2. Legacy formats are rejected.

`notes[*]` includes:

1. `note_id`
2. `raw_content`
3. `context`
4. `embedding`
5. `norm`
6. `source_turn_ids`
7. `source_commit_ids`
8. `source_timestamps`
9. `related_note_links`

`related_note_links` example:

```json
"related_note_links": [
  [930, 0.8873],
  [333, 0.8690]
]
```

## UI Guide (`ui/`)

Load an SMG JSON and inspect/edit visually.

### Current behavior highlights

1. Graph edges use score-normalized thickness/intensity.
2. Force link distance/strength is score-aware.
3. Node hover cards show:
   - note id
   - node role/kind
   - snippet
   - short commit ids
   - timestamp summaries
4. Right-click node pins a card.
5. Multiple pinned cards are supported.
6. Right-click pinned node again unpins that card.
7. Left-click still selects the node.

### Related links editing format

Editor field expects:

1. `id:score` pairs, comma/space/newline separated.
2. Plain `id` is accepted and treated as score `0`.

Examples:

```text
930:0.887, 333:0.869
6711:0.74
6057
```

## Library API: Spectral Build Tuning

New: tunable spectral construction config.

Type:

1. `SpectralBuildConfig`

Fields:

1. `num_spectral_dims`
2. `adj_sparse_threshold`
3. `spectral_link_similarity_threshold`
4. `embed_link_similarity_threshold`
5. `max_clusters`
6. `min_clusters`

Methods:

1. `build_spectral_structure(progress)`  
   Uses default `SpectralBuildConfig`.
2. `build_spectral_structure_with_config(progress, &config)`  
   Uses custom tuning.

Example:

```rust
use spectral_cortex::{SpectralBuildConfig, SpectralMemoryGraph};

let mut smg = SpectralMemoryGraph::new()?;
let config = SpectralBuildConfig {
    num_spectral_dims: 12,
    adj_sparse_threshold: 0.15,
    spectral_link_similarity_threshold: 0.72,
    embed_link_similarity_threshold: 0.45,
    max_clusters: 18,
    min_clusters: 2,
};
smg.build_spectral_structure_with_config(None, &config)?;
```

### Tuning guidance

`num_spectral_dims`

1. Higher can capture more structure but may increase noise/cost.
2. Start around `8-16`.

`adj_sparse_threshold`

1. Higher removes weaker semantic edges early.
2. Too high can fragment graph neighborhoods.

`spectral_link_similarity_threshold`

1. Higher yields fewer, stronger long-range links.

`embed_link_similarity_threshold`

1. Lower enforces stronger “semantic distance” for long-range links.

`min_clusters` and `max_clusters`

1. Bounds around eigengap-selected cluster count.
2. Use narrower bounds for more stable cluster cardinality across runs.

## Recommended Workflows

### First-time build

```bash
./target/release/spectral-cortex ingest \
  -r /path/to/repo \
  --out smg.json \
  --git-filter-preset git-noise \
  --git-filter-drop '^Merge pull request' \
  --git-filter-case-insensitive
```

### Daily incremental update

```bash
./target/release/spectral-cortex update \
  -r /path/to/repo \
  -o smg.json \
  --git-filter-preset git-noise
```

### Investigation query

```bash
./target/release/spectral-cortex query \
  --query "why was dependency injection added" \
  --smg smg.json \
  --json \
  --top-k 10 \
  --links-k 8 \
  --min-score 0.6
```

## Troubleshooting

`SMG format error`

1. If load fails with unsupported format version, regenerate using current binary.

`No/poor query hits`

1. Lower `--min-score`.
2. Increase `--candidate-k`.
3. Try `--no-temporal` for time-agnostic retrieval.

`Unexpected commit splitting`

1. Set `--git-commit-split-mode off` to compare baseline.
2. Increase/decrease `--git-commit-split-min-confidence`.
3. Adjust `--git-commit-split-max-segments` for commit style.

`Performance`

1. Tune `--workers` to machine CPU.
2. Use `--max-commits` during experimentation.
3. Use incremental update in normal operation.

## Compatibility and Stability Notes

1. JSON format is strict (`spectral-cortex-2`).
2. `related_note_links` is the canonical per-note adjacency representation.
3. `related_note_ids` has been removed from graph storage/output.
