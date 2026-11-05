# Spectral Cortex — Rust SMG for AI-Agent Memory (Git History)

Spectral Cortex is a compact Rust implementation of a Spectral Memory Graph (SMG) designed to be used as a short-term and long-term memory store for AI agents that reason over a project's git history. It converts commit messages and other short text chunks into embeddings, builds a spectral graph of semantic relationships, clusters related content, and exposes a retrieval API tuned for agent workflows.

This README is targeted at developers who want a local, explainable memory backing for AI agents that need to answer questions, recall past decisions, or link present context to repository history.

Highlights
- Purpose-built for agent memory over git history (commits, PR messages, notes).
- **AST-Aware Split Mode**: Use `tree-sitter` (Rust, TS, JS, Python) to map commit messages directly to code symbols.
- **Structural Fusion**: Fuses semantic similarity with the repo's call-graph and structural links.
- **Embedding Deduplication**: Batch-level de-duplication of identical content (e.g., multiple AST symbols in one commit) to drastically reduce embedding costs.
- **Hybrid Search**: Combines vector similarity with keyword-based metadata boosting for symbols and file paths.
- **Temporal Re-ranking**: Combines semantic similarity with configurable recency scoring.
- **Analysis Commands**: Inspect notes, structural hotspots, and symbol history from an SMG.

Contents
- Quick start
- MCP server (markdown-first tools)
- Agent-oriented workflows & examples
- CLI reference (important flags)
- Temporal re-ranking behavior (defaults & control)
- Library API & data model
- Persistence format
- Extensibility notes (hooks for agents)
- Testing & development
- Contributing and license

Quick start (developer)
-----------------------
Clone and build:

```bash
# Clone and build
git clone https://github.com/mrorigo/spectral-cortex.git
cd spectral-cortex
cargo build --release
```

Install from this repository (single binary with CLI + MCP subcommand):

```bash
cargo install --path crates/spectral-cortex-cli --force
```

Run with:

```bash
spectral-cortex --help
```

Ingest a repository and build the SMG (recommended CLI flow):

```bash
spectral-cortex ingest --repo /path/to/repo --out smg.json
```

Query the saved SMG programmatically (JSON output suitable for agents):

```bash
spectral-cortex query --query "why did we add X" --smg smg.json --json --top-k 10
```

MCP server (markdown-first tools)
----------------------------------
A dedicated MCP subcommand is available for agent workflows that need compact, markdown-first responses instead of verbose JSON.

Run it over stdio:
```bash
spectral-cortex mcp --smg smg.json
```

Available tools:
- `graph_summary`: compact graph metadata for an SMG file
- `query_graph`: semantic query with markdown tables and compact related-note summaries
- `inspect_note`: inspect one note and related notes with spectral similarity
- `long_range_links`: list top long-range links in markdown table format
- `get_structural_hotspots`: find the most frequently modified AST symbols (the "brittle" parts)
- `inspect_symbol_history`: deep dive into the chronological evolution of a specific structural symbol (class/function)

MCP client wiring example (recommended):
```json
{
  "mcpServers": {
    "spectral-cortex": {
      "command": "spectral-cortex",
      "args": ["mcp", "--smg", "/path/to/smg.json"]
    }
  }
}
```

If the binary is not on `PATH`, use an absolute path:
```json
{
  "mcpServers": {
    "spectral-cortex": {
      "command": "/absolute/path/to/spectral-cortex",
      "args": ["mcp", "--smg", "/absolute/path/to/smg.json"]
    }
  }
}
```

Development fallback (build+run from source each launch):
```json
{
  "mcpServers": {
    "spectral-cortex": {
      "command": "cargo",
      "args": ["run", "-p", "spectral-cortex", "--release", "--", "mcp", "--smg", "smg.json"],
      "cwd": "/Users/origo/src/spectral-cortex"
    }
  }
}
```

Tool input examples:

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

All MCP tool responses are markdown-first and intentionally compact to reduce token usage.
The server uses RMCP 3.2.0, supports MCP 2026-07-28 negotiation, and also
retains the legacy initialize lifecycle supported by the SDK. Successful tool
responses include the markdown plus structured content in the form
`{"markdown":"..."}`. The server is local-only over stdio and reads the SMG
file supplied at startup.

Agent-oriented workflows & examples
----------------------------------
The typical flow for an agent using the SMG as memory:

1. Periodic ingestion: run the ingest job (cron / CI hook) and persist `smg.json`.
2. At runtime, load `smg.json` once per agent process or cache it in memory.
3. For a user or agent query:
   - Get top-K relevant turn IDs and associated note metadata via the CLI or library API (JSON).
   - Retrieve the source commit ids, timestamps, and content snippets for context.
   - Use the returned snippets + candidate commit ids as evidence to feed into your agent's prompt or grounding layer.
4. Optionally: store agent feedback (relevance labels) externally for tuning ranking weights in future enhancements.

Why this is suited to agents
- Small and self-contained: you can run entirely on a developer machine or container.
- Model2Vec embeddings are used in production, with `grybsh/m2v-MiniLM-L6-v2` as the default checkpoint.
- Outputs structured JSON that an agent can parse to build prompts or context windows.
- Temporal re-ranking biases results toward recent, likely more actionable history — useful for agents that should prefer recent fixes or regression-causing commits.

CLI reference (important flags)
-------------------------------
The `spectral-cortex` binary exposes: `ingest`, `update`, `query`, `note`, `mcp`, `hotspots`, and `history`.

Ingest (collect commits -> SMG):
```bash
cargo run -p spectral-cortex --release -- ingest --repo /path/to/repo --out smg.json
```

Update (incremental append ingest; only new commits are embedded):
```bash
cargo run -p spectral-cortex --release -- \
  update --repo /path/to/repo --out smg.json --git-filter-preset git-noise
```

Query (default, temporal enabled):
```bash
cargo run -p spectral-cortex --release -- \
  query --query "refactor" --smg smg.json --json --top-k 10
```

Inspect one note:
```bash
cargo run -p spectral-cortex --release -- \
  note --smg smg.json --note-id 42 --json
```

Inspect structural activity and symbol history:
```bash
spectral-cortex hotspots --smg smg.json --top-k 10
spectral-cortex history --smg smg.json --symbol 'crate::module::function'
```

Run MCP server with preloaded SMG:
```bash
cargo run -p spectral-cortex --release -- \
  mcp --smg smg.json
```

`mcp` also accepts `--smd` as an alias for `--smg`.

Key query flags (agent-friendly):
- `--top-k <n>`: how many final results to return (default 5, maximum 10,000).
- `--candidate-k <n>`: how many candidates to retrieve before temporal reranking (defaults to `top_k * 5`).
- `--min-score <float>`: inclusive threshold applied to the combined `final_score` (default 0.6).
- `--no-temporal`: disable temporal re-ranking for this query (temporal is enabled by default).
- `--temporal-weight <0..1>`: control recency influence (default 0.20).
- `--temporal-half-life-days <float>`: half-life for exponential decay (default 14.0).
- `--keyword-weight <float>`: weight for hybrid metadata boosting (default 0.3).
- `--file <string>` and `--symbol <string>`: hard substring filters. Nonmatching notes are excluded before ranking.
- `--time-start`, `--time-end`, and `--time-window-days`: accepted by the CLI but currently not applied to query retrieval.
- `--json`: emit machine-readable JSON (recommended for agents).

Key ingest/update filtering flags:
- `--git-filter-preset git-noise`: drop common metadata lines (e.g. `Co-authored-by`, `Signed-off-by`).
- `--git-filter-drop <regex>`: repeatable custom line-drop regex.
- `--git-filter-case-insensitive`: case-insensitive regex matching.
- `--git-commit-split-mode <off|auto|strict|ast>`: split multi-change commit messages. `ast` uses tree-sitter.
- `--git-commit-split-max-segments <n>`: cap segments per commit.
- `--git-commit-split-min-confidence <0..1>`: confidence threshold for `auto`.
- `--num-spectral-dims <n>`: number of spectral dimensions (k) to compute (default 8).
- `--min-clusters <n>`: minimum clusters allowed (default 2).
- `--max-clusters <n>`: maximum clusters allowed (default 8).

Retrieval raw semantic scores and final temporal scores are bounded to `[0, 1]`. Negative cosine similarities score as `0`. Keyword and cluster boosts are multiplicative and capped at `1`.

Git hook automation (post-commit)
---------------------------------
For local agent memory that stays fresh automatically, wire the `update` command into a git `post-commit` hook.

Example `.git/hooks/post-commit`:
```bash
#!/usr/bin/env bash
set -euo pipefail

spectral-cortex update \
  --repo . \
  --out smg.json \
  --git-filter-preset git-noise
```

Make it executable:
```bash
chmod +x .git/hooks/post-commit
```

Temporal re-ranking (defaults & rationale)
-----------------------------------------
Temporal re-ranking is enabled by default because agents typically benefit from fresher context when interpreting repository state. The default strategy is:

- Mode: exponential decay
- Weight: 0.20 (20% recency influence)
- Half-life: 14 days

Combination formula (final score):
final = (1 - weight) * semantic_score + weight * temporal_score

Notes:
- Missing timestamps are treated as very old (temporal_score = 0).
- `--no-temporal` disables temporal scoring when you need canonical, time-agnostic retrieval.
- `--min-score` is applied to `final_score`, so agent clients can filter noisy candidates consistently.

`ingest` and `update` always rebuild spectral structures after ingesting turns.

Spectral Building Intuition
---------------------------
When tuning the spectral structure (clusters and dimensions), consider the following:

- **Architectural Match**: `max_clusters` should roughly correspond to the number of top-level modules or logical "areas" in your codebase. For a medium-sized project, 5–15 clusters is usually a good starting point.
- **Spectral Resolution**: `num_spectral_dims` (k) defines the dimensionality of the spectral embedding before clustering. It should typically be greater than or equal to `max_clusters`. Higher values capture more structural nuance but can introduce noise.
- **Eigengap Heuristic**: The internal algorithm uses the "eigengap" (the largest jump between sorted eigenvalues) to choose the optimal cluster count within your `min`/`max` bounds. A strong gap indicates a "natural" partition in the semantic/structural graph.
- **Stickiness**: Settings used during `ingest` are stored in the SMG JSON metadata and automatically reused during `update` unless overridden by CLI flags.

Library API & data model
------------------------
Use the library if you embed the SMG directly inside an agent process.

Primary types:
- `SpectralMemoryGraph`
  - `new() -> Result<Self>`: initializes embedder and structures.
  - `ingest_turn(&mut self, turn: &ConversationTurn) -> Result<()>`: add a turn.
  - `build_spectral_structure(&mut self, progress: Option<ProgressCallback>) -> Result<()>`: compute spectral embeddings and clusters.
  - `retrieve_candidates(&self, query: &str, candidate_note_k: usize, file_filter: Option<&str>, symbol_filter: Option<&str>, keyword_weight: f32) -> Result<Vec<Candidate>>`: return bounded raw semantic candidates.
  - `retrieve_with_scores(&self, query: &str, top_k: usize) -> Result<Vec<(u64, f32)>>`: return final per-turn scores using default temporal settings.

- `ConversationTurn`
  ```rust
  pub struct ConversationTurn {
      pub turn_id: u64,
      pub speaker: String,
      pub content: String,
      pub topic: String,
      pub entities: Vec<String>,
      pub commit_id: Option<String>,
      pub timestamp: u64, // unix epoch seconds
      pub symbol_id: Option<String>,
      pub ast_node_type: Option<String>,
      pub file_path: Option<String>,
  }
  ```

- `SMGNote`
  - Internal note stored per embedded turn; includes:
    - `raw_content`
    - `embedding: Vec<f32>`
    - `source_turn_ids: Vec<u64>`
    - `source_commit_ids: Vec<Option<String>>`
    - `source_timestamps: Vec<u64>`
    - `symbol_id: Option<String>`
    - `ast_node_type: Option<String>`
    - `file_path: Option<String>`
    - `related_note_links: Vec<(u32, f32)>`

Persistence format
------------------
The JSON format is strict and versioned (`metadata.format_version = "spectral-cortex-v1"`).

Persistence
-----------
SMG persistence uses a compact JSON representation (see `src/lib.rs` helpers):

```rust
// Save
save_smg_json(&smg, Path::new("smg.json"))?;

// Load
let smg = load_smg_json(Path::new("smg.json"))?;
```

The persisted structure stores notes in stable sorted order, optional cluster labels, and centroids. Spectral matrices are not persisted (they are recomputable via `build_spectral_structure()`).

Extensibility & agent hooks
---------------------------
- Retrieval diagnostics: query JSON includes per-result `score`, `turn_id`, `note_id`, `related_notes`, and where available `commit_id` and `cluster_label`. Top-level JSON includes `temporal` settings used for the query.
- Re-ranking: you can override the default re-ranker by calling `re_rank_with_temporal` with a custom `TemporalConfig` (weight, half-life, mode).
- Incremental ingestion: `ingest_turn` appends turns — you can build an ingestion pipeline that streams new commits into a long-running agent process.
- Feedback loop: collect agent judgments (useful/not useful) in a separate store and use those signals to adjust `temporal_weight` or to implement a learned ranker later.

Testing & development
---------------------

Model2Vec embeddings
--------------------
Model2Vec is the sole production embedding backend. The default checkpoint is
`grybsh/m2v-MiniLM-L6-v2`; select another Hugging Face model or local model
directory with `--model-name`:

```bash
cargo run -p spectral-cortex -- ingest --repo .
cargo run -p spectral-cortex -- query --smg smg.json --query "search text"
```

`MODEL2VEC_MODEL_NAME` and `MODEL2VEC_NORMALIZE` can be used instead of the
corresponding model-name and normalize options. Model2Vec models may be a
Hugging Face identifier or a local model directory. The selected model must be
the same for ingestion and querying; its embedding dimension is part of the
SMG's compatibility contract.

- Run unit tests:
```bash
cargo test -p spectral-cortex
```
- Tests use the deterministic fake embedder with the `fake-embed` feature.
- Linting & formatting:
```bash
cargo fmt
cargo clippy -- -D warnings
```

Developer notes
---------------
- Model2Vec loads checkpoints from the Hugging Face Hub on first use, or from a local model directory.
- Default settings assume agents should prefer recent context; tune via CLI or library `TemporalConfig` for domain needs (e.g., security audits vs. active feature work).
- If you plan to serve the SMG from a shared service, snapshot `smg.json` and load it into worker processes to avoid repeated rebuilds.

Contributing
------------
If you improve retrieval, temporal defaults, or add learning-to-rank, please:
1. Fork and create a feature branch.
2. Add unit tests and integration tests for retrieval ordering and temporal logic.
3. Open a PR describing the change and expected agent behavior.

License
-------
MIT. See `LICENSE` for details.
