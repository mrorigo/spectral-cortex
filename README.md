# Spectral Cortex — Rust SMG for AI-Agent Memory (Git History)

Spectral Cortex is a compact Rust implementation of a Spectral Memory Graph (SMG) designed to be used as a short-term and long-term memory store for AI agents that reason over a project's git history. It converts commit messages and other short text chunks into embeddings, builds a spectral graph of semantic relationships, clusters related content, and exposes a retrieval API tuned for agent workflows.

This README is targeted at developers who want a local, explainable memory backing for AI agents that need to answer questions, recall past decisions, or link present context to repository history.

Highlights
- Purpose-built for agent memory over git history (commits, PR messages, notes).
- Small, dependency-light Rust codebase with no heavy ML runtime at inference time.
- Default-enabled temporal re-ranking to prefer recent, relevant items (opt-out available).
- CLI workflows for ingesting repositories, persisting SMGs, and querying with JSON output for programmatic agents.

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
Clone and build; the project bundles the MiniLM embedder so you can run locally:

```bash
# Clone and build
git clone https://github.com/your-org/spectral-memory-graph.git
cd spectral-memory-graph/rust-version
cargo build --release
```

Install from this repository:

```bash
cargo install --path crates/spectral-cortex-cli --force
```

On macOS, install also provisions Torch runtime dylibs under `~/.cargo/bin/libtorch` so the installed binary can run directly.

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
- Deterministic local embedder available for tests; real MiniLM used by default for realistic retrieval.
- Outputs structured JSON that an agent can parse to build prompts or context windows.
- Temporal re-ranking biases results toward recent, likely more actionable history — useful for agents that should prefer recent fixes or regression-causing commits.

CLI reference (important flags)
-------------------------------
The CLI binary `spectral-cortex` exposes three primary flows: `ingest`, `update`, and `query`.

Ingest (collect commits -> SMG):
```bash
cargo run -p spectral-cortex --features git2-backend -- ingest --repo /path/to/repo --out smg.json
```

Update (incremental append ingest; only new commits are embedded):
```bash
cargo run -p spectral-cortex --features git2-backend -- \
  update --repo /path/to/repo --out smg.json --git-filter-preset git-noise
```

Query (default, temporal enabled):
```bash
cargo run -p spectral-cortex -- query --query "refactor" --smg smg.json --json --top-k 10
```

Key query flags (agent-friendly):
- `--top-k <n>`: how many final results to return (default 5).
- `--candidate-k <n>`: how many candidates to retrieve from vector search before filtering (defaults to `top_k * 5`).
- `--min-score <float>`: inclusive threshold applied to the combined `final_score` (default 0.7).
- `--no-temporal`: disable temporal re-ranking for this query (temporal is enabled by default).
- `--temporal-weight <0..1>`: control recency influence (default 0.20).
- `--temporal-half-life-days <float>`: half-life for exponential decay (default 14.0).
- `--json`: emit machine-readable JSON (recommended for agents).

Key ingest/update filtering flags:
- `--git-filter-preset git-noise`: drop common metadata lines (e.g. `Co-authored-by`, `Signed-off-by`).
- `--git-filter-drop <regex>`: repeatable custom line-drop regex.
- `--git-filter-case-insensitive`: case-insensitive regex matching.

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

Library API & data model
------------------------
Use the library if you embed the SMG directly inside an agent process.

Primary types:
- `SpectralMemoryGraph`
  - `new() -> Result<Self>`: initializes embedder and structures.
  - `ingest_turn(&mut self, turn: &ConversationTurn) -> Result<()>`: add a turn.
  - `build_spectral_structure(&mut self) -> Result<()>`: compute spectral embeddings & clusters.
  - `retrieve_with_scores(&self, query: &str, candidate_k: usize) -> Result<Vec<(u64, f32)>>`: returns per-turn final scores (semantic + temporal + cluster boosts). Callers may re-rank with a custom `TemporalConfig` if you prefer different defaults.

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
  }
  ```

- `SMGNote`
  - Internal note stored per embedded turn; includes:
    - `raw_content`, `context`
    - `embedding: Vec<f32>`
    - `source_turn_ids: Vec<u64>`
    - `source_commit_ids: Vec<Option<String>>`
    - `source_timestamps: Vec<u64>`
    - `related_note_links: Vec<(u32, f32)>`

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
- Retrieval diagnostics: JSON output includes `raw_score`, `temporal_score`, `final_score`, `timestamp`, `commit_id` and `cluster_label` where available. Agents can use these fields to select evidence and explainability info for prompts.
- Re-ranking: you can override the default re-ranker by calling `re_rank_with_temporal` with a custom `TemporalConfig` (weight, half-life, mode).
- Incremental ingestion: `ingest_turn` appends turns — you can build an ingestion pipeline that streams new commits into a long-running agent process.
- Feedback loop: collect agent judgments (useful/not useful) in a separate store and use those signals to adjust `temporal_weight` or to implement a learned ranker later.

Testing & development
---------------------
- Run unit tests:
```bash
cargo test -p spectral-cortex
```
- Use deterministic fake embedder for tests (the project auto-selects a deterministic fake embedder under `cfg(test)` so CI is reproducible).
- Linting & formatting:
```bash
cargo fmt
cargo clippy -- -D warnings
```

Developer notes
---------------
- The embedder bundles MiniLM assets via a companion `rust_embed` repo; no network fetch is required at runtime.
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
