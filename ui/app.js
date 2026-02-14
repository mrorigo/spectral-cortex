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
    renderGraphEmpty("Load an SMG JSON file to visualize the graph.");
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
        state.reverseRelated.set(note.note_id, new Set());
        state.longRangeById.set(note.note_id, []);
      }
    }

    for (const note of state.graph.notes) {
      const srcId = note.note_id;
      const related = Array.isArray(note.related_note_ids)
        ? note.related_note_ids
        : [];
      for (const dstId of related) {
        if (!state.reverseRelated.has(dstId)) {
          state.validation.warnings.push(
            `note ${srcId} references missing related note ${dstId}`,
          );
          continue;
        }
        state.reverseRelated.get(dstId).add(srcId);
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
    els.relatedInput.value = (note.related_note_ids || []).join(", ");

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

    const parsedIds = parseIdList(els.relatedInput.value);
    const deduped = Array.from(new Set(parsedIds));
    const existing = deduped.filter((id) => state.notesById.has(id));
    const dropped = deduped.filter((id) => !state.notesById.has(id));

    note.context = els.contextInput.value;
    note.raw_content = els.rawInput.value;
    note.related_note_ids = existing;

    if (dropped.length > 0) {
      state.validation.warnings.push(
        `Dropped missing related ids for ${note.note_id}: ${dropped.join(", ")}`,
      );
    }

    state.dirty = true;
    buildIndexes();
    syncStatus();
    renderList();
    renderEditor();
    renderGraph();
  }

  function parseIdList(text) {
    return text
      .split(/[\s,]+/)
      .map((chunk) => chunk.trim())
      .filter(Boolean)
      .map((chunk) => Number.parseInt(chunk, 10))
      .filter((n) => Number.isInteger(n));
  }

  function deleteSelectedNote() {
    const id = state.selectedNoteId;
    if (!state.notesById.has(id)) {
      return;
    }

    const note = state.notesById.get(id);
    const outbound = Array.isArray(note.related_note_ids)
      ? note.related_note_ids.length
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
      if (!Array.isArray(item.related_note_ids)) {
        item.related_note_ids = [];
        continue;
      }
      item.related_note_ids = item.related_note_ids.filter(
        (relatedId) => relatedId !== id,
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

    const svg = d3
      .select(els.graphRoot)
      .append("svg")
      .attr("width", width)
      .attr("height", height)
      .attr("viewBox", `0 0 ${width} ${height}`);

    const scene = svg.append("g");
    svg.call(
      d3
        .zoom()
        .scaleExtent([0.35, 4])
        .on("zoom", (event) => {
          scene.attr("transform", event.transform);
        }),
    );

    const linkColor = {
      related_out: "#0d7f70",
      related_in: "#1d5f9d",
      long_range: "#ef6c4d",
    };
    const nodeColor = {
      selected: "#ef6c4d",
      outbound: "#0d7f70",
      inbound: "#1d5f9d",
      long_range: "#d7892f",
    };

    const link = scene
      .append("g")
      .attr("stroke-opacity", 0.56)
      .selectAll("line")
      .data(data.links)
      .join("line")
      .attr("stroke-width", (d) => (d.kind === "long_range" ? 2 : 1.3))
      .attr("stroke", (d) => linkColor[d.kind] || "#8aa");

    const node = scene
      .append("g")
      .selectAll("circle")
      .data(data.nodes)
      .join("circle")
      .attr("r", (d) => (d.kind === "selected" ? 11 : 7.5))
      .attr("fill", (d) => nodeColor[d.kind] || "#6a8")
      .attr("stroke", "#fff")
      .attr("stroke-width", 1.2)
      .style("cursor", "pointer")
      .on("click", (_, d) => {
        state.selectedNoteId = d.id;
        renderList();
        renderEditor();
        renderGraph();
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
          .distance((d) => (d.kind === "long_range" ? 120 : 70)),
      )
      .force("charge", d3.forceManyBody().strength(-180))
      .force("center", d3.forceCenter(width / 2, height / 2))
      .force(
        "collision",
        d3.forceCollide().radius((d) => (d.kind === "selected" ? 14 : 10)),
      )
      .on("tick", () => {
        link
          .attr("x1", (d) => d.source.x)
          .attr("y1", (d) => d.source.y)
          .attr("x2", (d) => d.target.x)
          .attr("y2", (d) => d.target.y);
        node.attr("cx", (d) => d.x).attr("cy", (d) => d.y);
        label.attr("x", (d) => d.x).attr("y", (d) => d.y);
      });

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
  }

  function buildNeighborhoodData(selectedId) {
    const nodes = [];
    const links = [];
    const nodeKinds = new Map([[selectedId, "selected"]]);

    const pushNode = (id, kind) => {
      if (!state.notesById.has(id)) {
        return;
      }
      if (!nodeKinds.has(id)) {
        nodeKinds.set(id, kind);
        return;
      }
      if (nodeKinds.get(id) === "long_range" && kind !== "long_range") {
        nodeKinds.set(id, kind);
      }
    };

    const selectedNote = state.notesById.get(selectedId);
    const outbound = Array.isArray(selectedNote?.related_note_ids)
      ? selectedNote.related_note_ids
      : [];
    const inbound = Array.from(state.reverseRelated.get(selectedId) || []);

    for (const id of outbound) {
      pushNode(id, "outbound");
      links.push({ source: selectedId, target: id, kind: "related_out" });
    }

    for (const id of inbound) {
      pushNode(id, "inbound");
      links.push({ source: id, target: selectedId, kind: "related_in" });
    }

    if (els.showLongRange.checked) {
      const longRange = state.longRangeById.get(selectedId) || [];
      for (const entry of longRange.slice(0, 50)) {
        pushNode(entry.other, "long_range");
        links.push({
          source: selectedId,
          target: entry.other,
          kind: "long_range",
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
