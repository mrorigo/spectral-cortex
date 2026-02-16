(() => {
  const state = {
    fileName: null,
    graph: null,
    notesById: new Map(),
    reverseRelated: new Map(),
    longRangeById: new Map(),
    selectedNoteId: null,
    dirty: false,
    filters: { query: "", sort: "id_asc" },
    graphControls: {
      relatedLimit: 3,
      depth: 1,
      viewMode: "neighborhood",
      minScore: 0.3,
      longRangeTopK: 120,
    },
    validation: { errors: [], warnings: [] },
    clusterById: new Map(),
    clusterCounts: new Map(),
  };

  const LIMIT_LIST_RENDER = 300;
  const GRAPH_NODE_CAP = 120;
  const CLUSTER_MAP_NODE_CAP = 1200;
  const CLUSTER_PALETTE = [
    "#1cffd8",
    "#62a8ff",
    "#ff71cf",
    "#b066ff",
    "#ffb347",
    "#7bff8a",
    "#59f0ff",
    "#ff8bf8",
    "#ffe16b",
    "#8c9fff",
    "#4cf7be",
    "#ff9070",
  ];

  const els = {
    fileInput: document.getElementById("file-input"),
    saveBtn: document.getElementById("save-btn"),
    fileName: document.getElementById("file-name"),
    noteCount: document.getElementById("note-count"),
    dirtyFlag: document.getElementById("dirty-flag"),
    validationSummary: document.getElementById("validation-summary"),
    searchInput: document.getElementById("search-input"),
    sortSelect: document.getElementById("sort-select"),
    showLongRange: document.getElementById("show-long-range"),
    relatedLimitSlider: document.getElementById("related-limit-slider"),
    relatedLimitValue: document.getElementById("related-limit-value"),
    depthSlider: document.getElementById("depth-slider"),
    depthValue: document.getElementById("depth-value"),
    viewModeSelect: document.getElementById("view-mode-select"),
    minScoreSlider: document.getElementById("min-score-slider"),
    minScoreValue: document.getElementById("min-score-value"),
    longRangeTopKSlider: document.getElementById("long-range-topk-slider"),
    longRangeTopKValue: document.getElementById("long-range-topk-value"),
    scoreHistogram: document.getElementById("score-histogram"),
    scoreMeta: document.getElementById("score-meta"),
    graphSubtitle: document.getElementById("graph-subtitle"),
    listMeta: document.getElementById("list-meta"),
    noteList: document.getElementById("note-list"),
    graphRoot: document.getElementById("graph-root"),
    selectedNoteMeta: document.getElementById("selected-note-meta"),
    contextInput: document.getElementById("context-input"),
    rawInput: document.getElementById("raw-input"),
    relatedInput: document.getElementById("related-input"),
    readonlyMeta: document.getElementById("readonly-meta"),
    applyBtn: document.getElementById("apply-btn"),
    deleteBtn: document.getElementById("delete-btn"),
  };

  function init() {
    els.fileInput.addEventListener("change", onFileChange);
    els.saveBtn.addEventListener("click", onSave);
    els.searchInput.addEventListener("input", onFilterChange);
    els.sortSelect.addEventListener("change", onFilterChange);
    els.showLongRange.addEventListener("change", () => renderGraph());
    els.relatedLimitSlider.addEventListener("input", onGraphControlChange);
    els.depthSlider.addEventListener("input", onGraphControlChange);
    els.viewModeSelect.addEventListener("change", onGraphControlChange);
    els.minScoreSlider.addEventListener("input", onGraphControlChange);
    els.longRangeTopKSlider.addEventListener("input", onGraphControlChange);
    els.applyBtn.addEventListener("click", applyEditorChanges);
    els.deleteBtn.addEventListener("click", deleteSelectedNote);

    window.addEventListener("beforeunload", (event) => {
      if (!state.dirty) {
        return;
      }
      event.preventDefault();
      event.returnValue = "";
    });

    window.addEventListener("keydown", (event) => {
      const isSave =
        (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "s";
      if (isSave) {
        event.preventDefault();
        if (!els.saveBtn.disabled) {
          onSave();
        }
      }
    });

    syncStatus();
    renderList();
    syncGraphControls();
    renderGraphEmpty("Load an SMG JSON file to visualize the graph.");
  }

  function onGraphControlChange() {
    state.graphControls.relatedLimit = Number.parseInt(
      els.relatedLimitSlider.value,
      10,
    );
    state.graphControls.depth = Number.parseInt(els.depthSlider.value, 10);
    state.graphControls.viewMode = String(els.viewModeSelect.value || "");
    state.graphControls.minScore = Number.parseFloat(els.minScoreSlider.value);
    state.graphControls.longRangeTopK = Number.parseInt(
      els.longRangeTopKSlider.value,
      10,
    );
    syncGraphControls();
    renderGraph();
  }

  function syncGraphControls() {
    els.viewModeSelect.value = state.graphControls.viewMode;
    els.relatedLimitSlider.value = String(state.graphControls.relatedLimit);
    els.depthSlider.value = String(state.graphControls.depth);
    els.minScoreSlider.value = String(state.graphControls.minScore);
    els.longRangeTopKSlider.value = String(state.graphControls.longRangeTopK);
    els.relatedLimitValue.textContent = String(
      state.graphControls.relatedLimit,
    );
    els.depthValue.textContent = String(state.graphControls.depth);
    els.minScoreValue.textContent = state.graphControls.minScore.toFixed(2);
    els.longRangeTopKValue.textContent = String(
      state.graphControls.longRangeTopK,
    );
    syncControlAvailability();
  }

  function syncControlAvailability() {
    const mode = state.graphControls.viewMode;
    const isNeighborhood = mode === "neighborhood";
    const isClusterMap = mode === "cluster_map";
    const isLongRange = mode === "long_range";
    const isClusterMatrix = mode === "cluster_matrix";

    // Neighborhood + cluster map use related budget, long-range-only mode does not.
    els.relatedLimitSlider.disabled = isLongRange || isClusterMatrix;
    // Depth only affects neighborhood expansion.
    els.depthSlider.disabled = !isNeighborhood;
    // Min score affects all graph modes.
    els.minScoreSlider.disabled = false;
    // Long-range top-K affects cluster map, long-range, and cluster-matrix views.
    els.longRangeTopKSlider.disabled = isNeighborhood;
    // Long-range toggle is irrelevant in long-range-only mode.
    els.showLongRange.disabled = isLongRange;
    if (isLongRange) {
      els.showLongRange.checked = true;
    }

    // Keep mode relationships explicit for readability in dev tools.
    els.relatedLimitSlider.dataset.modeActive = String(
      isNeighborhood || isClusterMap,
    );
    els.depthSlider.dataset.modeActive = String(isNeighborhood);
    els.longRangeTopKSlider.dataset.modeActive = String(
      isClusterMap || isLongRange || isClusterMatrix,
    );
  }

  function onFileChange(event) {
    const file = event.target.files?.[0];
    if (!file) {
      return;
    }

    const reader = new FileReader();
    reader.onload = () => {
      try {
        const parsed = JSON.parse(String(reader.result));
        loadGraph(parsed, file.name);
      } catch (error) {
        state.graph = null;
        state.validation = {
          errors: ["Invalid JSON syntax."],
          warnings: [],
        };
        syncStatus();
        renderList();
        renderGraphEmpty("JSON parsing failed.");
        setEditorEnabled(false);
      }
    };
    reader.readAsText(file);
  }

  function loadGraph(graph, fileName) {
    state.fileName = fileName;
    state.graph = graph;
    state.dirty = false;

    state.validation = validateGraph(graph);
    buildIndexes();

    const firstNote = Array.isArray(graph.notes) ? graph.notes[0] : null;
    state.selectedNoteId = firstNote ? firstNote.note_id : null;

    syncStatus();
    renderList();
    renderEditor();
    renderGraph();
  }

  function validateGraph(graph) {
    const errors = [];
    const warnings = [];

    if (!graph || typeof graph !== "object") {
      errors.push("Root JSON must be an object.");
      return { errors, warnings };
    }

    if (!Array.isArray(graph.notes)) {
      errors.push("`notes` must be an array.");
      return { errors, warnings };
    }

    const requiredKeys = [
      "metadata",
      "notes",
      "cluster_labels",
      "cluster_centroids",
      "cluster_centroid_norms",
      "long_range_links",
    ];

    for (const key of requiredKeys) {
      if (!(key in graph)) {
        warnings.push(`Missing top-level key: ${key}`);
      }
    }

    const ids = new Set();
    for (const note of graph.notes) {
      if (typeof note?.note_id !== "number") {
        errors.push("Each note must include numeric `note_id`.");
        continue;
      }
      if (ids.has(note.note_id)) {
        errors.push(`Duplicate note_id: ${note.note_id}`);
      }
      ids.add(note.note_id);

      if (!Array.isArray(note.related_note_links)) {
        errors.push(
          `note ${note.note_id} is missing \`related_note_links\` array.`,
        );
      } else {
        for (const entry of note.related_note_links) {
          const valid =
            Array.isArray(entry) &&
            entry.length === 2 &&
            typeof entry[0] === "number" &&
            typeof entry[1] === "number";
          if (!valid) {
            errors.push(
              `note ${note.note_id} has invalid related_note_links entries; expected [number, number].`,
            );
            break;
          }
        }
      }
    }

    if (
      Array.isArray(graph.cluster_labels) &&
      graph.cluster_labels.length !== graph.notes.length
    ) {
      warnings.push("`cluster_labels` length does not match `notes` length.");
    }

    if (Array.isArray(graph.long_range_links)) {
      for (const link of graph.long_range_links) {
        const valid =
          Array.isArray(link) &&
          link.length === 3 &&
          typeof link[0] === "number" &&
          typeof link[1] === "number" &&
          typeof link[2] === "number";
        if (!valid) {
          warnings.push(
            "Some `long_range_links` entries are not [number, number, number].",
          );
          break;
        }
      }
    } else {
      warnings.push("`long_range_links` should be an array.");
    }

    return { errors, warnings };
  }

  function buildIndexes() {
    state.notesById = new Map();
    state.reverseRelated = new Map();
    state.longRangeById = new Map();
    state.clusterById = new Map();
    state.clusterCounts = new Map();

    if (!state.graph || !Array.isArray(state.graph.notes)) {
      return;
    }

    for (const note of state.graph.notes) {
      if (typeof note.note_id === "number") {
        state.notesById.set(note.note_id, note);
        state.reverseRelated.set(note.note_id, new Map());
        state.longRangeById.set(note.note_id, []);
      }
    }

    if (Array.isArray(state.graph.cluster_labels)) {
      for (const [idx, note] of state.graph.notes.entries()) {
        if (typeof note?.note_id !== "number") {
          continue;
        }
        const clusterId = Number.parseInt(state.graph.cluster_labels[idx], 10);
        if (!Number.isInteger(clusterId)) {
          continue;
        }
        state.clusterById.set(note.note_id, clusterId);
        state.clusterCounts.set(
          clusterId,
          (state.clusterCounts.get(clusterId) || 0) + 1,
        );
      }
    }

    for (const note of state.graph.notes) {
      const srcId = note.note_id;
      const related = Array.isArray(note.related_note_links)
        ? note.related_note_links
        : [];
      for (const entry of related) {
        if (!Array.isArray(entry) || entry.length !== 2) {
          continue;
        }
        const [dstId, score] = entry;
        if (!state.reverseRelated.has(dstId)) {
          state.validation.warnings.push(
            `note ${srcId} references missing related note ${dstId}`,
          );
          continue;
        }
        state.reverseRelated.get(dstId).set(srcId, score);
      }
    }

    const longLinks = Array.isArray(state.graph.long_range_links)
      ? state.graph.long_range_links
      : [];
    for (const entry of longLinks) {
      if (!Array.isArray(entry) || entry.length !== 3) {
        continue;
      }
      const [a, b, score] = entry;
      if (!state.longRangeById.has(a) || !state.longRangeById.has(b)) {
        continue;
      }
      state.longRangeById.get(a).push({ other: b, score });
      state.longRangeById.get(b).push({ other: a, score });
    }
  }

  function syncStatus() {
    const notes = Array.isArray(state.graph?.notes) ? state.graph.notes : [];
    const errCount = state.validation.errors.length;
    const warnCount = state.validation.warnings.length;

    els.fileName.textContent = state.fileName || "None";
    els.noteCount.textContent = String(notes.length);
    els.dirtyFlag.textContent = state.dirty ? "Yes" : "No";
    els.saveBtn.disabled = !state.graph || errCount > 0;

    if (!state.graph) {
      els.validationSummary.textContent = "No file loaded";
      return;
    }

    if (errCount > 0) {
      els.validationSummary.textContent = `${errCount} error(s), ${warnCount} warning(s)`;
      return;
    }

    if (warnCount > 0) {
      els.validationSummary.textContent = `Valid with ${warnCount} warning(s)`;
      return;
    }

    els.validationSummary.textContent = "Valid";
  }

  function onFilterChange() {
    state.filters.query = els.searchInput.value.trim();
    state.filters.sort = els.sortSelect.value;
    renderList();
  }

  function getFilteredNotes() {
    const notes = Array.isArray(state.graph?.notes)
      ? [...state.graph.notes]
      : [];
    const query = state.filters.query.toLowerCase();

    let filtered = notes;
    if (query) {
      filtered = notes.filter((note) => {
        const noteId = String(note.note_id ?? "").toLowerCase();
        const context = String(note.context ?? "").toLowerCase();
        const raw = String(note.raw_content ?? "").toLowerCase();
        const commits = Array.isArray(note.source_commit_ids)
          ? note.source_commit_ids.join(" ").toLowerCase()
          : "";
        return (
          noteId.includes(query) ||
          context.includes(query) ||
          raw.includes(query) ||
          commits.includes(query)
        );
      });
    }

    filtered.sort((a, b) => {
      if (state.filters.sort === "id_desc") {
        return (b.note_id ?? 0) - (a.note_id ?? 0);
      }
      if (state.filters.sort === "timestamp_desc") {
        const aTs = Array.isArray(a.source_timestamps)
          ? Math.max(0, ...a.source_timestamps)
          : 0;
        const bTs = Array.isArray(b.source_timestamps)
          ? Math.max(0, ...b.source_timestamps)
          : 0;
        return bTs - aTs;
      }
      return (a.note_id ?? 0) - (b.note_id ?? 0);
    });

    return filtered;
  }

  function renderList() {
    const notes = getFilteredNotes();
    const shown = notes.slice(0, LIMIT_LIST_RENDER);

    els.noteList.innerHTML = "";

    if (!state.graph) {
      els.listMeta.textContent = "No notes loaded";
      return;
    }

    if (notes.length === 0) {
      els.listMeta.textContent = "No results";
      return;
    }

    els.listMeta.textContent = `Showing ${shown.length}/${notes.length}`;

    for (const note of shown) {
      const button = document.createElement("button");
      button.className = "note-item";
      button.type = "button";
      if (note.note_id === state.selectedNoteId) {
        button.classList.add("active");
      }
      button.addEventListener("click", () => {
        state.selectedNoteId = note.note_id;
        renderList();
        renderEditor();
        renderGraph();
      });

      const idLine = document.createElement("span");
      idLine.className = "note-id";
      idLine.textContent = `#${note.note_id}`;

      const snippet = document.createElement("span");
      snippet.className = "note-snippet";
      snippet.textContent = summarize(
        note.context || note.raw_content || "",
        90,
      );

      button.append(idLine, snippet);
      els.noteList.appendChild(button);
    }
  }

  function renderEditor() {
    const note = state.notesById.get(state.selectedNoteId);
    if (!note) {
      setEditorEnabled(false);
      els.selectedNoteMeta.textContent = "No note selected";
      els.contextInput.value = "";
      els.rawInput.value = "";
      els.relatedInput.value = "";
      els.readonlyMeta.innerHTML = "";
      return;
    }

    setEditorEnabled(true);

    els.selectedNoteMeta.textContent = `note_id=${note.note_id}`;
    els.contextInput.value = String(note.context ?? "");
    els.rawInput.value = String(note.raw_content ?? "");
    els.relatedInput.value = Array.isArray(note.related_note_links)
      ? note.related_note_links
          .map((entry) =>
            Array.isArray(entry) && entry.length === 2
              ? `${entry[0]}:${entry[1].toFixed(3)}`
              : "",
          )
          .filter(Boolean)
          .join(", ")
      : "";

    const sourceTurns = Array.isArray(note.source_turn_ids)
      ? note.source_turn_ids.length
      : 0;
    const sourceCommits = Array.isArray(note.source_commit_ids)
      ? note.source_commit_ids.length
      : 0;
    const sourceTimes = Array.isArray(note.source_timestamps)
      ? note.source_timestamps.length
      : 0;
    const embedDim = Array.isArray(note.embedding) ? note.embedding.length : 0;

    els.readonlyMeta.innerHTML = "";
    for (const [key, value] of [
      ["Embedding Dim", String(embedDim)],
      ["Norm", String(note.norm ?? "n/a")],
      ["Source Turns", String(sourceTurns)],
      ["Source Commits", String(sourceCommits)],
      ["Source Timestamps", String(sourceTimes)],
      [
        "Inbound Links",
        String(state.reverseRelated.get(note.note_id)?.size ?? 0),
      ],
    ]) {
      const cell = document.createElement("div");
      cell.className = "meta-cell";
      cell.innerHTML = `<span class="meta-key">${key}</span>${value}`;
      els.readonlyMeta.appendChild(cell);
    }
  }

  function setEditorEnabled(enabled) {
    els.contextInput.disabled = !enabled;
    els.rawInput.disabled = !enabled;
    els.relatedInput.disabled = !enabled;
    els.applyBtn.disabled = !enabled;
    els.deleteBtn.disabled = !enabled;
  }

  function applyEditorChanges() {
    const note = state.notesById.get(state.selectedNoteId);
    if (!note) {
      return;
    }

    const parsedLinks = parseRelatedLinks(els.relatedInput.value);
    const deduped = new Map();
    for (const [id, score] of parsedLinks) {
      const prev = deduped.get(id);
      deduped.set(id, prev === undefined ? score : Math.max(prev, score));
    }
    const existing = [];
    const dropped = [];
    for (const [id, score] of deduped.entries()) {
      if (state.notesById.has(id)) {
        existing.push([id, score]);
      } else {
        dropped.push(id);
      }
    }

    note.context = els.contextInput.value;
    note.raw_content = els.rawInput.value;
    note.related_note_links = existing;

    if (dropped.length > 0) {
      state.validation.warnings.push(
        `Dropped missing related links for ${note.note_id}: ${dropped.join(", ")}`,
      );
    }

    state.dirty = true;
    buildIndexes();
    syncStatus();
    renderList();
    renderEditor();
    renderGraph();
  }

  function parseRelatedLinks(text) {
    return text
      .split(/[\s,]+/)
      .map((chunk) => chunk.trim())
      .filter(Boolean)
      .map((chunk) => {
        const [idPart, scorePart] = chunk.split(":");
        const id = Number.parseInt(idPart, 10);
        const score =
          scorePart === undefined ? 0 : Number.parseFloat(scorePart.trim());
        if (!Number.isInteger(id)) {
          return null;
        }
        if (!Number.isFinite(score)) {
          return null;
        }
        return [id, score];
      })
      .filter((entry) => entry !== null);
  }

  function deleteSelectedNote() {
    const id = state.selectedNoteId;
    if (!state.notesById.has(id)) {
      return;
    }

    const note = state.notesById.get(id);
    const outbound = Array.isArray(note.related_note_links)
      ? note.related_note_links.length
      : 0;
    const inbound = state.reverseRelated.get(id)?.size ?? 0;
    const ok = window.confirm(
      `Delete note ${id}?\n\nOutbound links: ${outbound}\nInbound links: ${inbound}\n\nThis cannot be undone.`,
    );
    if (!ok) {
      return;
    }

    const notes = state.graph.notes;
    const idx = notes.findIndex((n) => n.note_id === id);
    if (idx < 0) {
      return;
    }

    notes.splice(idx, 1);

    if (
      Array.isArray(state.graph.cluster_labels) &&
      idx < state.graph.cluster_labels.length
    ) {
      state.graph.cluster_labels.splice(idx, 1);
    }

    for (const item of notes) {
      if (!Array.isArray(item.related_note_links)) {
        item.related_note_links = [];
        continue;
      }
      item.related_note_links = item.related_note_links.filter(
        (entry) =>
          Array.isArray(entry) && entry.length === 2 && entry[0] !== id,
      );
    }

    if (Array.isArray(state.graph.long_range_links)) {
      state.graph.long_range_links = state.graph.long_range_links.filter(
        (entry) => {
          if (!Array.isArray(entry) || entry.length !== 3) {
            return false;
          }
          return entry[0] !== id && entry[1] !== id;
        },
      );
    }

    state.dirty = true;
    buildIndexes();

    if (state.graph.notes.length > 0) {
      const safeIndex = Math.min(idx, state.graph.notes.length - 1);
      state.selectedNoteId = state.graph.notes[safeIndex].note_id;
    } else {
      state.selectedNoteId = null;
    }

    syncStatus();
    renderList();
    renderEditor();
    renderGraph();
  }

  function onSave() {
    if (!state.graph) {
      return;
    }

    state.validation = validateGraph(state.graph);
    syncStatus();

    if (state.validation.errors.length > 0) {
      window.alert(
        `Cannot save due to validation errors:\n- ${state.validation.errors.join("\n- ")}`,
      );
      return;
    }

    const baseName = (state.fileName || "smg.json").replace(/\.json$/i, "");
    const outName = `${baseName}-edited.json`;
    const payload = JSON.stringify(state.graph, null, 2);
    const blob = new Blob([payload], { type: "application/json" });
    const url = URL.createObjectURL(blob);

    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = outName;
    document.body.appendChild(anchor);
    anchor.click();
    anchor.remove();

    URL.revokeObjectURL(url);
    state.dirty = false;
    syncStatus();
  }

  function renderGraph() {
    if (!window.d3) {
      renderGraphEmpty("D3 failed to load from CDN.");
      return;
    }
    const data = buildGraphData();
    if (
      !data ||
      ((!data.nodes || data.nodes.length === 0) &&
        data.mode !== "cluster_matrix")
    ) {
      renderScoreHistogram([], 0, null);
      renderGraphEmpty("No graphable data for current mode/filters.");
      return;
    }
    els.graphSubtitle.textContent = data.subtitle;
    renderScoreHistogram(
      data.scoreValues || [],
      data.thresholdRaw || 0,
      data.scoreDomain || null,
    );
    if (data.mode === "cluster_matrix") {
      renderClusterMatrix(data);
      return;
    }

    els.graphRoot.innerHTML = "";
    const width = Math.max(420, els.graphRoot.clientWidth || 640);
    const height = Math.max(340, els.graphRoot.clientHeight || 520);
    const pinnedLayer = document.createElement("div");
    pinnedLayer.className = "graph-pinned-layer";
    els.graphRoot.appendChild(pinnedLayer);
    const pinnedCards = new Map();
    const hoverCard = document.createElement("div");
    hoverCard.className = "graph-hover-card";
    hoverCard.setAttribute("aria-hidden", "true");
    els.graphRoot.appendChild(hoverCard);
    const legend = document.createElement("div");
    legend.className = "graph-legend";
    legend.innerHTML = buildLegendHtml(data.mode);
    els.graphRoot.appendChild(legend);

    const svg = d3
      .select(els.graphRoot)
      .append("svg")
      .attr("width", width)
      .attr("height", height)
      .attr("viewBox", `0 0 ${width} ${height}`);
    const defs = svg.append("defs");
    const linkGlow = defs
      .append("filter")
      .attr("id", "link-neon-glow")
      .attr("x", "-50%")
      .attr("y", "-50%")
      .attr("width", "200%")
      .attr("height", "200%");
    linkGlow.append("feGaussianBlur").attr("stdDeviation", 1.35);
    linkGlow.append("feMerge").html(`
      <feMergeNode />
      <feMergeNode in="SourceGraphic" />
    `);
    const nodeGlow = defs
      .append("filter")
      .attr("id", "node-neon-glow")
      .attr("x", "-60%")
      .attr("y", "-60%")
      .attr("width", "220%")
      .attr("height", "220%");
    nodeGlow.append("feGaussianBlur").attr("stdDeviation", 2.1);
    nodeGlow.append("feMerge").html(`
      <feMergeNode />
      <feMergeNode in="SourceGraphic" />
    `);

    const scene = svg.append("g");
    const minZoomScale =
      data.mode === "cluster_map"
        ? 0.05
        : data.mode === "long_range"
          ? 0.08
          : 0.35;
    const zoomBehavior = d3
      .zoom()
      .scaleExtent([minZoomScale, 4])
      .on("zoom", (event) => {
        scene.attr("transform", event.transform);
      });
    svg.call(zoomBehavior);

    const linkColor = {
      related_out: "#1cffd8",
      related_in: "#62a8ff",
      long_range: "#ff71cf",
    };
    const nodeColor = {
      selected: "#ff71cf",
      outbound: "#1cffd8",
      inbound: "#62a8ff",
      expanded: "#b066ff",
      long_range: "#ffb347",
      cluster: "#7b8cff",
    };

    const scoredValues = data.links
      .map((d) => (Number.isFinite(d.score) ? d.score : 0))
      .sort((a, b) => a - b);
    const minScore = scoredValues.length > 0 ? scoredValues[0] : 0;
    const maxScore =
      scoredValues.length > 0 ? scoredValues[scoredValues.length - 1] : 1;
    const normalizeScore = (value) => {
      const score = Number.isFinite(value) ? value : 0;
      if (maxScore <= minScore) {
        return score > 0 ? 1 : 0;
      }
      return Math.max(
        0,
        Math.min(1, (score - minScore) / (maxScore - minScore)),
      );
    };

    const link = scene
      .append("g")
      .selectAll("line")
      .data(data.links)
      .join("line")
      .attr("stroke-width", (d) => {
        const n = normalizeScore(d.score);
        const base = d.kind === "long_range" ? 1.8 : 1.1;
        const extra = d.kind === "long_range" ? 2.4 : 1.8;
        return base + n * extra;
      })
      .attr("stroke-opacity", (d) => {
        const n = normalizeScore(d.score);
        const base = d.kind === "long_range" ? 0.35 : 0.24;
        const extra = d.kind === "long_range" ? 0.58 : 0.64;
        return Math.max(0.05, Math.min(1, base + n * extra));
      })
      .attr("filter", "url(#link-neon-glow)")
      .attr("stroke", (d) => linkColor[d.kind] || "#8aa");

    // Persist normalized link strength for both rendering and force simulation.
    for (const entry of data.links) {
      entry.strengthNorm = normalizeScore(entry.score);
    }

    const node = scene
      .append("g")
      .selectAll("circle")
      .data(data.nodes)
      .join("circle")
      .attr("r", (d) => (d.kind === "selected" ? 11 : 7.5))
      .attr("fill", (d) => {
        if (d.kind === "cluster") {
          return clusterColor(d.clusterId);
        }
        return nodeColor[d.kind] || "#6a8";
      })
      .attr("stroke", "#2a1749")
      .attr("stroke-width", 1.2)
      .attr("filter", "url(#node-neon-glow)")
      .style("cursor", "pointer")
      .on("mouseenter", (event, d) => {
        const ref = state.notesById.get(d.id);
        hoverCard.innerHTML = buildNodeHoverCardHtml(ref, d, false);
        hoverCard.classList.add("visible");
        positionHoverCard(event, hoverCard, els.graphRoot);
      })
      .on("mousemove", (event) => {
        if (hoverCard.classList.contains("visible")) {
          positionHoverCard(event, hoverCard, els.graphRoot);
        }
      })
      .on("mouseleave", () => {
        hideHoverCard(hoverCard);
      })
      .on("contextmenu", (event, d) => {
        event.preventDefault();
        event.stopPropagation();
        if (pinnedCards.has(d.id)) {
          pinnedCards.get(d.id).remove();
          pinnedCards.delete(d.id);
          return;
        }

        const ref = state.notesById.get(d.id);
        const pinCard = document.createElement("div");
        pinCard.className = "graph-hover-card pinned visible";
        pinCard.innerHTML = buildNodeHoverCardHtml(ref, d, true);
        pinnedLayer.appendChild(pinCard);
        positionHoverCard(event, pinCard, els.graphRoot);
        pinnedCards.set(d.id, pinCard);
      })
      .on("click", (event, d) => {
        event.stopPropagation();
        state.selectedNoteId = d.id;
        renderList();
        renderEditor();
        renderGraph();
      });

    svg.on("click", () => {
      hideHoverCard(hoverCard);
    });

    node.append("title").text((d) => {
      const ref = state.notesById.get(d.id);
      return `#${d.id}\n${summarize(ref?.context || ref?.raw_content || "", 120)}`;
    });

    const label = scene
      .append("g")
      .selectAll("text")
      .data(data.nodes)
      .join("text")
      .attr("class", "graph-node-label")
      .attr("dx", 9)
      .attr("dy", 3)
      .text((d) => {
        if (d.kind === "selected") {
          return `#${d.id}`;
        }
        if (data.nodes.length <= 220) {
          return `#${d.id}`;
        }
        return "";
      });

    const simulation = d3
      .forceSimulation(data.nodes)
      .force(
        "link",
        d3
          .forceLink(data.links)
          .id((d) => d.id)
          .distance((d) => {
            const n = Number.isFinite(d.strengthNorm) ? d.strengthNorm : 0;
            if (d.kind === "long_range") {
              return 145 - n * 55;
            }
            return 90 - n * 45;
          })
          .strength((d) => {
            const n = Number.isFinite(d.strengthNorm) ? d.strengthNorm : 0;
            const base = d.kind === "long_range" ? 0.08 : 0.15;
            const extra = d.kind === "long_range" ? 0.22 : 0.45;
            return base + n * extra;
          }),
      )
      .force(
        "charge",
        d3
          .forceManyBody()
          .strength(
            data.mode === "cluster_map"
              ? -105
              : data.mode === "long_range"
                ? -130
                : -180,
          ),
      )
      .force("center", d3.forceCenter(width / 2, height / 2))
      .force(
        "collision",
        d3
          .forceCollide()
          .radius((d) =>
            d.kind === "selected" ? 14 : data.mode === "cluster_map" ? 8 : 10,
          ),
      )
      .on("tick", ticked);

    node.call(
      d3
        .drag()
        .on("start", (event) => {
          if (!event.active) {
            simulation.alphaTarget(0.3).restart();
          }
          event.subject.fx = event.subject.x;
          event.subject.fy = event.subject.y;
        })
        .on("drag", (event) => {
          event.subject.fx = event.x;
          event.subject.fy = event.y;
        })
        .on("end", (event) => {
          if (!event.active) {
            simulation.alphaTarget(0);
          }
          event.subject.fx = null;
          event.subject.fy = null;
        }),
    );

    // Pre-settle briefly, then fit the full neighborhood in view.
    simulation.stop();
    for (let i = 0; i < (data.mode === "cluster_map" ? 26 : 80); i += 1) {
      simulation.tick();
    }
    ticked();
    zoomToFit(
      svg,
      zoomBehavior,
      data.nodes,
      width,
      height,
      data.mode === "cluster_map" ? 44 : data.mode === "long_range" ? 38 : 26,
      data.mode === "cluster_map"
        ? 0.05
        : data.mode === "long_range"
          ? 0.08
          : 0.35,
    );
    if (data.mode === "neighborhood") {
      simulation.alpha(0.35).restart();
    }

    function ticked() {
      link
        .attr("x1", (d) => d.source.x)
        .attr("y1", (d) => d.source.y)
        .attr("x2", (d) => d.target.x)
        .attr("y2", (d) => d.target.y);
      node.attr("cx", (d) => d.x).attr("cy", (d) => d.y);
      label.attr("x", (d) => d.x).attr("y", (d) => d.y);
    }
  }

  function zoomToFit(
    svg,
    zoomBehavior,
    nodes,
    width,
    height,
    padding,
    minScale = 0.35,
  ) {
    if (!nodes.length) {
      return;
    }
    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;

    for (const n of nodes) {
      if (!Number.isFinite(n.x) || !Number.isFinite(n.y)) {
        continue;
      }
      if (n.x < minX) minX = n.x;
      if (n.y < minY) minY = n.y;
      if (n.x > maxX) maxX = n.x;
      if (n.y > maxY) maxY = n.y;
    }

    if (!Number.isFinite(minX) || !Number.isFinite(minY)) {
      return;
    }

    const boxW = Math.max(1, maxX - minX);
    const boxH = Math.max(1, maxY - minY);
    const fitW = Math.max(1, width - padding * 2);
    const fitH = Math.max(1, height - padding * 2);
    const scale = Math.max(
      minScale,
      Math.min(4, Math.min(fitW / boxW, fitH / boxH)),
    );
    const centerX = (minX + maxX) / 2;
    const centerY = (minY + maxY) / 2;
    const tx = width / 2 - centerX * scale;
    const ty = height / 2 - centerY * scale;

    svg.call(
      zoomBehavior.transform,
      d3.zoomIdentity.translate(tx, ty).scale(scale),
    );
  }

  function computeScoreDomain(scores) {
    const clean = Array.isArray(scores)
      ? scores.filter((v) => Number.isFinite(v))
      : [];
    if (clean.length === 0) {
      return null;
    }
    let min = Infinity;
    let max = -Infinity;
    for (const v of clean) {
      if (v < min) min = v;
      if (v > max) max = v;
    }
    return { min, max };
  }

  function quantile(sortedValues, q) {
    if (!Array.isArray(sortedValues) || sortedValues.length === 0) {
      return 0;
    }
    const clampedQ = Math.max(0, Math.min(1, q));
    if (sortedValues.length === 1) {
      return sortedValues[0];
    }
    const pos = clampedQ * (sortedValues.length - 1);
    const lo = Math.floor(pos);
    const hi = Math.ceil(pos);
    if (lo === hi) {
      return sortedValues[lo];
    }
    const w = pos - lo;
    return sortedValues[lo] * (1 - w) + sortedValues[hi] * w;
  }

  function computeDistribution(values, options = {}) {
    const clean = Array.isArray(values)
      ? values.filter((v) => Number.isFinite(v))
      : [];
    if (clean.length === 0) {
      return null;
    }
    const useLog = options.log === true;
    const transformed = clean
      .map((v) => (useLog ? Math.log1p(Math.max(0, v)) : v))
      .sort((a, b) => a - b);
    const min = transformed[0];
    const max = transformed[transformed.length - 1];
    const p10 = quantile(transformed, 0.1);
    const p90 = quantile(transformed, 0.9);
    return {
      useLog,
      min,
      max,
      p10,
      p90,
    };
  }

  function normalizeByDistribution(value, distribution) {
    if (!distribution || !Number.isFinite(value)) {
      return 0;
    }
    const x = distribution.useLog ? Math.log1p(Math.max(0, value)) : value;
    const lo = Number.isFinite(distribution.p10)
      ? distribution.p10
      : distribution.min;
    const hi = Number.isFinite(distribution.p90)
      ? distribution.p90
      : distribution.max;
    const span = hi - lo;
    if (span > 1e-9) {
      return Math.max(0, Math.min(1, (x - lo) / span));
    }
    const fallback = distribution.max - distribution.min;
    if (fallback > 1e-9) {
      return Math.max(
        0,
        Math.min(
          1,
          (x - distribution.min) / (distribution.max - distribution.min),
        ),
      );
    }
    return 1;
  }

  function normalizedToRawScore(normalized, domain) {
    const norm = Math.max(
      0,
      Math.min(1, Number.isFinite(normalized) ? normalized : 0),
    );
    if (
      !domain ||
      !Number.isFinite(domain.min) ||
      !Number.isFinite(domain.max)
    ) {
      return 0;
    }
    if (domain.max <= domain.min) {
      return domain.min;
    }
    return domain.min + norm * (domain.max - domain.min);
  }

  function buildGraphData() {
    const mode = state.graphControls.viewMode;
    const selectedId =
      typeof state.selectedNoteId === "number" ? state.selectedNoteId : null;

    if (mode === "cluster_map") {
      return buildClusterMapData(selectedId);
    }
    if (mode === "long_range") {
      return buildLongRangeData(selectedId);
    }
    if (mode === "cluster_matrix") {
      return buildClusterMatrixData(selectedId);
    }
    if (!state.notesById.has(selectedId)) {
      return {
        mode,
        subtitle: "No selected note",
        nodes: [],
        links: [],
        scoreValues: [],
        scoreDomain: null,
        thresholdRaw: 0,
      };
    }
    const neighborhood = buildNeighborhoodData(selectedId);
    return {
      mode: "neighborhood",
      subtitle: `Selected note neighborhood #${selectedId}`,
      nodes: neighborhood.nodes,
      links: neighborhood.links,
      scoreValues: neighborhood.scoreValues,
      scoreDomain: neighborhood.scoreDomain,
      thresholdRaw: neighborhood.thresholdRaw,
    };
  }

  function buildClusterMapData(selectedId) {
    const notes = Array.isArray(state.graph?.notes) ? state.graph.notes : [];
    const sortedIds = notes
      .map((n) => n?.note_id)
      .filter((id) => Number.isInteger(id))
      .sort((a, b) => a - b);
    if (sortedIds.length === 0) {
      return {
        mode: "cluster_map",
        subtitle: "Cluster map",
        nodes: [],
        links: [],
      };
    }

    const ordered = [];
    const seen = new Set();
    const pushOrdered = (id) => {
      if (!Number.isInteger(id) || seen.has(id) || !state.notesById.has(id)) {
        return;
      }
      if (ordered.length >= CLUSTER_MAP_NODE_CAP) {
        return;
      }
      seen.add(id);
      ordered.push(id);
    };

    if (Number.isInteger(selectedId)) {
      pushOrdered(selectedId);
    }

    if (state.clusterCounts.size > 0) {
      const clusterEntries = Array.from(state.clusterCounts.entries()).sort(
        (a, b) => a[0] - b[0],
      );
      const perClusterCap = Math.max(
        1,
        Math.floor(CLUSTER_MAP_NODE_CAP / Math.max(1, clusterEntries.length)),
      );
      for (const [clusterId] of clusterEntries) {
        const clusterIds = sortedIds.filter(
          (id) => state.clusterById.get(id) === clusterId,
        );
        for (const id of clusterIds.slice(0, perClusterCap)) {
          pushOrdered(id);
        }
      }
    }

    for (const id of sortedIds) {
      if (ordered.length >= CLUSTER_MAP_NODE_CAP) {
        break;
      }
      pushOrdered(id);
    }
    const allowed = new Set(ordered);
    const nodes = ordered.map((id) => ({
      id,
      kind: id === selectedId ? "selected" : "cluster",
      clusterId: state.clusterById.get(id),
    }));

    const scoreValues = [];
    const perNodeBudget = Math.max(
      1,
      Math.min(3, state.graphControls.relatedLimit),
    );
    for (const srcId of ordered) {
      const note = state.notesById.get(srcId);
      if (!note || !Array.isArray(note.related_note_links)) {
        continue;
      }
      const related = note.related_note_links
        .filter(
          (entry) =>
            Array.isArray(entry) &&
            entry.length === 2 &&
            Number.isInteger(entry[0]) &&
            Number.isFinite(entry[1]),
        )
        .sort((a, b) => b[1] - a[1])
        .slice(0, perNodeBudget);
      for (const [, score] of related) {
        scoreValues.push(score);
      }
    }
    if (els.showLongRange.checked) {
      const topK = Math.max(
        1,
        Math.min(220, state.graphControls.longRangeTopK),
      );
      const longLinks = Array.isArray(state.graph?.long_range_links)
        ? state.graph.long_range_links
        : [];
      for (const entry of longLinks
        .filter(
          (e) =>
            Array.isArray(e) &&
            e.length === 3 &&
            Number.isInteger(e[0]) &&
            Number.isInteger(e[1]) &&
            Number.isFinite(e[2]),
        )
        .sort((a, b) => b[2] - a[2])
        .slice(0, topK)) {
        scoreValues.push(entry[2]);
      }
    }
    const scoreDomain = computeScoreDomain(scoreValues);
    const minScore = normalizedToRawScore(
      state.graphControls.minScore,
      scoreDomain,
    );
    const edgeSet = new Set();
    const links = [];

    for (const srcId of ordered) {
      const note = state.notesById.get(srcId);
      if (!note || !Array.isArray(note.related_note_links)) {
        continue;
      }
      const related = note.related_note_links
        .filter(
          (entry) =>
            Array.isArray(entry) &&
            entry.length === 2 &&
            Number.isInteger(entry[0]) &&
            Number.isFinite(entry[1]) &&
            entry[1] >= minScore,
        )
        .sort((a, b) => b[1] - a[1])
        .slice(0, perNodeBudget);

      for (const [dstId, score] of related) {
        if (!allowed.has(dstId)) {
          continue;
        }
        const a = Math.min(srcId, dstId);
        const b = Math.max(srcId, dstId);
        const edgeKey = `${a}:${b}:related`;
        if (edgeSet.has(edgeKey)) {
          continue;
        }
        edgeSet.add(edgeKey);
        links.push({
          source: srcId,
          target: dstId,
          kind: "related_out",
          score,
        });
      }
    }

    if (els.showLongRange.checked) {
      const topK = Math.max(
        1,
        Math.min(220, state.graphControls.longRangeTopK),
      );
      const longLinks = Array.isArray(state.graph?.long_range_links)
        ? state.graph.long_range_links
        : [];
      const filteredLong = longLinks
        .filter(
          (entry) =>
            Array.isArray(entry) &&
            entry.length === 3 &&
            Number.isInteger(entry[0]) &&
            Number.isInteger(entry[1]) &&
            Number.isFinite(entry[2]) &&
            entry[2] >= minScore,
        )
        .sort((a, b) => b[2] - a[2])
        .slice(0, topK);
      for (const [a, b, score] of filteredLong) {
        if (!allowed.has(a) || !allowed.has(b)) {
          continue;
        }
        const edgeKey = `${Math.min(a, b)}:${Math.max(a, b)}:long`;
        if (edgeSet.has(edgeKey)) {
          continue;
        }
        edgeSet.add(edgeKey);
        links.push({ source: a, target: b, kind: "long_range", score });
      }
    }

    const suffix =
      sortedIds.length > ordered.length
        ? ` (showing ${ordered.length}/${sortedIds.length} nodes)`
        : "";
    return {
      mode: "cluster_map",
      subtitle: `Global cluster map${suffix}`,
      nodes,
      links,
      scoreValues,
      scoreDomain,
      thresholdRaw: minScore,
    };
  }

  function buildLongRangeData(selectedId) {
    const topK = Math.max(1, state.graphControls.longRangeTopK);
    const longLinks = Array.isArray(state.graph?.long_range_links)
      ? state.graph.long_range_links
      : [];
    const scoreValues = longLinks
      .filter(
        (entry) =>
          Array.isArray(entry) &&
          entry.length === 3 &&
          Number.isInteger(entry[0]) &&
          Number.isInteger(entry[1]) &&
          Number.isFinite(entry[2]),
      )
      .map((entry) => entry[2]);
    const scoreDomain = computeScoreDomain(scoreValues);
    const minScore = normalizedToRawScore(
      state.graphControls.minScore,
      scoreDomain,
    );
    const filtered = longLinks
      .filter(
        (entry) =>
          Array.isArray(entry) &&
          entry.length === 3 &&
          Number.isInteger(entry[0]) &&
          Number.isInteger(entry[1]) &&
          Number.isFinite(entry[2]) &&
          entry[2] >= minScore,
      )
      .sort((a, b) => b[2] - a[2])
      .slice(0, topK);

    const nodeKinds = new Map();
    for (const [a, b] of filtered) {
      if (state.notesById.has(a)) {
        nodeKinds.set(a, "long_range");
      }
      if (state.notesById.has(b)) {
        nodeKinds.set(b, "long_range");
      }
    }
    if (Number.isInteger(selectedId) && nodeKinds.has(selectedId)) {
      nodeKinds.set(selectedId, "selected");
    }

    const nodes = Array.from(nodeKinds.entries()).map(([id, kind]) => ({
      id,
      kind,
      clusterId: state.clusterById.get(id),
    }));
    const links = filtered.map(([a, b, score]) => ({
      source: a,
      target: b,
      kind: "long_range",
      score,
    }));

    return {
      mode: "long_range",
      subtitle: `Long-range links (top ${topK})`,
      nodes,
      links,
      scoreValues,
      scoreDomain,
      thresholdRaw: minScore,
    };
  }

  function buildClusterMatrixData(selectedId) {
    const clusters = Array.from(state.clusterCounts.keys()).sort(
      (a, b) => a - b,
    );
    if (clusters.length === 0) {
      return {
        mode: "cluster_matrix",
        subtitle: "Cluster matrix unavailable (missing cluster labels)",
        clusters: [],
        matrix: new Map(),
        maxCount: 0,
        maxMean: 0,
        scoreValues: [],
        scoreDomain: null,
        thresholdRaw: 0,
      };
    }

    const scoreValues = [];
    const matrix = new Map();
    const edgeSet = new Set();

    const addClusterEdge = (srcId, dstId, score, relationKind) => {
      if (!Number.isInteger(srcId) || !Number.isInteger(dstId)) {
        return;
      }
      if (!Number.isFinite(score)) {
        return;
      }
      const a = Math.min(srcId, dstId);
      const b = Math.max(srcId, dstId);
      const edgeKey = `${a}:${b}:${relationKind}`;
      if (edgeSet.has(edgeKey)) {
        return;
      }
      edgeSet.add(edgeKey);

      const ca = state.clusterById.get(srcId);
      const cb = state.clusterById.get(dstId);
      if (!Number.isInteger(ca) || !Number.isInteger(cb)) {
        return;
      }
      scoreValues.push(score);
      const c0 = Math.min(ca, cb);
      const c1 = Math.max(ca, cb);
      const key = `${c0}:${c1}`;
      if (!matrix.has(key)) {
        matrix.set(key, {
          count: 0,
          sum: 0,
          max: 0,
          sampleSrc: srcId,
          sampleDst: dstId,
        });
      }
      const stat = matrix.get(key);
      stat.count += 1;
      stat.sum += score;
      if (score > stat.max) {
        stat.max = score;
      }
    };

    for (const note of state.graph?.notes || []) {
      const srcId = note?.note_id;
      if (!Number.isInteger(srcId) || !Array.isArray(note.related_note_links)) {
        continue;
      }
      for (const entry of note.related_note_links) {
        if (
          Array.isArray(entry) &&
          entry.length === 2 &&
          Number.isInteger(entry[0]) &&
          Number.isFinite(entry[1])
        ) {
          addClusterEdge(srcId, entry[0], entry[1], "related");
        }
      }
    }

    if (
      els.showLongRange.checked &&
      Array.isArray(state.graph?.long_range_links)
    ) {
      const topK = Math.max(1, state.graphControls.longRangeTopK);
      const longLinks = state.graph.long_range_links
        .filter(
          (entry) =>
            Array.isArray(entry) &&
            entry.length === 3 &&
            Number.isInteger(entry[0]) &&
            Number.isInteger(entry[1]) &&
            Number.isFinite(entry[2]),
        )
        .sort((a, b) => b[2] - a[2])
        .slice(0, topK);
      for (const [a, b, score] of longLinks) {
        addClusterEdge(a, b, score, "long");
      }
    }

    const scoreDomain = computeScoreDomain(scoreValues);
    const thresholdRaw = normalizedToRawScore(
      state.graphControls.minScore,
      scoreDomain,
    );

    let maxCount = 0;
    let maxMean = 0;
    const visibleCounts = [];
    const visibleMeans = [];
    for (const stat of matrix.values()) {
      if (!stat || stat.max < thresholdRaw) {
        continue;
      }
      if (stat.count > maxCount) {
        maxCount = stat.count;
      }
      const mean = stat.sum / Math.max(1, stat.count);
      visibleCounts.push(stat.count);
      visibleMeans.push(mean);
      if (mean > maxMean) {
        maxMean = mean;
      }
    }
    const countDistribution = computeDistribution(visibleCounts, { log: true });
    const meanDistribution = computeDistribution(visibleMeans, { log: false });

    const selectedCluster = Number.isInteger(selectedId)
      ? state.clusterById.get(selectedId)
      : null;
    const suffix = els.showLongRange.checked
      ? ` (related + long-range top ${state.graphControls.longRangeTopK})`
      : " (related only)";
    return {
      mode: "cluster_matrix",
      subtitle: `Cluster-to-cluster matrix${suffix}`,
      clusters,
      matrix,
      maxCount,
      maxMean,
      countDistribution,
      meanDistribution,
      selectedCluster,
      scoreValues,
      scoreDomain,
      thresholdRaw,
    };
  }

  function renderClusterMatrix(data) {
    els.graphRoot.innerHTML = "";

    if (!Array.isArray(data.clusters) || data.clusters.length === 0) {
      renderGraphEmpty("Cluster labels missing; cannot build cluster matrix.");
      return;
    }

    const wrap = document.createElement("div");
    wrap.className = "cluster-matrix-wrap";

    const table = document.createElement("table");
    table.className = "cluster-matrix";

    const thead = document.createElement("thead");
    const headRow = document.createElement("tr");
    const corner = document.createElement("th");
    corner.className = "cluster-matrix-corner mono";
    corner.textContent = "C↔C";
    headRow.appendChild(corner);
    for (const cid of data.clusters) {
      const th = document.createElement("th");
      th.className = "cluster-matrix-header mono";
      th.textContent = `C${cid}`;
      if (cid === data.selectedCluster) {
        th.classList.add("selected");
      }
      headRow.appendChild(th);
    }
    thead.appendChild(headRow);
    table.appendChild(thead);

    const tbody = document.createElement("tbody");
    for (const r of data.clusters) {
      const tr = document.createElement("tr");
      const rowHead = document.createElement("th");
      rowHead.className = "cluster-matrix-header mono";
      rowHead.textContent = `C${r}`;
      if (r === data.selectedCluster) {
        rowHead.classList.add("selected");
      }
      tr.appendChild(rowHead);

      for (const c of data.clusters) {
        const key = `${Math.min(r, c)}:${Math.max(r, c)}`;
        const stat = data.matrix.get(key);
        const td = document.createElement("td");
        td.className = "cluster-matrix-cell";
        if (!stat || stat.max < data.thresholdRaw) {
          td.classList.add("empty");
          td.textContent = "·";
          tr.appendChild(td);
          continue;
        }
        const mean = stat.sum / Math.max(1, stat.count);
        const countNorm = Math.pow(
          normalizeByDistribution(stat.count, data.countDistribution),
          0.8,
        );
        const meanNorm = Math.pow(
          normalizeByDistribution(mean, data.meanDistribution),
          0.8,
        );
        const strength = Math.max(
          0,
          Math.min(1, 0.65 * meanNorm + 0.35 * countNorm),
        );
        td.style.background = `linear-gradient(135deg, rgba(28,255,216,${
          0.1 + 0.64 * meanNorm
        }), rgba(98,168,255,${0.1 + 0.56 * countNorm}))`;
        td.style.borderColor = `rgba(176, 102, 255, ${0.24 + 0.42 * strength})`;
        td.style.boxShadow = `inset 0 0 ${Math.round(4 + 10 * strength)}px rgba(160,120,255,${
          0.08 + 0.2 * strength
        })`;
        td.innerHTML = `<span class="mono">${stat.count}</span><small>${mean.toFixed(3)}</small>`;
        td.title = `Clusters C${r} ↔ C${c}\nlinks=${stat.count}\nmean=${mean.toFixed(
          4,
        )}\nmax=${stat.max.toFixed(4)}\nintensity=${strength.toFixed(3)}`;
        td.addEventListener("click", () => {
          const candidate = Number.isInteger(stat.sampleSrc)
            ? stat.sampleSrc
            : stat.sampleDst;
          if (Number.isInteger(candidate) && state.notesById.has(candidate)) {
            state.selectedNoteId = candidate;
            renderList();
            renderEditor();
          }
        });
        tr.appendChild(td);
      }
      tbody.appendChild(tr);
    }
    table.appendChild(tbody);

    const legend = document.createElement("div");
    legend.className = "cluster-matrix-meta muted";
    legend.textContent =
      `Cells show link count and mean similarity. ` +
      `Intensity uses normalized count/log-count and mean quantiles. ` +
      `Threshold >= ${data.thresholdRaw.toFixed(3)}.`;

    wrap.appendChild(table);
    wrap.appendChild(legend);
    els.graphRoot.appendChild(wrap);
  }

  function clusterColor(clusterId) {
    const cid = Number.parseInt(clusterId, 10);
    if (!Number.isInteger(cid) || cid < 0) {
      return "#7b8cff";
    }
    return CLUSTER_PALETTE[cid % CLUSTER_PALETTE.length];
  }

  function buildLegendHtml(mode) {
    if (mode === "cluster_map") {
      const clusterRows = Array.from(state.clusterCounts.entries())
        .sort((a, b) => a[0] - b[0])
        .slice(0, 8)
        .map(
          ([clusterId, count]) =>
            `<div class="legend-row"><span class="legend-swatch" style="--swatch:${clusterColor(clusterId)}"></span>Cluster ${clusterId} (${count})</div>`,
        )
        .join("");
      return `
        <div class="legend-title">Legend</div>
        <div class="legend-row"><span class="legend-swatch" style="--swatch:#ff71cf"></span>Selected node</div>
        <div class="legend-row"><span class="legend-swatch line" style="--swatch:#1cffd8"></span>Related links</div>
        <div class="legend-row"><span class="legend-swatch line" style="--swatch:#ff71cf"></span>Long-range links</div>
        ${clusterRows}
      `;
    }
    if (mode === "long_range") {
      return `
        <div class="legend-title">Legend</div>
        <div class="legend-row"><span class="legend-swatch" style="--swatch:#ff71cf"></span>Selected node</div>
        <div class="legend-row"><span class="legend-swatch" style="--swatch:#ffb347"></span>Long-range node</div>
        <div class="legend-row"><span class="legend-swatch line" style="--swatch:#ff71cf"></span>Long-range link</div>
      `;
    }
    return `
      <div class="legend-title">Legend</div>
      <div class="legend-row"><span class="legend-swatch" style="--swatch:#ff71cf"></span>Selected node</div>
      <div class="legend-row"><span class="legend-swatch" style="--swatch:#1cffd8"></span>Outbound / related out</div>
      <div class="legend-row"><span class="legend-swatch" style="--swatch:#62a8ff"></span>Inbound / related in</div>
      <div class="legend-row"><span class="legend-swatch" style="--swatch:#b066ff"></span>Expanded node</div>
      <div class="legend-row"><span class="legend-swatch" style="--swatch:#ffb347"></span>Long-range node</div>
      <div class="legend-row"><span class="legend-swatch line" style="--swatch:#ff71cf"></span>Long-range link</div>
    `;
  }

  function renderScoreHistogram(values, thresholdRaw, scoreDomain) {
    if (!els.scoreHistogram || !els.scoreMeta) {
      return;
    }
    const scores = Array.isArray(values)
      ? values.filter((v) => Number.isFinite(v))
      : [];
    if (scores.length === 0) {
      els.scoreHistogram.innerHTML = "";
      els.scoreMeta.textContent = "No edges";
      return;
    }
    const domain = scoreDomain || computeScoreDomain(scores);
    if (!domain) {
      els.scoreHistogram.innerHTML = "";
      els.scoreMeta.textContent = "No edges";
      return;
    }
    const bins = new Array(20).fill(0);
    const span = Math.max(1e-9, domain.max - domain.min);
    const normThreshold = Math.max(
      0,
      Math.min(1, (thresholdRaw - domain.min) / span),
    );
    for (const score of scores) {
      const normalized = Math.max(0, Math.min(1, (score - domain.min) / span));
      const idx = Math.min(19, Math.floor(normalized * 20));
      bins[idx] += 1;
    }
    const maxBin = Math.max(...bins, 1);
    els.scoreHistogram.innerHTML = bins
      .map((count, idx) => {
        const x = idx / 20;
        const h = Math.max(8, Math.round((count / maxBin) * 100));
        const active = x + 1 / 20 >= normThreshold ? " active" : "";
        return `<span class="score-bar${active}" style="height:${h}%"></span>`;
      })
      .join("");
    const above = scores.filter((s) => s >= thresholdRaw).length;
    els.scoreMeta.textContent =
      `${scores.length} edges, ${above} >= ${thresholdRaw.toFixed(3)} ` +
      `(range ${domain.min.toFixed(3)}-${domain.max.toFixed(3)})`;
  }

  function buildNeighborhoodData(selectedId) {
    const nodes = [];
    const links = [];
    const nodeKinds = new Map([[selectedId, "selected"]]);
    const depth = Math.max(1, Math.min(3, state.graphControls.depth));
    const relatedLimit = Math.max(1, state.graphControls.relatedLimit);
    const scoreValues = [];
    const selectedNote = state.notesById.get(selectedId);
    if (selectedNote && Array.isArray(selectedNote.related_note_links)) {
      for (const entry of selectedNote.related_note_links.slice(
        0,
        relatedLimit,
      )) {
        if (
          Array.isArray(entry) &&
          entry.length === 2 &&
          Number.isFinite(entry[1])
        ) {
          scoreValues.push(entry[1]);
        }
      }
    }
    const selectedInbound = Array.from(
      state.reverseRelated.get(selectedId)?.entries() || [],
    ).slice(0, relatedLimit);
    for (const entry of selectedInbound) {
      if (
        Array.isArray(entry) &&
        entry.length === 2 &&
        Number.isFinite(entry[1])
      ) {
        scoreValues.push(entry[1]);
      }
    }
    const selectedLongRange = (state.longRangeById.get(selectedId) || []).slice(
      0,
      Math.min(relatedLimit, state.graphControls.longRangeTopK, 50),
    );
    for (const entry of selectedLongRange) {
      if (entry && Number.isFinite(entry.score)) {
        scoreValues.push(entry.score);
      }
    }
    const initialDomain = computeScoreDomain(scoreValues);
    const thresholdRaw = normalizedToRawScore(
      state.graphControls.minScore,
      initialDomain,
    );
    const visitedDepth = new Map([[selectedId, 0]]);
    const queue = [{ id: selectedId, depth: 0 }];
    const edgeSet = new Set();

    const pushNode = (id, kind) => {
      if (!state.notesById.has(id)) {
        return;
      }
      if (!nodeKinds.has(id)) {
        nodeKinds.set(id, kind);
        return;
      }
      const rank = {
        selected: 4,
        outbound: 3,
        inbound: 3,
        expanded: 2,
        long_range: 1,
      };
      if (rank[kind] > (rank[nodeKinds.get(id)] || 0)) {
        nodeKinds.set(id, kind);
      }
    };

    while (queue.length > 0) {
      const current = queue.shift();
      if (!current || current.depth >= depth) {
        continue;
      }
      const srcId = current.id;
      const note = state.notesById.get(srcId);
      if (!note) {
        continue;
      }

      const outbound = Array.isArray(note.related_note_links)
        ? note.related_note_links
            .filter(
              (entry) =>
                Array.isArray(entry) &&
                entry.length === 2 &&
                Number.isInteger(entry[0]) &&
                Number.isFinite(entry[1]),
            )
            .sort((a, b) => b[1] - a[1])
        : [];
      const inbound = Array.from(
        state.reverseRelated.get(srcId)?.entries() || [],
      )
        .filter(
          (entry) =>
            Array.isArray(entry) &&
            entry.length === 2 &&
            Number.isInteger(entry[0]) &&
            Number.isFinite(entry[1]),
        )
        .sort((a, b) => b[1] - a[1] || a[0] - b[0]);
      const budget = Math.max(0, relatedLimit);
      let used = 0;

      for (const [, score] of outbound.slice(0, budget)) {
        scoreValues.push(score);
      }
      for (const [, score] of inbound.slice(0, budget)) {
        scoreValues.push(score);
      }
      for (const [dstId, score] of outbound) {
        if (used >= budget) {
          break;
        }
        if (!state.notesById.has(dstId)) {
          continue;
        }
        used += 1;
        if (score < thresholdRaw) {
          continue;
        }
        pushNode(dstId, current.depth === 0 ? "outbound" : "expanded");
        const edgeKey = `${srcId}->${dstId}:out`;
        if (!edgeSet.has(edgeKey)) {
          edgeSet.add(edgeKey);
          links.push({
            source: srcId,
            target: dstId,
            kind: "related_out",
            score,
          });
        }
        const nextDepth = current.depth + 1;
        if (
          !visitedDepth.has(dstId) ||
          nextDepth < (visitedDepth.get(dstId) ?? Number.MAX_SAFE_INTEGER)
        ) {
          visitedDepth.set(dstId, nextDepth);
          queue.push({ id: dstId, depth: nextDepth });
        }
      }

      for (const [dstId, score] of inbound) {
        if (used >= budget) {
          break;
        }
        if (!state.notesById.has(dstId)) {
          continue;
        }
        used += 1;
        if (score < thresholdRaw) {
          continue;
        }
        pushNode(dstId, current.depth === 0 ? "inbound" : "expanded");
        const edgeKey = `${dstId}->${srcId}:in`;
        if (!edgeSet.has(edgeKey)) {
          edgeSet.add(edgeKey);
          links.push({
            source: dstId,
            target: srcId,
            kind: "related_in",
            score,
          });
        }
        const nextDepth = current.depth + 1;
        if (
          !visitedDepth.has(dstId) ||
          nextDepth < (visitedDepth.get(dstId) ?? Number.MAX_SAFE_INTEGER)
        ) {
          visitedDepth.set(dstId, nextDepth);
          queue.push({ id: dstId, depth: nextDepth });
        }
      }
    }

    if (els.showLongRange.checked) {
      const longRange = (state.longRangeById.get(selectedId) || [])
        .filter((entry) => Number.isFinite(entry.score))
        .sort((a, b) => b.score - a.score);
      const longRangeLimit = Math.min(
        relatedLimit,
        50,
        state.graphControls.longRangeTopK,
      );
      for (const entry of longRange.slice(0, longRangeLimit)) {
        scoreValues.push(entry.score);
      }
      for (const entry of longRange.slice(0, longRangeLimit)) {
        if (entry.score < thresholdRaw) {
          continue;
        }
        pushNode(entry.other, "long_range");
        links.push({
          source: selectedId,
          target: entry.other,
          kind: "long_range",
          score: Number.isFinite(entry.score) ? entry.score : 0,
        });
      }
    }

    const allIds = Array.from(nodeKinds.keys()).slice(0, GRAPH_NODE_CAP);
    const allowed = new Set(allIds);

    for (const id of allIds) {
      nodes.push({ id, kind: nodeKinds.get(id) });
    }

    const filteredLinks = links.filter(
      (l) => allowed.has(l.source) && allowed.has(l.target),
    );

    const scoreDomain = computeScoreDomain(scoreValues);
    const thresholdRawFinal = normalizedToRawScore(
      state.graphControls.minScore,
      scoreDomain,
    );
    return {
      nodes,
      links: filteredLinks,
      scoreValues,
      scoreDomain,
      thresholdRaw: thresholdRawFinal,
    };
  }

  function renderGraphEmpty(message) {
    if (els.graphSubtitle) {
      els.graphSubtitle.textContent = "No graph data";
    }
    renderScoreHistogram([], 0, null);
    els.graphRoot.innerHTML = `<div class="graph-empty">${message}</div>`;
  }

  function buildNodeHoverCardHtml(note, nodeRef, isPinned) {
    if (!note) {
      return `<div class="hover-title">#${nodeRef.id}</div><p class="hover-snippet">Missing note payload.</p>`;
    }
    const relatedCount = Array.isArray(note.related_note_links)
      ? note.related_note_links.length
      : 0;
    const sourceCommits = Array.isArray(note.source_commit_ids)
      ? note.source_commit_ids
          .filter((id) => typeof id === "string" && id.trim().length > 0)
          .map((id) => id.trim().slice(0, 8))
      : [];
    const sourceTimes = Array.isArray(note.source_timestamps)
      ? note.source_timestamps
          .filter((ts) => Number.isFinite(ts))
          .map((ts) => formatTimestamp(ts))
      : [];
    const kind = nodeRef.kind || "node";
    const commitText =
      sourceCommits.length > 0 ? summarizeList(sourceCommits, 3) : "none";
    const timeText =
      sourceTimes.length > 0 ? summarizeList(sourceTimes, 2) : "none";
    return `
      <div class="hover-title">#${note.note_id} <span class="hover-kind">${kind}</span></div>
      <p class="hover-snippet">${summarize(note.context || note.raw_content || "", 160)}</p>
      <div class="hover-meta">
        <span>related: ${relatedCount}</span>
        <span>commits: ${commitText}</span>
        <span>times: ${timeText}</span>
      </div>
    `;
  }

  function hideHoverCard(card) {
    card.classList.remove("visible");
  }

  function positionHoverCard(event, card, root) {
    const rootRect = root.getBoundingClientRect();
    const cardRect = card.getBoundingClientRect();
    const margin = 14;
    let x = event.clientX - rootRect.left + 12;
    let y = event.clientY - rootRect.top + 12;

    if (x + cardRect.width > rootRect.width - margin) {
      x = event.clientX - rootRect.left - cardRect.width - 12;
    }
    if (x < margin) {
      x = margin;
    }
    if (y + cardRect.height > rootRect.height - margin) {
      y = event.clientY - rootRect.top - cardRect.height - 12;
    }
    if (y < margin) {
      y = margin;
    }

    card.style.left = `${Math.round(x)}px`;
    card.style.top = `${Math.round(y)}px`;
  }

  function formatTimestamp(epochSeconds) {
    const value = Number(epochSeconds);
    if (!Number.isFinite(value) || value <= 0) {
      return "n/a";
    }
    const dt = new Date(value * 1000);
    if (Number.isNaN(dt.getTime())) {
      return "n/a";
    }
    return dt.toISOString().slice(0, 10);
  }

  function summarizeList(values, maxItems) {
    if (!Array.isArray(values) || values.length === 0) {
      return "none";
    }
    const shown = values.slice(0, maxItems);
    const extra = values.length - shown.length;
    return extra > 0 ? `${shown.join(", ")} +${extra}` : shown.join(", ");
  }

  function summarize(text, max) {
    const compact = String(text || "")
      .replace(/\s+/g, " ")
      .trim();
    if (compact.length <= max) {
      return compact;
    }
    return `${compact.slice(0, max - 1)}…`;
  }

  init();
})();
