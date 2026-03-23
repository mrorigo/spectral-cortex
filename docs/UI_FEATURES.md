# UI Features Plan for Spectral Cortex Explorer

## Purpose

This document defines a practical, phased plan for expanding the current SMG Explorer UI from single-note neighborhood inspection into a full graph-analysis surface that includes clusters, long-range links, temporal behavior, and score-driven filtering.

The goal is to make the UI useful for:

1. Fast exploratory analysis.
2. Architectural storytelling.
3. Agent memory debugging and validation.
4. Presentation-quality screenshots and demos.

## Current State (Baseline)

The current UI already supports:

1. Loading an SMG JSON file.
2. Listing notes with search and sort.
3. Editing note context/raw content/related links.
4. Rendering a selected note neighborhood graph.
5. Score-aware edge thickness/opacity.
6. Hover cards and pinned node cards.
7. Long-range link toggle in neighborhood context.

Primary files:

1. `ui/app.js`
2. `ui/index.html`
3. `ui/styles.css`

## Available Data in SMG

The planned features should leverage existing persisted graph data:

1. `notes[*]`
2. `notes[*].related_note_links` as `[related_note_id, similarity]`
3. `notes[*].source_commit_ids`
4. `notes[*].source_timestamps`
5. `cluster_labels`
6. `cluster_centroids`
7. `long_range_links` as `[note_id_a, note_id_b, spectral_similarity]`

No schema changes are required for the first two phases.

## Product Goals

1. Add global context beyond ego networks.
2. Make cluster structure explainable and navigable.
3. Surface non-obvious long-range relationships clearly.
4. Make score and time dimensions first-class controls.
5. Keep performance acceptable on multi-thousand-node graphs.

## Non-Goals

1. Full graph rendering of all nodes at once with heavy interactivity on very large repos.
2. Recomputing embeddings/spectral structure in the browser.
3. Replacing CLI/MCP; UI complements them.

## Feature Roadmap

## Phase 1: High-Value, Low-Risk Additions

### 1) Global Cluster Map

Summary:

1. Add a new visualization mode that displays all notes by cluster color.
2. Show optional hull/region around each cluster.

Why:

1. Gives immediate macro view of repository memory topology.

Implementation outline:

1. Add `viewMode` state in `ui/app.js` with at least `neighborhood` and `cluster_map`.
2. Build a node set from all notes, plus optional subsampling toggle for very large files.
3. Color nodes by `cluster_labels[note_index]` when labels exist.
4. Add legend mapping cluster id to color and note count.
5. Reuse D3 simulation but with reduced labels by default.

Acceptance criteria:

1. User can switch between neighborhood and cluster map.
2. Cluster coloring is deterministic for the same file.
3. Selecting a node in cluster map syncs editor panel and list selection.

### 2) Long-Range Links View

Summary:

1. Add dedicated mode showing only `long_range_links`.
2. Emphasize bridge edges (thickness by similarity).

Why:

1. Long-range links are one of the most distinctive outputs of the system.

Implementation outline:

1. Add `viewMode = long_range`.
2. Node set includes endpoints in `long_range_links`.
3. Add top-K slider for links and min-similarity slider.
4. Use neon magenta link palette already introduced for visual continuity.
5. Add side table of top bridges with clickable rows.

Acceptance criteria:

1. User can inspect strongest long-range bridges quickly.
2. Clicking table row focuses corresponding edge and nodes.

### 3) Score Distribution + Threshold Controls

Summary:

1. Add UI controls for minimum edge score and optional percentile cut.
2. Add compact histogram for current edge-score distribution.

Why:

1. Lets users tune signal/noise interactively.

Implementation outline:

1. Compute score arrays per view mode.
2. Add lightweight SVG histogram component.
3. Filter graph edges before simulation build.
4. Keep node selection stable when filters change.

Acceptance criteria:

1. Score threshold updates graph in under 200 ms for medium graphs.
2. Histogram reflects filtered and total counts.

## Phase 2: Analytical Depth

### 4) Cluster-to-Cluster Matrix

Summary:

1. Add matrix view where each cell encodes connection strength between clusters.

Why:

1. Reveals coupling patterns not obvious in node-link layouts.

Implementation outline:

1. Build aggregate metrics:
2. `count(links between cluster i and j)`
3. `sum(similarity)` and `mean(similarity)`
4. Add toggle for relation source:
5. `related_note_links`
6. `long_range_links`
7. Clicking cell drills down to notes/edges for that cluster pair.

Acceptance criteria:

1. Matrix renders whenever cluster labels exist.
2. Drill-down returns concrete note pairs.

### 5) Temporal Evolution Panel

Summary:

1. Add timeline showing note and link activity by time bucket.

Why:

1. Enables “when did this happen?” investigation and storytelling.

Implementation outline:

1. Derive note timestamp from `source_timestamps`.
2. Bucket by day/week/month.
3. Show stacked cluster activity bars.
4. Overlay long-range-link count line per bucket.
5. Add brushing to set global time filter for all views.

Acceptance criteria:

1. Changing time range updates graph/list/query context.
2. Bucketing remains stable for large datasets.

### 6) Cluster Detail Panel

Summary:

1. Add right-side panel for selected cluster metadata and representative notes.

Why:

1. Turns cluster IDs into meaningful human-readable context.

Implementation outline:

1. For selected cluster, compute:
2. size
3. median timestamp
4. top connected clusters
5. representative notes (highest internal degree or centroid-nearest approximation)
6. Add actions:
7. Focus cluster in graph
8. Export cluster note IDs

Acceptance criteria:

1. User can navigate from macro cluster view to concrete notes in 1-2 clicks.

## Phase 3: Power Tools

### 7) Compare Mode (Dual Ego Networks)

Summary:

1. Side-by-side neighborhoods for two selected notes with overlap highlighting.

Why:

1. Useful for debugging retrieval and understanding related decision paths.

Implementation outline:

1. Add second selection slot (`compareNoteId`).
2. Build two neighborhoods and compute overlap set.
3. Show symmetric difference and intersection counts.
4. Offer “merge overlay” toggle.

Acceptance criteria:

1. Comparison can be saved as screenshot-ready view.

### 8) Provenance Overlay

Summary:

1. Visual toggle for short commit IDs and timestamp badges on nodes.

Why:

1. Keeps graph explainable and verifiable against git history.

Implementation outline:

1. Badge renderer for nodes with single source commit.
2. Compact “N commits” badge for aggregated nodes.
3. Tooltip panel with commit/timestamp details.

Acceptance criteria:

1. Provenance overlay can be enabled without making graph unreadable.

## UX and Interaction Requirements

1. View-mode switcher should be always visible near graph header.
2. Global controls (score/time filters) should persist across modes.
3. Legends should be mode-aware and compact.
4. Node click should always synchronize:
5. graph selection
6. note list highlight
7. editor panel
8. Hover and pinned cards should work in all node-link modes.

## Performance Strategy

1. Keep neighborhood mode as default for very large graphs.
2. Add hard cap and sampling options for full-map modes.
3. Use requestAnimationFrame/debounced control updates for sliders.
4. Memoize derived data:
5. cluster membership maps
6. long-range endpoint sets
7. histogram bins
8. time buckets

Suggested guardrails:

1. `> 12k nodes`: require explicit confirmation before full cluster map.
2. `> 40k edges`: default to higher score threshold.

## Data Derivation Utilities to Add

In `ui/app.js`, add helper builders:

1. `buildClusterIndex(graph)` -> `Map<clusterId, noteIds[]>`
2. `buildClusterEdgeMatrix(graph, relationKind)` -> matrix object
3. `buildTemporalBuckets(graph, granularity)` -> bucket series
4. `buildLongRangeEndpointGraph(graph, topK, minScore)` -> node/link set
5. `computeScoreStats(links)` -> histogram + quantiles

## UI Structure Changes

Minimal HTML changes in `ui/index.html`:

1. Add view-mode selector.
2. Add score and time control containers.
3. Add optional right-side analysis subpanel placeholder.

CSS additions in `ui/styles.css`:

1. Matrix/grid styling.
2. Timeline styling.
3. Analysis panel cards.
4. Consistent neon legend variants by mode.

## Testing and Validation

Manual scenarios:

1. Small graph (under 500 notes).
2. Medium graph (1k-5k notes).
3. Large graph (8k+ notes).
4. Graph with missing cluster labels.
5. Graph with sparse long-range links.
6. Graph with highly skewed timestamps.

Validation checks:

1. No JS errors when optional sections are missing.
2. Selection sync stays consistent after filter changes.
3. Graph remains interactive after switching modes repeatedly.
4. Export/save behavior unaffected.

## Delivery Plan

Recommended order:

1. Phase 1.1 Global Cluster Map
2. Phase 1.2 Long-Range Links View
3. Phase 1.3 Score Histogram/Threshold Controls
4. Phase 2.4 Cluster Matrix
5. Phase 2.5 Temporal Evolution Panel
6. Phase 2.6 Cluster Detail Panel
7. Phase 3 Compare + Provenance overlays

## Definition of Done (Per Feature)

1. Feature is discoverable from main UI controls.
2. Works on at least medium-size SMG without major lag.
3. Includes legend/explanation for visual encoding.
4. Preserves existing editor/list workflows.
5. Documented in `docs/USER-GUIDE.md` with one usage screenshot/example.

## Risks and Mitigations

1. Risk: Full-map rendering becomes sluggish on large graphs.
2. Mitigation: sampling, thresholds, and mode-specific caps.
3. Risk: Visual overload from too many encodings.
4. Mitigation: progressive disclosure and mode-specific legends.
5. Risk: Ambiguity in cluster interpretation.
6. Mitigation: cluster detail panel with representative notes and metadata.

## Future Extensions (Post-Plan)

1. Snapshot/story mode for generating presentation slides directly from views.
2. Saved view presets (filters + mode + selected note).
3. Diff mode between two SMG files (before/after ingest updates).
4. Optional WebGL rendering path for very large graphs.
