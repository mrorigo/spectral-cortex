Integrating `tree-sitter` into Spectral Cortex is exactly what is needed to bridge the gap between text-based retrieval and true code comprehension. Industry leaders have explicitly pointed out that naive vector retrieval fails because "Code is not merely text. It is a web of dependencies, inheritance hierarchies, and architectural patterns". Flattening this structure into basic embeddings destroys critical relationships, leading to "multi-hop reasoning failure" when an AI agent needs to trace logic from an API endpoint down to a database model.

Here is how you can use `tree-sitter` to evolve the Spectral Memory Graph (SMG) into a tool that truly understands the codebase structurally and temporally:

### 1. Symbol-Level Tracking (Solving the Rename/Refactor Problem)
Currently, `git blame` and naive history searches are frequently derailed by "cosmetic refactors, renames, or bulk style changes". If an agent relies on file paths, a file rename breaks its historical lineage.
*   **The Enhancement:** Use `tree-sitter` to parse the Abstract Syntax Tree (AST) of the files across commits, extracting unique symbols (e.g., `class PaymentService`, `fn calculate_tax`). Instead of attaching `ConversationTurn` or `SMGNote` metadata to a file path, attach it to a **Symbol ID**. 
*   **The Benefit:** If a developer renames a file or moves a function to a new module, `tree-sitter` allows Spectral Cortex to recognize that the AST node's signature remains structurally identical. The agent can seamlessly trace the history of `calculate_tax` across arbitrary file movements, bypassing the manual frustration of digging past refactoring noise.

### 2. Injecting AST Edges into the Spectral Adjacency Matrix
Spectral Cortex currently builds its memory graphs based on the semantic similarity (cosine similarity) of text chunks. However, this misses explicit structural dependencies.
*   **The Enhancement:** When `tree-sitter` parses the code, extract all imports, function calls, and inheritance structures to create structural edges. In your `build_spectral_structure` step, modify the `sparsify_adj` and `cosine_similarity_matrix` logic to fuse **semantic similarity** with **structural adjacency**. For example, if Function A calls Function B, inject a high-weight edge between their respective `SMGNote`s, even if their natural language embeddings aren't highly similar.
*   **The Benefit:** This directly solves the "multi-hop reasoning" gap. When an agent asks about an undocumented API constraint, the SMG can retrieve not just the function, but structurally retrieve the caller, the callee, and the historical commits that touched that specific call chain. 

### 3. Binding the "Why" to Specific Functions
A major complaint when dealing with legacy code is that the "intent behind code is missing", leaving developers to wonder "why 42 bottles of cider?" when looking at arbitrary choices. 
*   **The Enhancement:** Instead of chunking a commit diff as dumb text, use `tree-sitter` to parse the diff and identify exactly *which AST nodes* were modified. Then, bind the commit message (which usually contains the intent or "why") directly to the structural node. If you split a commit using your `--git-commit-split-mode strict` flag, map each split segment specifically to the function it changed.
*   **The Benefit:** When an agent queries the code, it doesn't just get a wall of file text. It receives the specific function, tightly bundled with a filtered timeline of the exact commit messages that modified that specific logic over time. 

### 4. AST-Aware "Hotspot" and "Churn" Analysis
A highly effective way to prioritize technical debt is by analyzing **Hotspots**—complex code that is frequently changed (high churn). Currently, churn is typically calculated at the file level.
*   **The Enhancement:** By using `tree-sitter` to track git history, Spectral Cortex can calculate churn and complexity metrics for individual functions or classes over time. You can generate a "Tech Debt Tracker" natively within the SMG that highlights volatile, highly-coupled symbols. 
*   **The Benefit:** You can provide an MCP tool for agents (e.g., `get_structural_hotspots`) that instantly tells the agent which functions are the most brittle or frequently modified, allowing the agent to be extra cautious—or request more context—when asked to modify those specific areas.

### 5. Abstracting Away Missing Documentation 
Developers often struggle to navigate new repositories because "they don't maintain documentation, and even if they do, it is not updated". 
*   **The Enhancement:** Combine Spectral Cortex's temporal ranking with `tree-sitter`'s structural map. When an agent is asked to fix a bug, it can query the SMG to pull the specific AST component, instantly cross-reference any Architecture Decision Records (ADRs) tied to those structural tags, and pull the most *recent* (temporal-weighted) bug-fix commits related to that symbol.
*   **The Benefit:** This creates an automated "Brain Dump". The agent doesn't need to read outdated, top-level documentation; Spectral Cortex serves it a highly accurate, micro-contextual package of the code's structure and the historical narrative of how it evolved.