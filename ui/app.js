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
    graphControls: { relatedLimit: 5, depth: 1 },
    validation: { errors: [], warnings: [] },
  };

  const LIMIT_LIST_RENDER = 300;
  const GRAPH_NODE_CAP = 120;

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
    syncGraphControls();
    renderGraph();
  }

  function syncGraphControls() {
    els.relatedLimitValue.textContent = String(
      state.graphControls.relatedLimit,
    );
    els.depthValue.textContent = String(state.graphControls.depth);
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
    const note = state.notesById.get(state.selectedNoteId);
    if (!note) {
      renderGraphEmpty("Select a note to render its neighborhood graph.");
      return;
    }
    if (!window.d3) {
      renderGraphEmpty("D3 failed to load from CDN.");
      return;
    }

    const data = buildNeighborhoodData(note.note_id);
    if (data.nodes.length === 0) {
      renderGraphEmpty("No graphable neighborhood available.");
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
    legend.innerHTML = `
      <div class="legend-title">Legend</div>
      <div class="legend-row"><span class="legend-swatch" style="--swatch:#ff71cf"></span>Selected node</div>
      <div class="legend-row"><span class="legend-swatch" style="--swatch:#1cffd8"></span>Outbound / related out</div>
      <div class="legend-row"><span class="legend-swatch" style="--swatch:#62a8ff"></span>Inbound / related in</div>
      <div class="legend-row"><span class="legend-swatch" style="--swatch:#b066ff"></span>Expanded node</div>
      <div class="legend-row"><span class="legend-swatch" style="--swatch:#ffb347"></span>Long-range node</div>
      <div class="legend-row"><span class="legend-swatch line" style="--swatch:#ff71cf"></span>Long-range link</div>
    `;
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
    const zoomBehavior = d3
      .zoom()
      .scaleExtent([0.35, 4])
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
      .attr("fill", (d) => nodeColor[d.kind] || "#6a8")
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
      .text((d) => `#${d.id}`);

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
      .force("charge", d3.forceManyBody().strength(-180))
      .force("center", d3.forceCenter(width / 2, height / 2))
      .force(
        "collision",
        d3.forceCollide().radius((d) => (d.kind === "selected" ? 14 : 10)),
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
    for (let i = 0; i < 80; i += 1) {
      simulation.tick();
    }
    ticked();
    zoomToFit(svg, zoomBehavior, data.nodes, width, height, 26);
    simulation.alpha(0.35).restart();

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

  function zoomToFit(svg, zoomBehavior, nodes, width, height, padding) {
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
      0.35,
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

  function buildNeighborhoodData(selectedId) {
    const nodes = [];
    const links = [];
    const nodeKinds = new Map([[selectedId, "selected"]]);
    const depth = Math.max(1, Math.min(3, state.graphControls.depth));
    const relatedLimit = Math.max(1, state.graphControls.relatedLimit);
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

      for (const [dstId, score] of outbound) {
        if (used >= budget) {
          break;
        }
        if (!state.notesById.has(dstId)) {
          continue;
        }
        used += 1;
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
      const longRange = state.longRangeById.get(selectedId) || [];
      const longRangeLimit = Math.min(relatedLimit, 50);
      for (const entry of longRange.slice(0, longRangeLimit)) {
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

    return { nodes, links: filteredLinks };
  }

  function renderGraphEmpty(message) {
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
        <span>${isPinned ? "pinned (right-click node to unpin)" : "right-click to pin"}</span>
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
