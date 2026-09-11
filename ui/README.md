# Spectral Cortex UI

A dependency-free browser UI for exploring persisted SMG JSON files. Open `index.html` directly in a browser, then select or drag in an SMG JSON file. The UI never fetches graph data over HTTP.

The Atlas view provides Field, Neighborhood, Bridges, Force, and Matrix graph lenses. Force mode computes an ephemeral client-side layout from the currently visible relationships; positions are not written back to the SMG file. If an SMG has no persisted semantic links, the UI derives a bounded local cosine-similarity map from its embedded vectors in a browser worker; graph data never leaves the browser. The left index can browse grouped Events, Symbols, or structural Hotspots, and supports preset or exact temporal windows. The Chronicle view organizes notes by month. Search covers commit content, file paths, symbols, AST node types, and commit hashes.
