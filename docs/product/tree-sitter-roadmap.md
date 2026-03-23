Transforming Spectral Cortex from a purely semantic text-based memory store into an AST-aware (Abstract Syntax Tree) structural graph requires bridging your existing numerical foundation with `tree-sitter`'s parsing capabilities. By grounding this roadmap in your current source code and recent RAG insights (like AllianceCoder and REPOFILTER), here is the concrete, step-by-step implementation plan.

### Phase 1: Expanding the Data Model for Symbol Tracking
Currently, Spectral Cortex tracks notes via `SMGNote` and serializes them in a strict format (`spectral-cortex-v1`). To track code structurally across renames and refactors, we must attach AST metadata to these notes.

*   **Step 1.1: Update `SMGNote` and `SerializableNote`**
    Modify `crates/spectral-cortex-lib/src/model/smg_note.rs` and the `SerializableNote` struct in `src/lib.rs`. Add fields to store the AST symbol identity and node type:
    ```rust
    pub symbol_id: Option<String>, // e.g., "class:PaymentService" or "fn:calculate_tax"
    pub ast_node_type: Option<String>, // e.g., "API_DEFINITION", "IMPLEMENTATION"
    ```
*   **Step 1.2: Implement API-Centric Typing**
    Following the findings from AllianceCoder, which show that retrieving structural API definitions yields higher performance gains than retrieving raw code, use the `ast_node_type` field to distinguish between function signatures/interfaces and their implementations. 

### Phase 2: Upgrading the Ingestion Layer (`git_commit_split.rs`)
Your current ingestion logic in `crates/spectral-cortex-cli/src/git_commit_split.rs` splits commit messages using regex patterns (conventional headers, bullets, or paragraphs) based on `CommitSplitMode`. We need to evolve this into an AST-aware parser.

*   **Step 2.1: Integrate `tree-sitter`**
    Add `tree-sitter` and language-specific parsers (e.g., `tree-sitter-rust`, `tree-sitter-python`) to `spectral-cortex-cli/Cargo.toml`.
*   **Step 2.2: Create an `AstSplitMode`**
    Extend the `CommitSplitMode` enum in `git_commit_split.rs` to include an `Ast` mode. 
*   **Step 2.3: Bind "Why" to AST Nodes**
    When analyzing a git diff during ingestion, run `tree-sitter` over the pre- and post-commit file states. Instead of chunking the diff as pure text, identify exactly *which* AST nodes were modified. Create an `SMGNote` that binds the commit message intent specifically to the extracted `symbol_id`, dropping noisy cosmetic line changes entirely.

### Phase 3: Fusing Semantic and Structural Edges (`spectral.rs`)
This is where Spectral Cortex's mathematical core gets a massive upgrade. Code is a web of dependencies and inheritance hierarchies, and flattening it destroys multi-hop reasoning. We must inject structural edges into your adjacency matrix.

*   **Step 3.1: Extract Call Graphs**
    During the `tree-sitter` parsing phase, extract structural relationships (e.g., "Function A calls Function B"). 
*   **Step 3.2: Modify the Adjacency Matrix Construction**
    In `crates/spectral-cortex-lib/src/graph/spectral.rs`, locate the `cosine_similarity_matrix(x: &Array2<f32>) -> Array2<f32>` function. Currently, it computes $W$ strictly based on the cosine similarity of the NLP embeddings.
    Update the logic to fuse semantic and structural data. If `tree-sitter` detects a direct call or inheritance relationship between two `note_ids`, artificially boost their weight in the adjacency matrix before it is passed to `sparsify_adj(w: &mut Array2<f32>, threshold: f32)`.
    *Formula concept:* $W_{final} = \alpha W_{semantic} + \beta W_{structural}$
*   **Step 3.3: API-Weighted Topological Expansion**
    When detecting long-range links via `detect_long_range_links()`, apply higher gravity (weights) to nodes tagged with the `API_DEFINITION` node type from Phase 1, ensuring the spectral graph inherently routes reasoning toward structural interfaces rather than scattered implementation details.

### Phase 4: Implementing "Spectral Polarity" for Noise Reduction
Recent insights from REPOFILTER show that simply forcing standard top-K vector retrieval introduces massive noise, as up to 85% of retrieved cross-file chunks are neutral or actively harmful to LLM generation. We can solve this entirely using your existing spectral mathematics.

*   **Step 4.1: Compute Spectral Polarity**
    In your retrieval logic (likely inside `SpectralMemoryGraph::retrieve_with_scores`), implement a geometric polarity filter. After finding the top-K semantic matches via the embedded query, map those candidates into the spectral eigenvector space using the cached `spectral_embeddings: Option<Array2<f32>>`.
*   **Step 4.2: Prune Negative Memories (<neg>)**
    Calculate the distance in the spectral space between the query and the candidate notes. If a note has high text similarity (shares keywords) but is extremely distant in the spectral topological space, flag it as a `<neg>` (negative) memory and drop it from the result set. This achieves massive context reduction with zero additional LLM overhead.

### Phase 5: Agent-Oriented MCP Tooling (`mcp_server.rs`)
Finally, expose these new structural capabilities to AI agents via your Model Context Protocol (MCP) server.

*   **Step 5.1: Enhance Existing Tools**
    Update `QueryGraphInput` and the `query_graph` tool in `crates/spectral-cortex-cli/src/mcp_server.rs` to return the new `symbol_id` alongside the raw context.
*   **Step 5.2: Create a `get_structural_hotspots` Tool**
    Since `tree-sitter` now tracks symbols over time, you can aggregate temporal churn per AST node. Expose an MCP tool that allows an agent to ask, "Which functions are the most brittle or frequently modified?" The tool can query the `SMGNote`s grouped by `symbol_id` and sort them by the density of their `source_timestamps`.
*   **Step 5.3: Create an `inspect_symbol_history` Tool**
    Add a tool that takes a `symbol_id` (e.g., `fn:calculate_tax`) and returns a tightly chronological, markdown-formatted timeline of every architectural decision (ADR) and bug-fix commit that touched that specific AST node.

By following this roadmap, you will successfully transform Spectral Cortex from a highly optimized textual semantic graph into a structurally aware, noise-filtering code intelligence engine that natively understands codebase architecture.