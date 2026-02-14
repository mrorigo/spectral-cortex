# Spectral Memory Graph SPA Plan

## Goal
Build a browser-only single page app in `ui/` (HTML/CSS/JS) to:
- Load a Spectral Memory Graph JSON file (using `smg-roo.json` as the reference shape).
- Explore the graph and note details.
- Edit and delete notes safely.
- Save the updated graph back to JSON.
- Match the visual theme used in `docs/index.html`.
- Provide an appealing force-directed visualization for graph exploration.

## Input Model Baseline (from `smg-roo.json` + README)
Top-level fields to preserve during roundtrip:
- `metadata` (must keep `format_version`, expected `spectral-cortex-1`).
- `notes` (array of note objects).
- `cluster_labels` (array aligned to note index/order).
- `cluster_centroids` (object keyed by cluster id strings, vector values).
- `cluster_centroid_norms` (object keyed by cluster id strings, numeric values).
- `long_range_links` (array of `[note_id_a, note_id_b, score]`).

Note fields to preserve:
- `note_id` (number; primary key for UI operations).
- `raw_content`, `context`.
- `embedding` (vector; 384-dim in sample).
- `norm`.
- `source_turn_ids`, `source_commit_ids`, `source_timestamps`.
- `related_note_ids`.

## Non-Goals (v1)
- No server backend.
- No recomputation of embeddings, clusters, or spectral structure.
- No full-graph live force simulation across all ~6k nodes at once.
- No automatic reindexing of `note_id`.

## UX and IA
## Layout
Single-page 3-column desktop layout (collapses on mobile):
- Left rail: file actions + filters/search + node list.
- Center: force-directed neighborhood graph for selected note.
- Right panel: selected note editor + metadata/actions.

Primary app sections:
1. Header: app name, theme-consistent branding, load/save buttons.
2. Data status bar: file name, note count, dirty state, schema warnings.
3. Explorer region:
- Search (by `note_id`, `context`, `raw_content`, commit ids).
- Sort options (id asc, id desc, timestamp desc).
- Paginated/virtualized list for scalability.
4. Force graph pane:
- Render selected node + neighborhood (`related_note_ids` + inbound links).
- Optionally overlay `long_range_links` touching selected node.
- Click a node to focus and inspect.
5. Editor pane:
- Editable fields: `raw_content`, `context`, `related_note_ids`.
- Read-only by default for heavy/derived fields (`embedding`, `norm`, source arrays).
- Delete action with confirmation and impact summary.

## Key User Flows
1. Load JSON:
- User chooses local file.
- Validate schema and build in-memory indexes.
- Show parse/validation diagnostics if invalid.

2. Explore:
- Search/filter note list.
- Select note to inspect details and local graph neighborhood.
- Pan/zoom and click nodes in force graph.

3. Edit node:
- Update editable fields.
- Validate before commit to in-memory state.
- Mark document dirty.

4. Delete node:
- Confirm delete.
- Remove note.
- Remove deleted id from all `related_note_ids`.
- Remove long-range links referencing deleted note.
- Update `cluster_labels` by note array index removal policy.
- Keep `note_id` values of remaining notes unchanged.
- Mark document dirty.

5. Save JSON:
- Re-run integrity checks.
- Serialize canonical JSON with stable indentation.
- Download via blob as `<original>-edited.json`.

## Theme Plan (mirror `docs/index.html`)
Adopt the same palette and typography tokens:
- Colors: `--bg`, `--bg-2`, `--ink`, `--ink-soft`, `--brand`, `--brand-2`, `--accent`, `--card`, `--line`.
- Effects: radial-gradient background + subtle grid mesh.
- Radius/shadow style: card-based panels with rounded corners.
- Fonts: Space Grotesk (UI), Fraunces (headings), IBM Plex Mono (code/ids).

Implementation approach:
- Define shared CSS variables in `ui/styles.css` adapted from docs.
- Keep button styles (`.btn`, `.btn-primary`, `.btn-ghost`) visually aligned.
- Preserve light theme and contrast from docs.

## Technical Architecture
## Files
- `ui/index.html`: semantic app shell and panels.
- `ui/styles.css`: theme tokens + layout + responsive behavior.
- `ui/app.js`: state, rendering, events, validation, serialization.
- `ui/PLAN.md`: this plan.

## State Model (JS)
```js
state = {
  fileName: null,
  graph: null,
  notesById: new Map(),
  reverseRelated: new Map(),
  longRangeById: new Map(),
  selectedNoteId: null,
  dirty: false,
  filters: { query: '', sort: 'id_asc' },
  validation: { errors: [], warnings: [] }
}
```

## Force Graph Strategy
Library choice: `d3-force` loaded via CDN for zero-build SPA simplicity.

Rendering model:
- Subgraph only (selected note neighborhood), not full graph.
- Node categories: `selected`, `outbound-related`, `inbound-related`, `long-range`.
- Edge categories: `related_out`, `related_in`, `long_range`.
- Max node cap (e.g., 120) to keep physics stable and responsive.

Interactions:
- Drag nodes to settle clusters.
- Zoom/pan canvas.
- Click node to select in editor/list.
- Hover tooltip with `note_id` and short context.

Fallback:
- If d3 fails to load, show a graceful message and keep list/editor fully functional.

## Data Integrity Rules
Validation on load and pre-save:
- Required top-level keys exist and have correct types.
- `notes` entries contain required fields.
- `note_id` unique across notes.
- `related_note_ids` reference existing note_ids (or are cleaned with warning).
- `cluster_labels.length === notes.length`.
- `long_range_links` entries are triples `[number, number, number]` and reference existing notes.
- Unknown extra fields are preserved (do not drop).

Edit validations:
- `raw_content/context` are strings.
- `related_note_ids` parsed as integer ids and deduplicated.

Delete side-effects:
- Purge deleted id from every note’s `related_note_ids`.
- Purge long-range link rows containing deleted id.
- Remove corresponding `cluster_labels` element by note array position.

## Rendering & Performance
Scalability target: handle ~6k notes smoothly (`smg-roo.json`).

Approach:
- Paginated/limited list rendering (avoid full DOM render).
- Debounced search input.
- Force layout only for local neighborhood.
- Avoid embedding large vectors in visible DOM.

## Implementation Phases
1. Scaffold UI shell
- Create HTML structure and baseline styles from docs theme.
- Add responsive breakpoints for desktop/tablet/mobile.

2. File IO + parsing
- Implement file picker with `FileReader`.
- Parse JSON safely with error surfaces.
- Build indexes and initial selection.

3. Explore + search
- Render note list with query/sort.
- Add selection and detail preview.

4. Force graph
- Implement local neighborhood extraction.
- Render d3 force graph with interaction hooks.

5. Edit workflow
- Editable form for selected note.
- Apply/cancel actions with dirty tracking.

6. Delete workflow
- Confirmation modal.
- Cascade cleanup across relations and long-range links.

7. Save workflow
- Validation pass.
- Blob download of updated JSON.

8. Polish
- Keyboard shortcuts (`Ctrl/Cmd+S`).
- Unsaved-changes navigation guard.
- Empty/error states.

## Acceptance Criteria
- Can load `smg-roo.json` without crashing.
- UI remains responsive with 6k+ notes.
- Force graph displays selected local neighborhood with pan/zoom/click.
- User can edit `context`, `raw_content`, and `related_note_ids`.
- User can delete a note and references are cleaned.
- Save produces valid JSON preserving non-edited fields.
- UI matches docs color/typographic theme.
- Works on modern desktop and mobile browsers.

## Risks and Mitigations
- Risk: force layout jank with too many nodes.
- Mitigation: local subgraph cap + simplified link set.

- Risk: large JSON parse/render stalls.
- Mitigation: limited list render, debounced filtering.

- Risk: schema corruption on save.
- Mitigation: strict pre-save validation + unknown-field preservation.

## Manual Test Matrix
- Load valid `smg-roo.json`.
- Load malformed JSON (syntax error).
- Load structurally invalid JSON (missing `notes`).
- Explore graph by clicking list items and graph nodes.
- Edit note text fields and save.
- Edit `related_note_ids` with invalid ids and verify warnings.
- Delete note with many links.
- Save and reopen for roundtrip integrity.
- Test mobile viewport interactions.

## Optional v2 Enhancements
- Cluster-level macro graph and drill-down transitions.
- Web Worker for parse/indexing.
- Diff preview before save.
- WebGL graph renderer for larger neighborhoods.
