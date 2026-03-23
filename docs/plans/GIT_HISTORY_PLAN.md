spectral-memory-graph/rust-version/GIT_HISTORY_PLAN.md#L1-240
# GIT_HISTORY_PLAN

Status: In progress — workspace, CLI scaffold, persistence helpers, spectral utilities (implemented and wired), and clippy/cargo-check fixes completed. Remaining work: unit & integration tests, CLI persistence UX, incremental spectral updates, and CI gating.

Last updated: 2026-02-11

Purpose
- Transform the existing Rust "Spectral Cortex" into a well-structured library crate + companion CLI that:
  - ingests git history,
  - builds a spectral memory graph (SMG),
  - persists SMG state,
  - supports deterministic testing and reliable querying.

High-level summary of progress
- Workspace & crates
  - Cargo workspace created at `rust-version/`.
  - Members:
    - `crates/spectral-cortex-lib` (library)
    - `crates/spectral-cortex-cli` (binary)
- Library
  - Moved existing model types and embedder into `crates/spectral-cortex-lib/src/`.
  - Implemented JSON persistence helpers `save_smg_json` and `load_smg_json`.
  - Kept persistence lean: large recomputable matrices (similarity, Laplacian, spectral embeddings) are intentionally omitted. Stored: notes, cluster labels and centroids.
  - The embedder:
    - Real heavy embedder gated by feature (`real-embed`).
    - Deterministic fake embedder provided by default for CI/tests (384-dim).
- Spectral utilities
  - Implemented numerics and helpers in `crates/spectral-cortex-lib/src/graph/spectral.rs`:
    - `assemble_embedding_matrix`
    - `cosine_similarity_matrix`
    - `sparsify_adj`
    - `degree_vector`
    - `normalized_laplacian`
    - `spectral_decomposition` (nalgebra)
    - `eigengap_heuristic`
    - `compute_spectral_embeddings`
    - `run_kmeans_on_spectral`
    - `compute_centroids_in_embedding_space`
    - `detect_long_range_links`
    - `incremental_spectral_update` (documented stub)
  - Each function is documented and designed for unit testing.
- Wiring
  - Replaced the previous inline spectral code in `SpectralMemoryGraph::build_spectral_structure()` (`crates/spectral-cortex-lib/src/graph/mod.rs`) to call the new spectral helpers in the canonical sequence:
    1. assemble embedding matrix
    2. compute cosine similarity
    3. sparsify adjacency
    4. normalized Laplacian
    5. spectral decomposition
    6. eigengap heuristic / choose k
    7. compute spectral embeddings
    8. KMeans clustering
    9. compute centroids (in original embedding space)
    10. detect long-range links and attach to notes
  - Ensured a deterministic ordering of notes (sorted by `note_id`) so behavior is reproducible in tests.
- CLI
  - `crates/spectral-cortex-cli/src/main.rs` implements a `clap`-based CLI with `ingest` and skeleton `query`.
  - `ingest` subcommand collects commits via a `git2` feature-gated backend and converts them to `ConversationTurn` objects, then ingests into the SMG and optionally builds spectral structures.
- Quality & tooling
  - Ran `cargo check` and iterated on code fixes.
  - Ran `cargo clippy -D warnings` on the workspace and fixed all reported issues (removed obsolete lints, addressed clippy suggestions, converted `&Vec` to slices, replaced redundant closures, used `.clamp()` where appropriate).
  - Logging utilities initialized during SMG construction to ensure consistent observability.
  - All changes follow the project's documentation and style conventions; public items carry doc comments.

Files changed / implemented (not exhaustive)
- rust-version/Cargo.toml (workspace)
- crates/spectral-cortex-lib/
  - src/lib.rs — re-exports, JSON persistence (`save_smg_json` / `load_smg_json`)
  - src/embed/mod.rs — feature-gated real embedder + deterministic fake embedder
  - src/graph/mod.rs — `SpectralMemoryGraph` (now calls spectral helpers)
  - src/graph/spectral.rs — spectral analysis utilities (new)
  - src/model/*.rs — `ConversationTurn`, `SMGNote` with serde derives; small signature fixes for clippy
  - src/utils/logging.rs — logging init
- crates/spectral-cortex-cli/
  - src/main.rs — `clap` CLI skeleton; `ingest` implemented (git2 feature-gated)

Outstanding / planned work (prioritized)
1. Unit tests for spectral utilities (HIGH)
   - Add tests that exercise:
     - `assemble_embedding_matrix` ordering and shape behavior.
     - `cosine_similarity_matrix` on small, deterministic inputs.
     - `normalized_laplacian` properties (symmetry, diagonal behavior).
     - `spectral_decomposition` and `eigengap_heuristic` on synthetic Laplacians.
     - `run_kmeans_on_spectral` determinism when using fake embedder.
   - Use the deterministic fake embedder to keep tests reproducible.
2. Integration test — end-to-end (HIGH)
   - Small fixture of synthetic turns or a tiny git repo in `tests/fixtures/`:
     - Ingest via CLI or lib API, `build_spectral_structure`, run `retrieve`, assert expected outputs and cluster metadata.
3. CLI persistence & UX improvements (MEDIUM)
   - Add `--output` (or `--out`) and `--append` options to `ingest` so users can persist SMG JSON files and append on repeated runs.
   - Implement `query` subcommand to load SMG JSON and run retrieval with `--json` output option.
4. GitIngestor abstraction & shell-out fallback (MEDIUM)
   - Implement a `git-cmd` backend that shells out to `git log` as a fallback to `git2` for environments without libgit2.
   - Expose backend choice via features or CLI flags for reproducibility and portability.
5. Incremental spectral updates & strategies (LOW / Phase 4)
   - Implement `incremental_spectral_update()` heuristics:
     - Local neighborhood recompute for new nodes.
     - Periodic global recompute trigger.
     - Configurable policies via `SmgConfig`.
6. CI configuration & coverage (MEDIUM)
   - Add a GitHub Actions workflow that runs:
     - `cargo fmt --all -- --check`
     - `cargo clippy --all --all-targets -- -D warnings`
     - `cargo test --all`
   - Ensure tests use the fake embedder by default so CI doesn’t require model downloads.
7. Documentation & examples (LOW)
   - Update repo README with example commands, quickstart for ingest + query, and developer guide for running tests.

Detailed next steps for the next session (recommended)
- Create/commit unit tests for the spectral helpers in `crates/spectral-cortex-lib/tests/spectral_utils.rs`:
  - Start with deterministic small matrices to validate numerical correctness.
- Add an integration test fixture (a tiny synthetic repo or a small list of `ConversationTurn` objects) and an E2E test that:
  - runs ingest (library API),
  - builds the spectral structure,
  - persists the SMG to JSON,
  - reloads it and runs `retrieve` for a known query,
  - asserts retrieved turn ids and cluster consistency.
- Wire CLI `ingest --out` to call `save_smg_json` at the end of `--rebuild`, and add `ingest --append` semantics for appending notes to an existing SMG file.
- Add CI workflow that executes the checks and runs the new tests.

Design & API considerations to keep in mind
- Public API should be small and well-documented:
  - Prefer builder-style configurations for `SmgConfig` and `RetrieveOptions`.
  - Keep embedding provider behind a trait (e.g., `Embedder`) so tests can inject fake determinism.
- Persistence must be stable and versioned:
  - `SerializableSMG` contains `format_version`, metadata, notes, `cluster_labels`, and `cluster_centroids`.
  - Any future format changes must be accompanied by a migration helper.
- Performance and memory:
  - For large repositories, implement streaming ingest + JSONL / SQLite adapters to avoid keeping all embeddings in memory.
  - Consider toggles for sparse vs dense similarity computation and approximate nearest neighbors for big graphs.

Acceptance criteria (updated)
- All code compiles in the workspace.
- `cargo clippy -- -D warnings` passes in CI for the workspace (or documented, justified exceptions).
- Unit tests for spectral utilities exist and are deterministic under the fake embedder.
- An integration test ingests a fixture and validates retrieval and cluster metadata.
- CLI supports `ingest --out` to persist computed SMG JSON for later query.

How to continue
- If you'd like, in the next session I will:
  - Add the unit tests + integration fixture and iterate until all tests pass.
  - Implement CLI `--out`/`--append` persistence handling and a `query` subcommand that loads persisted SMG JSON.
  - Add a simple `git-cmd` fallback ingestor.
- Tell me which of the prioritized items above you want me to implement next (tests, CLI persistence, git-cmd backend, or incremental spectral heuristics) and I'll produce the exact file edits and tests in the next session.
