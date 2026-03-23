# Implementation Plan: Tree-Sitter Integration in Spectral Cortex

## 1. Overview
This document outlines the detailed plan for integrating `tree-sitter` into Spectral Cortex. The goal is to transform the Spectral Memory Graph (SMG) from a purely semantic text-based store into an AST-aware (Abstract Syntax Tree) structural graph. This integration will solve the "rename/refactor" problem, enable multi-hop reasoning via structural edges, and bind commit intent ("why") directly to logical code symbols.

### Key Objectives
- **Symbol-Level Tracking**: Persistent identity for functions/classes across renames and moves.
- **Structural Adjacency Matrix**: Fusing semantic similarity with AST-derived call graphs and inheritance.
- **AST-Aware Commits**: Binding "why" decisions to specific code nodes during ingestion.
- **Spectral Polarity**: Modern noise reduction to prune irrelevant cross-file context.
- **Agentic MCP Tools**: Structural hotspots and symbol history inspection.

---

## 2. Phase 1: Data Model Expansion
Grounding: `crates/spectral-cortex-lib/src/model/smg_note.rs` and `crates/spectral-cortex-lib/src/lib.rs`.

### 2.1 Update `SMGNote`
Add AST metadata to the core note model to support symbol tracking and typing.

| File | Change |
| :--- | :--- |
| `smg_note.rs` | Add `symbol_id: Option<String>` (e.g., `fn:calculate_tax`). |
| `smg_note.rs` | Add `ast_node_type: Option<String>` (e.g., `API_DEFINITION`, `IMPLEMENTATION`). |
| `smg_note.rs` | Add `structural_links: Vec<u32>` to store AST-derived neighbors (callees/parents). |

### 2.2 Update `SerializableNote`
Ensure these fields are persisted to JSON for long-term memory across indexing runs.

| File | Change |
| :--- | :--- |
| `lib.rs` | Update `SerializableNote` struct with new fields. |
| `lib.rs` | Update `From<&SMGNote> for SerializableNote` implementation. |
| `lib.rs` | Update `load_smg_json` to reconstruct these fields. |

---

## 3. Phase 2: AST-Aware Ingestion
Grounding: `crates/spectral-cortex-cli/src/git_commit_split.rs` and `crates/spectral-cortex-cli/src/main.rs`.

### 3.1 Tree-Sitter Integration
Add dependencies to `crates/spectral-cortex-cli/Cargo.toml`:
- `tree-sitter`
- `tree-sitter-rust`
- `tree-sitter-typescript`
- `tree-sitter-python`

### 3.2 Extensible Language Interface
To support any language, we will implement a `SymbolParser` trait that abstracts away language-specific AST queries.

```rust
pub trait SymbolParser: Send + Sync {
    /// Recommended node types that represent symbols (e.g. "function" in Rust, "function_declaration" in TS)
    fn symbol_node_types(&self) -> &[&str];
    
    /// Extract a stable symbol_id from a node (e.g. "fn:calculate_tax")
    fn extract_symbol_id(&self, node: Node, source: &str) -> Option<String>;
    
    /// Identify if a node represents an interface/API definition vs implementation
    fn node_category(&self, node: Node) -> AstNodeCategory;
}
```

#### 3.2.1 Language-Specific Support
| Language | Symbol Node Types | Node Category Mapping |
| :--- | :--- | :--- |
| **Rust** | `function_item`, `struct_item`, `trait_item` | `trait_item` -> `API_DEFINITION` |
| **TypeScript** | `function_declaration`, `class_declaration`, `interface_declaration` | `interface_declaration` -> `API_DEFINITION` |
| **Python** | `function_definition`, `class_definition` | Based on docstrings or type hints |

### 3.3 Implement `AstSplitMode`
Modify `git_commit_split.rs` to support splitting by AST modification using the `SymbolParser` registry.

1.  **Extend `CommitSplitMode`**: Add `Ast` variant.
2.  **AST Diff Parser**:
    - During git ingestion, detect the file extension and select the appropriate `SymbolParser`.
    - Parse the pre-commit and post-commit state using `tree-sitter`.
    - Identify modified AST nodes (functions, classes, methods).
    - Map the commit message "why" specifically to the nodes it touches.
3.  **Symbol Extraction**: Generate stable `symbol_id` strings based on node signatures.

---

## 3.3 Binding the "Why" to specific functions
Currently, `ConversationTurn` (and thus `SMGNote`) represents a "chunk" of text. In high-fidelity mode, we will:
1. Parse the diff hunk.
2. Locate the AST node it modifies.
3. Use the node's scope/path as the `symbol_id`.
4. Create a specific `SMGNote` for that change, even if multiple nodes are touched in one file.

---

## 4. Phase 3: Structural Spectral Fusion
Grounding: `crates/spectral-cortex-lib/src/graph/spectral.rs`.

### 4.1 Fusing the Adjacency Matrix
In `cosine_similarity_matrix(x: &Array2<f32>) -> Array2<f32>`, modify the logic to incorporate structural edges.

**Proposed Logic**:
1. Compute `W_semantic` (existing NLP cosine similarity).
2. Retrieve `structural_links` for each note.
3. Apply boost: If `note_j` is in `note_i.structural_links`, set `W_final[i,j] = alpha * W_semantic[i,j] + beta`.
4. Ensure `W_final` remains symmetric.

### 4.2 API-Weighted Topological Expansion
In `detect_long_range_links`, give higher weight/gravity to nodes where `ast_node_type == "API_DEFINITION"`. This ensures the spectral "shortcuts" prefer routing through interfaces.

---

## 5. Phase 4: Spectral Polarity Filtering
Grounding: `crates/spectral-cortex-lib/src/graph/mod.rs` (`retrieve_with_scores`).

Implement a filter to drop "noisy" hits that are semantically similar in text but topologically distant in the code structure.

1.  **Map Query to Spectral Space**: Project the query embedding onto the first $k$ eigenvectors (`spectral_embeddings`).
2.  **Distance Verification**:
    - For each top-K semantic candidate:
    - Compute the distance to the query in spectral space.
    - If `distance > polarity_threshold`, flag as `<neg>` and prune.

---

## 6. Phase 5: Agent-Oriented MCP Tools
Grounding: `crates/spectral-cortex-cli/src/mcp_server.rs`.

### 6.1 `get_structural_hotspots`
- **Input**: None.
- **Logic**: Aggregate `SMGNote`s by `symbol_id`. Calculate churn frequency (density of `source_timestamps`) and AST complexity.
- **Return**: Markdown list of the top 10 most volatile methods/classes.

### 6.2 `inspect_symbol_history`
- **Input**: `symbol_id`.
- **Logic**: Retrieve all notes matching the `symbol_id`. Sort by `source_timestamps` (chronological).
- **Return**: Markdown timeline showing exactly how and *why* this specific symbol changed over time, bypassing file-level noise.

---

## 7. Testing & Verification

### 7.1 Integration Test Case: The Rename
1.  Ingest Commit 1: `fn old_name()` in `file_a.rs`.
2.  Ingest Commit 2: Rename `file_a.rs` -> `file_b.rs` and `old_name` -> `new_name`.
3.  **Verify**: Querying "old_name" should retrieve the history and "why" from Commit 1, linked via structural identity.

### 7.2 Structural Link Test
1.  Ingest a small graph where `Module A` calls `Module B`.
2.  Ensure `build_spectral_structure` generates a long-range link or high spectral similarity between them even if their text descriptions are disjoint (e.g., encryption logic calling a logging utility).

### 7.3 Hotspot Verification
1.  Simulate 10 commits to `fn volatile()` and 1 commit to `fn stable()`.
2.  Verify `get_structural_hotspots` correctly identifies `volatile`.
