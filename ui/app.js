const COLORS = ["#71e0d0", "#c7ed79", "#ff9b67", "#76b8dc", "#d5a7e8", "#e7d47c", "#75c79f", "#d78c88"];
const state = {
  data: null,
  notes: [],
  noteById: new Map(),
  identityRepresentative: new Map(),
  topologySnapshots: [],
  topologyCommitId: "",
  edges: [],
  filtered: [],
  cluster: null,
  edgeType: "all",
  graphMode: "field",
  force: { worker: null, session: 0, paused: false, status: "ready", positions: new Map(), draggingId: null },
  neighborhoodDepth: 2,
  neighborhoodBudget: 8,
  minStrength: 0,
  strengthRange: { min: 0, max: 1 },
  spectralStrength: 0,
  spectralRange: { min: 0, max: 1 },
  minStructuralDegree: 1,
  structuralDegreeById: new Map(),
  browseMode: "events",
  timeStart: 0,
  timeEnd: Number.POSITIVE_INFINITY,
  query: "",
  selectedId: null,
  view: "atlas",
  newestFirst: true,
  labels: true,
  semanticDerived: false,
  graph: { nodes: [], transform: { x: 0, y: 0, scale: 1 }, drag: null },
  showHistory: false,
};

const el = Object.fromEntries([...document.querySelectorAll("[id]")].map((node) => [node.id, node]));
const canvas = el.graphCanvas;
const ctx = canvas.getContext("2d");

const escapeHtml = (value = "") => String(value).replace(/[&<>'"]/g, (char) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "'": "&#39;", '"': "&quot;" })[char]);
const safeHref = (value) => /^(https?:|mailto:)/i.test(value) ? value : "";
function renderMarkdown(value = "") {
  const lines = String(value).replace(/\r/g, "").split("\n");
  const inline = (text) => escapeHtml(text)
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/\*([^*]+)\*/g, "<em>$1</em>")
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, (_, label, href) => {
      const safe = safeHref(href.replace(/&amp;/g, "&"));
      return safe ? `<a href="${escapeHtml(safe)}" target="_blank" rel="noreferrer">${label}</a>` : label;
    });
  const output = [];
  let list = null;
  let fenced = false;
  let code = [];
  const closeList = () => { if (list) output.push(`</${list}>`); list = null; };
  for (const line of lines) {
    if (line.startsWith("```")) {
      if (fenced) { output.push(`<pre><code>${escapeHtml(code.join("\n"))}</code></pre>`); code = []; }
      fenced = !fenced;
      continue;
    }
    if (fenced) { code.push(line); continue; }
    const heading = line.match(/^(#{1,3})\s+(.+)$/);
    const bullet = line.match(/^\s*([-*]|\d+\.)\s+(.+)$/);
    if (heading) { closeList(); output.push(`<h${heading[1].length}>${inline(heading[2])}</h${heading[1].length}>`); continue; }
    if (bullet) {
      const type = /\d+\./.test(bullet[1]) ? "ol" : "ul";
      if (list && list !== type) closeList();
      if (!list) { output.push(`<${type}>`); list = type; }
      output.push(`<li>${inline(bullet[2])}</li>`);
      continue;
    }
    closeList();
    if (!line.trim()) continue;
    if (line.startsWith("> ")) output.push(`<blockquote>${inline(line.slice(2))}</blockquote>`);
    else output.push(`<p>${inline(line)}</p>`);
  }
  if (fenced) output.push(`<pre><code>${escapeHtml(code.join("\n"))}</code></pre>`);
  closeList();
  return output.join("");
}
const formatNumber = (value) => new Intl.NumberFormat("en").format(value ?? 0);
const timestampOf = (note) => Math.max(0, ...(note.source_timestamps ?? [0]));
const formatDate = (timestamp, style = "medium") => timestamp ? new Intl.DateTimeFormat("en", style === "short" ? { month: "short", day: "numeric" } : { year: "numeric", month: "short", day: "numeric" }).format(new Date(timestamp * 1000)) : "Unknown";
const basename = (path) => path?.split("/").pop() ?? "No file";
const clusterOf = (note) => Number(state.data?.cluster_labels?.[note.__index] ?? 0);
const colorOf = (note) => COLORS[clusterOf(note) % COLORS.length];
const hash = (value) => [...String(value)].reduce((acc, char) => Math.imul(acc ^ char.charCodeAt(0), 16777619), 2166136261) >>> 0;

function validateGraph(data) {
  if (!data || !Array.isArray(data.notes)) throw new Error("This file has no SMG notes array.");
  if (!data.notes.every((note) => Number.isInteger(note.note_id) && typeof note.raw_content === "string")) {
    throw new Error("One or more notes do not match the SMG v1 shape.");
  }
}

async function normalizeGraph(data) {
  stopForceLayout();
  validateGraph(data);
  state.data = data;
  state.notes = data.notes.map((note, index) => ({ ...note, __index: index }));
  state.noteById = new Map(state.notes.map((note) => [note.note_id, note]));
  state.identityRepresentative = new Map();
  state.topologySnapshots = data.topology_snapshots ?? [];
  const snapshots = [...state.topologySnapshots].sort((a, b) => Number(a.timestamp ?? 0) - Number(b.timestamp ?? 0));
  if (!state.topologyCommitId || !snapshots.some((snapshot) => snapshot.commit_id === state.topologyCommitId)) {
    state.topologyCommitId = snapshots.at(-1)?.commit_id ?? "";
  }
  if (el.topologySnapshot) {
    el.topologySnapshot.replaceChildren(new Option("Latest topology snapshot", ""), ...snapshots.map((snapshot) => new Option(`${formatDate(Number(snapshot.timestamp ?? 0))} · ${String(snapshot.commit_id).slice(0, 12)}`, snapshot.commit_id)));
    el.topologySnapshot.value = state.topologyCommitId;
    el.topologySnapshot.disabled = snapshots.length === 0;
  }
  const latestByIdentity = new Map();
  for (const note of state.notes) {
    if (!note.symbol_id) continue;
    const key = `${note.symbol_id}\0${note.file_path ?? ""}\0${note.ast_node_type ?? ""}`;
    const current = latestByIdentity.get(key);
    if (!current || timestampOf(note) > timestampOf(current) || (timestampOf(note) === timestampOf(current) && note.note_id > current.note_id)) latestByIdentity.set(key, note);
  }
  for (const note of state.notes) {
    const key = note.symbol_id ? `${note.symbol_id}\0${note.file_path ?? ""}\0${note.ast_node_type ?? ""}` : "";
    state.identityRepresentative.set(note.note_id, latestByIdentity.get(key)?.note_id ?? note.note_id);
  }
  configureTimeFilter();
  const edgeMap = new Map();
  const addEdge = (source, target, type, weight = 1, longRange = false) => {
    if (source === target || !state.noteById.has(source) || !state.noteById.has(target)) return;
    const [a, b] = source < target ? [source, target] : [target, source];
    const key = `${a}:${b}:${type}`;
    const existing = edgeMap.get(key);
    if (existing) {
      existing.longRange = existing.longRange || longRange;
      if (existing.weight < weight) existing.weight = weight;
    } else {
      edgeMap.set(key, { source: a, target: b, type, weight, longRange });
    }
  };
  for (const note of state.notes) {
    for (const link of note.related_note_links ?? []) addEdge(note.note_id, Number(link[0]), "spectral", Number(link[1] ?? 1));
    for (const target of note.history_links ?? []) addEdge(note.note_id, Number(target), "history", 1);
  }
  for (const link of data.long_range_links ?? []) addEdge(Number(link[0]), Number(link[1]), "spectral", Number(link[2] ?? 1), true);
  const latestSnapshot = state.topologySnapshots.find((snapshot) => snapshot.commit_id === state.topologyCommitId)
    ?? [...state.topologySnapshots].sort((a, b) => Number(b.timestamp ?? 0) - Number(a.timestamp ?? 0))[0];
  const topologyNotes = new Map();
  for (const symbol of latestSnapshot?.symbols ?? []) {
    const note = state.notes.find((candidate) => candidate.symbol_id === symbol.symbol_id && candidate.file_path === symbol.file_path && candidate.ast_node_type === symbol.ast_node_type);
    if (note) topologyNotes.set(symbol.symbol_id, state.identityRepresentative.get(note.note_id) ?? note.note_id);
  }
  for (const link of latestSnapshot?.edges ?? []) {
    const source = topologyNotes.get(link.source_symbol_id);
    const target = topologyNotes.get(link.target_symbol_id);
    if (source !== undefined && target !== undefined) addEdge(source, target, `topology:${String(link.relation).toLowerCase()}`, 1);
  }
  state.semanticDerived = false;
  if (![...edgeMap.values()].some((edge) => edge.type === "spectral")) {
    showDerivationState();
  }
  // Keep embedding cosine links independent from persisted spectral links.
  // The two measures answer different questions and must be inspectable together.
  for (const link of await deriveSemanticLinks(state.notes)) addEdge(link.source, link.target, "embedding", link.weight);
  state.semanticDerived = true;
  state.edges = [...edgeMap.values()];
  configureStrengthThreshold();
  configureSpectralThreshold();
  configureStructuralThreshold();
  state.cluster = null;
  state.browseMode = "events";
  el.browseModes.querySelectorAll("button").forEach((button) => {
    const active = button.dataset.browse === state.browseMode;
    button.classList.toggle("is-active", active);
    button.setAttribute("aria-selected", String(active));
  });
  el.listLabel.textContent = "Events";
  state.query = "";
  state.selectedId = null;
  el.searchInput.value = "";
  buildGraphLayout();
  applyFilters();
  renderMetrics();
  renderClusters();
  renderInspector();
  el.stageMessage.hidden = true;
}

function showDerivationState() {
  el.stageMessage.hidden = false;
  el.stageMessage.querySelector("h2").textContent = "Mapping semantic neighborhoods";
  el.stageMessage.querySelector("p").textContent = "No persisted spectral links found; deriving local cosine similarity from embeddings.";
  el.stageMessage.querySelector(".loader-mark").style.animation = "";
}

function deriveSemanticLinks(notes) {
  const vectors = notes.map((note) => note.embedding);
  const dimensions = vectors[0]?.length ?? 0;
  if (!dimensions || !vectors.every((vector) => Array.isArray(vector) && vector.length === dimensions)) return Promise.resolve([]);
  const source = `self.onmessage = ({ data }) => {
    const { vectors, norms, noteIds, maxPerNode } = data;
    const best = Array.from({ length: vectors.length }, () => []);
    const offer = (index, candidate) => {
      const list = best[index];
      if (list.length < maxPerNode) { list.push(candidate); return; }
      let weakest = 0;
      for (let cursor = 1; cursor < list.length; cursor += 1) if (list[cursor].score < list[weakest].score) weakest = cursor;
      if (candidate.score > list[weakest].score) list[weakest] = candidate;
    };
    for (let left = 0; left < vectors.length; left += 1) {
      for (let right = left + 1; right < vectors.length; right += 1) {
        let dot = 0;
        for (let dimension = 0; dimension < vectors[left].length; dimension += 1) dot += vectors[left][dimension] * vectors[right][dimension];
        const score = dot / ((norms[left] || 1) * (norms[right] || 1));
        if (score >= 0.9995) continue;
        offer(left, { index: right, score });
        offer(right, { index: left, score });
      }
    }
    const unique = new Map();
    best.forEach((links, sourceIndex) => links.forEach(({ index, score }) => {
      const [left, right] = sourceIndex < index ? [sourceIndex, index] : [index, sourceIndex];
      const key = left + ":" + right;
      const existing = unique.get(key);
      if (existing === undefined || score > existing) unique.set(key, score);
    }));
    self.postMessage([...unique.entries()].map(([key, weight]) => {
      const [left, right] = key.split(":").map(Number);
      return { source: noteIds[left], target: noteIds[right], weight };
    }));
  };`;
  if (typeof Worker === "undefined") return Promise.resolve([]);
  try {
    return new Promise((resolve) => {
      const workerUrl = URL.createObjectURL(new Blob([source], { type: "text/javascript" }));
      const worker = new Worker(workerUrl);
      worker.addEventListener("message", ({ data }) => {
        worker.terminate();
        URL.revokeObjectURL(workerUrl);
        resolve(data);
      }, { once: true });
      worker.addEventListener("error", () => {
        worker.terminate();
        URL.revokeObjectURL(workerUrl);
        resolve([]);
      }, { once: true });
      worker.postMessage({
        vectors,
        norms: notes.map((note) => Number(note.norm ?? 1)),
        noteIds: notes.map((note) => note.note_id),
        maxPerNode: 5,
      });
    });
  } catch {
    return Promise.resolve([]);
  }
}

function buildGraphLayout() {
  const clusters = [...new Set(state.notes.map(clusterOf))].sort((a, b) => a - b);
  const times = state.notes.map(timestampOf).filter(Boolean);
  const minTime = Math.min(...times);
  const maxTime = Math.max(...times);
  const degreeById = new Map();
  for (const edge of state.edges) {
    degreeById.set(edge.source, (degreeById.get(edge.source) ?? 0) + 1);
    degreeById.set(edge.target, (degreeById.get(edge.target) ?? 0) + 1);
  }
  state.graph.nodes = state.notes.map((note) => {
    const cluster = clusterOf(note);
    const ci = Math.max(0, clusters.indexOf(cluster));
    const angle = (ci / Math.max(1, clusters.length)) * Math.PI * 2 - Math.PI / 2;
    const seed = hash(note.note_id);
    const radius = 130 + (seed % 210);
    const timeBias = maxTime > minTime ? ((timestampOf(note) - minTime) / (maxTime - minTime) - .5) * 210 : 0;
    return {
      id: note.note_id,
      note,
      x: Math.cos(angle) * 330 + Math.cos((seed % 628) / 100) * radius + timeBias * .25,
      y: Math.sin(angle) * 240 + Math.sin((seed % 628) / 100) * radius * .68 - timeBias * .18,
      degree: degreeById.get(note.note_id) ?? 0,
    };
  });
  [...state.graph.nodes]
    .sort((left, right) => right.degree - left.degree)
    .forEach((node, index) => { node.labelRank = index; });
  fitGraph();
}

function forceEdges() {
  const projection = projectGraph();
  const visible = projection.visible;
  const ranges = new Map([["embedding", state.strengthRange], ["spectral", state.spectralRange]]);
  const edges = new Map();
  for (const edge of projection.edges) {
    if (!visible.has(edge.source) || !visible.has(edge.target)) continue;
    if (state.edgeType !== "all" && edge.type !== state.edgeType) continue;
    if (edge.type === "embedding" && edge.weight < state.minStrength) continue;
    if (edge.type === "spectral" && edge.weight < state.spectralStrength) continue;
    if (edge.type === "history" && ((state.structuralDegreeById.get(edge.source) ?? 0) < state.minStructuralDegree || (state.structuralDegreeById.get(edge.target) ?? 0) < state.minStructuralDegree)) continue;
    const range = ranges.get(edge.type);
    const normalized = range && range.max > range.min ? Math.max(0, Math.min(1, (edge.weight - range.min) / (range.max - range.min))) : edge.type === "history" ? .8 : edge.type.startsWith("topology:") ? .9 : .5;
    const [source, target] = edge.source < edge.target ? [edge.source, edge.target] : [edge.target, edge.source];
    const key = `${source}:${target}:${edge.type}`;
    if (!edges.has(key) || edges.get(key).normalized_strength < normalized) edges.set(key, { source, target, normalized_strength: normalized });
  }
  return { nodes: [...visible].map((id) => state.graph.nodes.find((node) => node.id === id)).filter(Boolean), edges: [...edges.values()] };
}

function stopForceLayout() {
  state.force.session += 1;
  state.force.worker?.terminate();
  state.force.worker = null;
  state.force.inline = null;
}

function updateForceDebug({ phase, nodes = 0, edges = 0, tick = 0, displacement = 0, error = "" }) {
  if (!el.forceDebug) return;
  const canvasSize = `${Math.round(canvas.clientWidth)}×${Math.round(canvas.clientHeight)}`;
  el.forceDebug.textContent = error
    ? `Force error: ${error}`
    : `Force ${phase} · s${state.force.session} · ${nodes}n/${edges}e · t${tick} · Δ${displacement.toFixed(2)} · ${canvasSize}`;
}

function startForceLayout() {
  if (state.graphMode !== "force" || !state.graph.nodes.length) return;
  stopForceLayout();
  state.force.paused = false;
  if (el.toggleForce) el.toggleForce.textContent = "Pause";
  const session = state.force.session;
  // Keep the first implementation on the rendering thread. This works when
  // the UI is opened directly or served, and makes motion observable before
  // introducing worker transport as a measured optimization.
  startInlineForceLayout(session);
}

function startInlineForceLayout(session) {
  const graph = forceEdges();
  const nodes = graph.nodes;
  const indexById = new Map(nodes.map((node, index) => [node.id, index]));
  const velocity = nodes.map((node, index) => {
    const angle = (index / Math.max(1, nodes.length)) * Math.PI * 2 + (hash(node.id) % 100) / 100;
    return { x: Math.cos(angle) * 5, y: Math.sin(angle) * 5 };
  });
  let tick = 0;
  let stableTicks = 0;
  let alpha = 1;
  let paused = false;
  const schedule = () => window.setTimeout(step, 16);
  state.force.status = "layouting";
  if (el.forceStatus) el.forceStatus.textContent = "Layouting";
  updateForceDebug({ phase: "started", nodes: nodes.length, edges: graph.edges.length });
  const step = () => {
    if (session !== state.force.session || state.graphMode !== "force") return;
    try {
      if (!paused) {
      const forces = nodes.map(() => ({ x: 0, y: 0 }));
      for (let left = 0; left < nodes.length; left += 1) for (let right = left + 1; right < nodes.length; right += 1) {
        const a = nodes[left], b = nodes[right], dx = b.x - a.x, dy = b.y - a.y, distance = Math.max(12, Math.hypot(dx, dy)), force = Math.min(7, 1_200 / (distance * distance));
        forces[left].x -= dx / distance * force; forces[left].y -= dy / distance * force; forces[right].x += dx / distance * force; forces[right].y += dy / distance * force;
      }
      for (const edge of graph.edges) {
        const ai = indexById.get(edge.source), bi = indexById.get(edge.target);
        if (ai === undefined || bi === undefined) continue;
        const a = nodes[ai], b = nodes[bi];
        const dx = b.x - a.x, dy = b.y - a.y, distance = Math.max(12, Math.hypot(dx, dy)), force = Math.max(-5, Math.min(5, (distance - 80) * (.008 + edge.normalized_strength * .032)));
        forces[ai].x += dx / distance * force; forces[ai].y += dy / distance * force; forces[bi].x -= dx / distance * force; forces[bi].y -= dy / distance * force;
      }
      let displacement = 0;
      nodes.forEach((node, index) => {
        forces[index].x -= node.x * .003;
        forces[index].y -= node.y * .003;
        velocity[index].x = (velocity[index].x + forces[index].x * .16 * alpha) * .78;
        velocity[index].y = (velocity[index].y + forces[index].y * .16 * alpha) * .78;
        const maxSpeed = .35 + 10 * alpha;
        const speed = Math.hypot(velocity[index].x, velocity[index].y);
        if (speed > maxSpeed) { velocity[index].x *= maxSpeed / speed; velocity[index].y *= maxSpeed / speed; }
        node.x += velocity[index].x;
        node.y += velocity[index].y;
        displacement = Math.max(displacement, Math.hypot(velocity[index].x, velocity[index].y));
      });
      tick += 1;
      alpha *= .975;
      drawGraph();
      stableTicks = displacement < .1 ? stableTicks + 1 : 0;
      const done = (alpha < .01 && stableTicks >= 8) || tick >= 1_200;
        if (el.forceStatus) el.forceStatus.textContent = done ? (tick >= 1_200 ? "Iteration cap reached" : "Converged") : `Layouting · ${tick} · ${nodes.length} nodes`;
        updateForceDebug({ phase: `${done ? "done" : "running"} α${alpha.toFixed(3)}`, nodes: nodes.length, edges: graph.edges.length, tick, displacement });
        if (!done) schedule();
      } else {
        updateForceDebug({ phase: "paused", nodes: nodes.length, edges: graph.edges.length, tick });
      }
    } catch (error) {
      state.force.status = "error";
      if (el.forceStatus) el.forceStatus.textContent = "Error";
      updateForceDebug({ phase: "error", nodes: nodes.length, edges: graph.edges.length, tick, error: error instanceof Error ? error.message : String(error) });
    }
  };
  state.force.inline = { pause: () => { paused = true; }, resume: () => { if (paused) { paused = false; schedule(); } } };
  step();
}

function configureStrengthThreshold() {
  const weights = state.edges.filter((edge) => edge.type === "embedding").map((edge) => edge.weight).sort((left, right) => left - right);
  const hasWeights = weights.length > 0;
  const semanticButton = el.edgeFilter.querySelector('[data-edge="embedding"]');
  const min = hasWeights ? Math.floor(Math.min(...weights) * 100) / 100 : 0;
  const max = hasWeights ? Math.ceil(Math.max(...weights) * 100) / 100 : 1;
  const initial = state.semanticDerived && hasWeights ? weights[Math.floor((weights.length - 1) * .75)] : min;
  if (!hasWeights && state.edgeType === "embedding") state.edgeType = "all";
  state.minStrength = initial;
  state.strengthRange = { min, max };
  el.strengthThreshold.min = "0";
  el.strengthThreshold.max = "1000";
  el.strengthThreshold.step = "1";
  el.strengthThreshold.value = String(strengthToSlider(initial));
  semanticButton.disabled = !hasWeights;
  el.edgeFilter.querySelectorAll("button").forEach((button) => button.classList.toggle("is-active", button.dataset.edge === state.edgeType));
  el.thresholdValue.textContent = hasWeights ? `≥ ${initial.toFixed(2)}` : "No embedding links";
  updateSemanticThresholdInteractivity(hasWeights);
}

function sliderToStrength(value) {
  const progress = Number(value) / 1000;
  const { min, max } = state.strengthRange;
  return min + (max - min) * (1 - (1 - progress) ** 2);
}

function strengthToSlider(value) {
  const { min, max } = state.strengthRange;
  if (max <= min) return 0;
  const progress = Math.max(0, Math.min(1, (value - min) / (max - min)));
  return Math.round((1 - Math.sqrt(1 - progress)) * 1000);
}

function updateSemanticThresholdInteractivity(hasSemantic = state.edges.some((edge) => edge.type === "embedding")) {
  const disabled = !hasSemantic || state.edgeType === "history" || state.edgeType === "spectral" || state.edgeType.startsWith("topology:");
  el.strengthThreshold.disabled = disabled;
  el.thresholdValue.style.opacity = disabled ? ".45" : "1";
}

function configureSpectralThreshold() {
  const weights = state.edges.filter((edge) => edge.type === "spectral").map((edge) => edge.weight).sort((a, b) => a - b);
  const hasWeights = weights.length > 0;
  const min = hasWeights ? Math.floor(Math.min(...weights) * 100) / 100 : 0;
  const max = hasWeights ? Math.ceil(Math.max(...weights) * 100) / 100 : 1;
  state.spectralRange = { min, max };
  state.spectralStrength = hasWeights ? min : 0;
  el.spectralThreshold.min = "0";
  el.spectralThreshold.max = "1000";
  el.spectralThreshold.value = String(strengthToSliderFor(state.spectralStrength, state.spectralRange));
  el.spectralThresholdValue.textContent = hasWeights ? `≥ ${state.spectralStrength.toFixed(2)}` : "No spectral links";
  el.spectralThreshold.disabled = !hasWeights || state.edgeType === "embedding" || state.edgeType === "history" || state.edgeType.startsWith("topology:");
  el.spectralThresholdValue.style.opacity = el.spectralThreshold.disabled ? ".45" : "1";
}

function sliderToSpectral(value) {
  const progress = Number(value) / 1000;
  const { min, max } = state.spectralRange;
  return min + (max - min) * (1 - (1 - progress) ** 2);
}

function strengthToSliderFor(value, { min, max }) {
  if (max <= min) return 0;
  const progress = Math.max(0, Math.min(1, (value - min) / (max - min)));
  return Math.round((1 - Math.sqrt(1 - progress)) * 1000);
}

function configureStructuralThreshold() {
  state.structuralDegreeById = new Map(state.notes.map((note) => [note.note_id, 0]));
  const structuralEdges = (state.showHistory ? state.edges : projectedEdges()).filter((edge) => edge.type === "history");
  for (const edge of structuralEdges) {
    state.structuralDegreeById.set(edge.source, (state.structuralDegreeById.get(edge.source) ?? 0) + 1);
    state.structuralDegreeById.set(edge.target, (state.structuralDegreeById.get(edge.target) ?? 0) + 1);
  }
  const max = Math.max(0, ...state.structuralDegreeById.values());
  const hasStructuralLinks = max > 0;
  const persistedStructuralLinks = state.edges.some((edge) => edge.type === "history");
  state.minStructuralDegree = 1;
  el.structuralThreshold.min = "1";
  el.structuralThreshold.max = String(Math.max(1, max));
  el.structuralThreshold.value = "1";
  el.structuralThresholdValue.textContent = hasStructuralLinks
    ? "≥ 1"
    : persistedStructuralLinks && !state.showHistory ? "History links only — show history" : "No history links";
  updateStructuralThresholdInteractivity(hasStructuralLinks);
}

function updateStructuralThresholdInteractivity(hasStructural = state.edges.some((edge) => edge.type === "history")) {
  const disabled = !hasStructural || state.edgeType === "embedding" || state.edgeType === "spectral" || state.edgeType.startsWith("topology:");
  el.structuralThreshold.disabled = disabled;
  el.structuralThresholdValue.style.opacity = disabled ? ".45" : "1";
}

function dateInputValue(timestamp) {
  if (!timestamp) return "";
  const date = new Date(timestamp * 1000);
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}

function configureTimeFilter() {
  const times = state.notes.map(timestampOf).filter(Boolean);
  const min = Math.min(...times);
  const max = Math.max(...times);
  if (!times.length) {
    state.timeStart = 0;
    state.timeEnd = Number.POSITIVE_INFINITY;
    el.timeStart.disabled = true;
    el.timeEnd.disabled = true;
    el.timeWindowValue.textContent = "No timestamps";
    return;
  }
  const minValue = dateInputValue(min);
  const maxValue = dateInputValue(max);
  el.timeStart.disabled = false;
  el.timeEnd.disabled = false;
  el.timeStart.min = minValue;
  el.timeStart.max = maxValue;
  el.timeEnd.min = minValue;
  el.timeEnd.max = maxValue;
  el.timeStart.value = "";
  el.timeEnd.value = "";
  state.timeStart = 0;
  state.timeEnd = Number.POSITIVE_INFINITY;
  el.timeWindowValue.textContent = "All history";
  el.timePresets.querySelectorAll("button").forEach((button) => button.classList.toggle("is-active", button.dataset.days === "0"));
}

function applyTimeWindow(startValue, endValue, label) {
  state.timeStart = startValue ? new Date(`${startValue}T00:00:00`).getTime() / 1000 : 0;
  state.timeEnd = endValue ? new Date(`${endValue}T23:59:59`).getTime() / 1000 : Number.POSITIVE_INFINITY;
  el.timeWindowValue.textContent = label;
  applyFilters();
}

function applyFilters() {
  const terms = state.query.toLocaleLowerCase().trim().split(/\s+/).filter(Boolean);
  const latestSymbols = new Map();
  for (const note of state.notes) {
    if (!note.symbol_id) continue;
    const key = `${note.symbol_id}\0${note.file_path ?? ""}\0${note.ast_node_type ?? ""}`;
    const current = latestSymbols.get(key);
    if (!current || timestampOf(note) > timestampOf(current) || (timestampOf(note) === timestampOf(current) && note.note_id > current.note_id)) latestSymbols.set(key, note);
  }
  state.filtered = state.notes.filter((note) => {
    if (!state.showHistory && note.symbol_id) {
      const key = `${note.symbol_id}\0${note.file_path ?? ""}\0${note.ast_node_type ?? ""}`;
      if (latestSymbols.get(key) !== note) return false;
    }
    if (state.cluster !== null && clusterOf(note) !== state.cluster) return false;
    const timestamp = timestampOf(note);
    if (timestamp && (timestamp < state.timeStart || timestamp > state.timeEnd)) return false;
    if (!timestamp && (state.timeStart > 0 || Number.isFinite(state.timeEnd))) return false;
    if (!terms.length) return true;
    const haystack = [note.raw_content, note.symbol_id, note.file_path, note.ast_node_type, ...(note.source_commit_ids ?? [])].filter(Boolean).join(" ").toLocaleLowerCase();
    return terms.every((term) => haystack.includes(term));
  });
  renderNoteList();
  if (state.view === "chronicle") renderTimeline();
  if (state.graphMode === "force") startForceLayout(); else drawGraph();
}

function renderMetrics() {
  const symbols = new Set(state.notes.map((note) => note.symbol_id).filter(Boolean));
  const structural = state.edges.filter((edge) => edge.type === "history").length;
  const times = state.notes.map(timestampOf).filter(Boolean).sort((a, b) => a - b);
  el.metricNodes.textContent = formatNumber(state.notes.length);
  el.metricSymbols.textContent = formatNumber(symbols.size);
  el.metricStructural.textContent = formatNumber(structural);
  el.metricLongRange.textContent = formatNumber(state.data.long_range_links?.length ?? 0);
  el.metricRange.textContent = times.length ? `${formatDate(times[0], "short")} ${new Date(times[0] * 1000).getFullYear()} — ${formatDate(times.at(-1), "short")} ${new Date(times.at(-1) * 1000).getFullYear()}` : "No timestamps";
}

function renderClusters() {
  const counts = new Map();
  for (const note of state.notes) counts.set(clusterOf(note), (counts.get(clusterOf(note)) ?? 0) + 1);
  el.clusterList.innerHTML = [...counts.entries()].sort(([a], [b]) => a - b).map(([cluster, count]) => `
    <button class="cluster-button${state.cluster === cluster ? " is-active" : ""}" style="--cluster-color:${COLORS[cluster % COLORS.length]}" data-cluster="${cluster}" type="button">
      <i></i><span>Cluster ${String(cluster + 1).padStart(2, "0")}</span><b>${count}</b>
    </button>`).join("");
}

function renderNoteList() {
  const notes = [...state.filtered].sort((a, b) => (timestampOf(b) - timestampOf(a)) * (state.newestFirst ? 1 : -1));
  let items;
  if (state.browseMode === "symbols") items = symbolItems(notes);
  else if (state.browseMode === "hotspots") items = hotspotItems(notes);
  else items = eventItems(notes);
  const visible = items.slice(0, 180);
  el.resultCount.textContent = formatNumber(items.length);
  el.noteList.innerHTML = visible.length ? visible.map((item) => `
    <button class="note-row${state.selectedId === item.note.note_id ? " is-active" : ""}" style="--cluster-color:${colorOf(item.note)}" data-note-id="${item.note.note_id}" type="button">
      <div class="note-row-content">
        <h3>${escapeHtml(item.title)}</h3>
        <span class="note-meta"><time>${escapeHtml(item.meta)}</time><span>${escapeHtml(item.detail)}</span></span>
      </div>
    </button>`).join("") : `<div class="list-empty">No ${state.browseMode} match this combination.<br>Clear a filter or try a broader term.</div>`;
}

function eventItems(notes) {
  const groups = new Map();
  for (const note of notes) {
    const commit = note.source_commit_ids?.find(Boolean);
    const key = commit ?? `${timestampOf(note)}:${note.raw_content}`;
    if (!groups.has(key)) groups.set(key, { note, title: note.raw_content, meta: formatDate(timestampOf(note), "short"), detail: `${groups.size + 1} event`, count: 0 });
    groups.get(key).count += 1;
  }
  return [...groups.values()].map((item) => ({ ...item, detail: `${item.count} ${item.count === 1 ? "note" : "notes"}` })).sort((a, b) => timestampOf(b.note) - timestampOf(a.note));
}

function symbolItems(notes) {
  const groups = new Map();
  for (const note of notes) {
    const key = note.symbol_id ?? note.file_path ?? `note:${note.note_id}`;
    if (!groups.has(key)) groups.set(key, { note, title: key, meta: formatDate(timestampOf(note), "short"), detail: "", count: 0, degree: state.structuralDegreeById.get(note.note_id) ?? 0 });
    const item = groups.get(key);
    item.count += 1;
    item.degree = Math.max(item.degree, state.structuralDegreeById.get(note.note_id) ?? 0);
    if (timestampOf(note) > timestampOf(item.note)) item.note = note;
  }
  return [...groups.values()].map((item) => ({ ...item, detail: `${item.count} ${item.count === 1 ? "event" : "events"} · ${basename(item.note.file_path)}` })).sort((a, b) => timestampOf(b.note) - timestampOf(a.note));
}

function hotspotItems(notes) {
  return symbolItems(notes).sort((a, b) => {
    const aScore = (state.structuralDegreeById.get(a.note.note_id) ?? 0) * 10 + a.count;
    const bScore = (state.structuralDegreeById.get(b.note.note_id) ?? 0) * 10 + b.count;
    return bScore - aScore;
  }).map((item) => ({ ...item, meta: `degree ${state.structuralDegreeById.get(item.note.note_id) ?? 0}`, detail: `${item.count} ${item.count === 1 ? "change" : "changes"}` }));
}

function renderTimeline() {
  const groups = new Map();
  const sorted = [...state.filtered].sort((a, b) => timestampOf(b) - timestampOf(a)).slice(0, 300);
  for (const note of sorted) {
    const date = new Date(timestampOf(note) * 1000);
    const key = timestampOf(note) ? `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}` : "unknown";
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(note);
  }
  el.timeline.innerHTML = [...groups.entries()].map(([key, notes]) => {
    const label = key === "unknown" ? "Unknown" : new Intl.DateTimeFormat("en", { month: "short", year: "numeric" }).format(new Date(`${key}-01T00:00:00`));
    return `<section class="timeline-group"><time class="timeline-date">${label}</time><div class="timeline-items">${notes.map((note) => `
      <button class="timeline-item" data-note-id="${note.note_id}" style="--cluster-color:${colorOf(note)}" type="button">
        <strong>${escapeHtml(note.raw_content)}</strong><small>${escapeHtml(note.symbol_id ?? basename(note.file_path))}</small>
      </button>`).join("")}</div></section>`;
  }).join("") || `<div class="list-empty">No memories to place on the chronicle.</div>`;
}

function connectionsFor(note) {
  return state.edges.filter((edge) => edge.source === note.note_id || edge.target === note.note_id).map((edge) => ({ ...edge, otherId: edge.source === note.note_id ? edge.target : edge.source })).sort((a, b) => (a.type === b.type ? b.weight - a.weight : a.type === "semantic" ? -1 : 1));
}

function renderInspector() {
  const note = state.noteById.get(state.selectedId);
  el.emptyInspector.hidden = Boolean(note);
  el.noteInspector.hidden = !note;
  el.inspector.classList.toggle("is-open", Boolean(note));
  if (!note) return;
  const cluster = clusterOf(note);
  const connections = connectionsFor(note);
  el.detailId.textContent = `#${note.note_id}`;
  el.detailContent.innerHTML = renderMarkdown(note.raw_content);
  el.detailDate.textContent = formatDate(timestampOf(note));
  el.detailCluster.innerHTML = `<span style="color:${COLORS[cluster % COLORS.length]}">●</span> Cluster ${String(cluster + 1).padStart(2, "0")}`;
  el.detailFile.textContent = note.file_path ?? "Not recorded";
  el.detailSymbol.textContent = note.symbol_id ?? "Not recorded";
  el.detailCommit.textContent = note.source_commit_ids?.find(Boolean)?.slice(0, 12) ?? "Not recorded";
  el.detailChips.innerHTML = [note.ast_node_type && `<span class="detail-chip accent">${escapeHtml(note.ast_node_type)}</span>`, note.symbol_id && `<span class="detail-chip">symbol</span>`, `${note.embedding?.length ?? 0} dimensions`].filter(Boolean).map((chip) => typeof chip === "string" && chip.startsWith("<") ? chip : `<span class="detail-chip">${chip}</span>`).join("");
  el.connectionCount.textContent = connections.length;
  el.connectionList.innerHTML = connections.slice(0, 30).map((connection) => {
    const other = state.noteById.get(connection.otherId);
    const linkLabel = connection.type === "spectral" ? "spectral cosine" : connection.type === "embedding" ? "embedding cosine" : connection.type;
    return `<button class="connection-row" data-note-id="${connection.otherId}" style="--connection-color:${colorOf(other)}" type="button"><i></i><span>${escapeHtml(other.raw_content)}</span><small>${linkLabel}${["spectral", "embedding"].includes(connection.type) ? ` ${connection.weight.toFixed(2)}` : ""}</small></button>`;
  }).join("") || `<div class="list-empty">This memory has no recorded connections.</div>`;
}

function selectNote(id) {
  if (!state.noteById.has(id)) return;
  state.selectedId = id;
  if (state.graphMode === "neighborhood") el.stageMessage.hidden = true;
  renderInspector();
  el.inspector.scrollTop = 0;
  renderNoteList();
  drawGraph();
}

function selectClickedNote(event) {
  const target = event.target instanceof Element ? event.target.closest("[data-note-id]") : null;
  if (target) selectNote(Number(target.dataset.noteId));
}

function resizeCanvas() {
  const rect = canvas.getBoundingClientRect();
  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  canvas.width = Math.max(1, Math.round(rect.width * dpr));
  canvas.height = Math.max(1, Math.round(rect.height * dpr));
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  drawGraph();
}

function screenPosition(node) {
  const rect = canvas.getBoundingClientRect();
  const { x, y, scale } = state.graph.transform;
  return { x: rect.width / 2 + x + node.x * scale, y: rect.height / 2 + y + node.y * scale };
}

function projectGraph() {
  const filtered = new Set(state.filtered.map((note) => note.note_id));
  let edges = state.showHistory ? state.edges : projectedEdges();
  if (state.graphMode === "bridges") edges = edges.filter((edge) => edge.longRange);
  if (state.graphMode === "neighborhood") {
    if (state.selectedId === null) return { visible: new Set(), edges: [] };
    const distance = new Map([[state.selectedId, 0]]);
    const queue = [state.selectedId];
    while (queue.length) {
      const current = queue.shift();
      if (distance.get(current) >= state.neighborhoodDepth) continue;
      for (const edge of state.edges) {
        if (edge.source !== current && edge.target !== current) continue;
        const other = edge.source === current ? edge.target : edge.source;
        if (filtered.has(other) && !distance.has(other)) { distance.set(other, distance.get(current) + 1); queue.push(other); }
      }
    }
    const visible = new Set(distance.keys());
    edges = edges.filter((edge) => visible.has(edge.source) && visible.has(edge.target));
    const budget = new Map();
    const budgetEdges = [...edges].sort((left, right) => right.weight - left.weight).filter((edge) => {
      const sourceCount = budget.get(edge.source) ?? 0;
      const targetCount = budget.get(edge.target) ?? 0;
      if (sourceCount >= state.neighborhoodBudget || targetCount >= state.neighborhoodBudget) return false;
      budget.set(edge.source, sourceCount + 1);
      budget.set(edge.target, targetCount + 1);
      return true;
    });
    return { visible, edges: budgetEdges };
  }
  if (state.graphMode === "bridges") {
    const visible = new Set();
    edges.forEach((edge) => { if (filtered.has(edge.source) && filtered.has(edge.target)) { visible.add(edge.source); visible.add(edge.target); } });
    return { visible, edges: edges.filter((edge) => visible.has(edge.source) && visible.has(edge.target)) };
  }
  return { visible: filtered, edges: edges.filter((edge) => filtered.has(edge.source) && filtered.has(edge.target)) };
}

function projectedEdges() {
  const projected = new Map();
  for (const edge of state.edges) {
    const source = state.identityRepresentative.get(edge.source) ?? edge.source;
    const target = state.identityRepresentative.get(edge.target) ?? edge.target;
    if (source === target) continue;
    const [left, right] = source < target ? [source, target] : [target, source];
    const key = `${left}:${right}:${edge.type}`;
    const current = projected.get(key);
    if (!current || edge.weight > current.weight) projected.set(key, { ...edge, source: left, target: right });
  }
  return [...projected.values()];
}

function drawGraph() {
  if (!state.data || state.view !== "atlas") return;
  if (state.graphMode === "neighborhood" && state.selectedId === null) {
    el.stageMessage.hidden = false;
    el.stageMessage.querySelector("h2").textContent = "Select a memory";
    el.stageMessage.querySelector("p").textContent = "Neighborhood mode begins with a selected node.";
    el.stageMessage.querySelector(".loader-mark").style.animation = "none";
    return;
  }
  if (state.graphMode === "bridges" && !state.edges.some((edge) => edge.longRange)) {
    el.stageMessage.hidden = false;
    el.stageMessage.querySelector("h2").textContent = "No persisted bridges";
    el.stageMessage.querySelector("p").textContent = "This SMG does not contain long-range links.";
    el.stageMessage.querySelector(".loader-mark").style.animation = "none";
    return;
  }
  if (state.graphMode === "matrix") return drawMatrix();
  const rect = canvas.getBoundingClientRect();
  ctx.clearRect(0, 0, rect.width, rect.height);
  const projection = projectGraph();
  const visible = projection.visible;
  const selectedConnections = state.selectedId === null ? new Set() : new Set(connectionsFor(state.noteById.get(state.selectedId)).map((edge) => edge.otherId));
  const nodeMap = new Map(state.graph.nodes.map((node) => [node.id, node]));
  const edgeTypes = state.graphMode === "bridges" ? new Set(["spectral"]) : state.edgeType === "all" ? new Set(["embedding", "spectral", "history", "topology:call", "topology:import"]) : new Set([state.edgeType]);
  ctx.save();
  for (const edge of projection.edges) {
    const emphasized = state.selectedId !== null && (edge.source === state.selectedId || edge.target === state.selectedId);
    if (!visible.has(edge.source) || !visible.has(edge.target)) continue;
    if (!emphasized && !edgeTypes.has(edge.type)) continue;
    if (!emphasized && edge.type === "embedding" && edge.weight < state.minStrength) continue;
    if (!emphasized && edge.type === "spectral" && edge.weight < state.spectralStrength) continue;
    if (!emphasized && edge.type === "history" && ((state.structuralDegreeById.get(edge.source) ?? 0) < state.minStructuralDegree || (state.structuralDegreeById.get(edge.target) ?? 0) < state.minStructuralDegree)) continue;
    const from = screenPosition(nodeMap.get(edge.source));
    const to = screenPosition(nodeMap.get(edge.target));
    const focusedType = state.edgeType === edge.type;
    ctx.beginPath();
    ctx.moveTo(from.x, from.y);
    ctx.lineTo(to.x, to.y);
    ctx.strokeStyle = edge.type === "history"
      ? `rgba(113,224,208,${emphasized ? .86 : focusedType ? .52 : .14})`
      : edge.type === "spectral" ? `rgba(213,167,232,${emphasized ? .86 : focusedType ? .42 : .16})` : edge.type.startsWith("topology:") ? `rgba(118,184,220,${emphasized ? .86 : focusedType ? .42 : .16})` : `rgba(255,155,103,${emphasized ? .86 : focusedType ? .42 : .16})`;
    ctx.lineWidth = emphasized ? 1.6 : edge.type === "history" ? focusedType ? 1.1 : .6 : focusedType ? Math.max(.65, edge.weight) : Math.max(.4, edge.weight);
    ctx.setLineDash(edge.type === "history" ? focusedType ? [5, 4] : [2, 4] : edge.type === "spectral" ? [8, 4] : edge.type.startsWith("topology:") ? [3, 3] : []);
    ctx.stroke();
  }
  ctx.setLineDash([]);
  const nodes = state.graph.nodes.filter((node) => visible.has(node.id));
  for (const node of nodes) {
    const point = screenPosition(node);
    if (point.x < -30 || point.y < -30 || point.x > rect.width + 30 || point.y > rect.height + 30) continue;
    const selected = node.id === state.selectedId;
    const related = selectedConnections.has(node.id);
    const radius = (selected ? 7 : Math.min(5, 2.2 + Math.sqrt(node.degree) * .35)) * Math.max(.75, Math.min(1.25, state.graph.transform.scale));
    if (selected || related) {
      ctx.beginPath();
      ctx.arc(point.x, point.y, radius + (selected ? 7 : 4), 0, Math.PI * 2);
      ctx.fillStyle = selected ? `${colorOf(node.note)}22` : "rgba(220,232,231,.035)";
      ctx.fill();
    }
    ctx.beginPath();
    ctx.arc(point.x, point.y, radius, 0, Math.PI * 2);
    ctx.fillStyle = colorOf(node.note);
    ctx.globalAlpha = state.selectedId === null || selected || related ? .96 : .24;
    ctx.fill();
    ctx.globalAlpha = 1;
    const labelBudget = state.graph.transform.scale >= 1.7 ? Number.POSITIVE_INFINITY : state.graph.transform.scale >= 1.15 ? 80 : 28;
    if (state.labels && (selected || node.labelRank < labelBudget)) {
      const label = node.note.symbol_id?.split(":").at(-1) ?? node.note.raw_content;
      ctx.font = selected ? "600 11px DM Mono, monospace" : "10px DM Mono, monospace";
      ctx.fillStyle = selected ? "#dce8e7" : "rgba(220,232,231,.72)";
      ctx.fillText(label.slice(0, 34), point.x + radius + 6, point.y + 3);
    }
  }
  ctx.restore();
}

function drawMatrix() {
  const rect = canvas.getBoundingClientRect();
  ctx.clearRect(0, 0, rect.width, rect.height);
  const clusters = [...new Set(state.filtered.map(clusterOf))].sort((a, b) => a - b);
  if (!clusters.length) return;
  const counts = new Map();
  const visible = new Set(state.filtered.map((note) => note.note_id));
  for (const edge of state.edges) {
    if (!visible.has(edge.source) || !visible.has(edge.target)) continue;
    if (state.edgeType !== "all" && edge.type !== state.edgeType) continue;
    if (edge.type === "embedding" && edge.weight < state.minStrength) continue;
    if (edge.type === "spectral" && edge.weight < state.spectralStrength) continue;
    if (edge.type === "history" && ((state.structuralDegreeById.get(edge.source) ?? 0) < state.minStructuralDegree || (state.structuralDegreeById.get(edge.target) ?? 0) < state.minStructuralDegree)) continue;
    const source = clusterOf(state.noteById.get(edge.source));
    const target = clusterOf(state.noteById.get(edge.target));
    const key = `${source}:${target}`;
    counts.set(key, (counts.get(key) ?? 0) + 1);
    if (source !== target) counts.set(`${target}:${source}`, (counts.get(`${target}:${source}`) ?? 0) + 1);
  }
  const max = Math.max(1, ...counts.values());
  const size = Math.min(rect.width, rect.height) * .62;
  const cell = size / clusters.length;
  const left = rect.width / 2 - size / 2;
  const top = rect.height / 2 - size / 2 + 20;
  ctx.font = "10px DM Mono, monospace";
  ctx.textAlign = "center";
  clusters.forEach((cluster, index) => {
    const x = left + index * cell + cell / 2;
    ctx.fillStyle = COLORS[cluster % COLORS.length];
    ctx.fillText(`C${String(cluster + 1).padStart(2, "0")}`, x, top - 14);
    ctx.fillText(`C${String(cluster + 1).padStart(2, "0")}`, left - 25, top + index * cell + cell / 2 + 3);
  });
  clusters.forEach((source, row) => clusters.forEach((target, column) => {
    const count = counts.get(`${source}:${target}`) ?? 0;
    ctx.fillStyle = count ? `rgba(255,155,103,${.08 + .8 * count / max})` : "rgba(220,232,231,.025)";
    ctx.fillRect(left + column * cell + 1, top + row * cell + 1, cell - 2, cell - 2);
    if (count) { ctx.fillStyle = "#dce8e7"; ctx.font = "10px DM Mono, monospace"; ctx.fillText(String(count), left + column * cell + cell / 2, top + row * cell + cell / 2 + 3); }
  }));
  ctx.textAlign = "start";
}

function fitGraph() {
  const rect = canvas.getBoundingClientRect();
  if (!state.graph.nodes.length || !rect.width) return;
  const xs = state.graph.nodes.map((node) => node.x);
  const ys = state.graph.nodes.map((node) => node.y);
  const width = Math.max(1, Math.max(...xs) - Math.min(...xs));
  const height = Math.max(1, Math.max(...ys) - Math.min(...ys));
  state.graph.transform = { x: 0, y: 20, scale: Math.min((rect.width - 110) / width, (rect.height - 135) / height, 1.05) };
  drawGraph();
}

function setView(view) {
  if (view !== "atlas") stopForceLayout();
  state.view = view;
  document.querySelectorAll("[data-view]").forEach((button) => {
    const active = button.dataset.view === view;
    button.classList.toggle("is-active", active);
    button.setAttribute("aria-pressed", String(active));
  });
  const atlas = view === "atlas";
  canvas.hidden = !atlas;
  el.graphKey.hidden = !atlas;
  el.canvasActions.hidden = !atlas;
  el.timeline.hidden = atlas;
  el.viewTitle.textContent = atlas ? "Atlas" : "Chronicle";
  el.stageEyebrow.textContent = atlas ? "Topology / all memory" : "Temporal record / newest first";
  el.stageTitle.textContent = atlas ? "Relationship field" : "Project chronicle";
  el.listLabel.textContent = atlas ? `${state.browseMode[0].toUpperCase()}${state.browseMode.slice(1)}` : "Visible memories";
  if (atlas) requestAnimationFrame(() => { setGraphMode(state.graphMode); resizeCanvas(); }); else renderTimeline();
}

function setGraphMode(mode) {
  if (state.graphMode === "force" && mode !== "force") stopForceLayout();
  state.graphMode = mode;
  el.graphModes.querySelectorAll("button").forEach((button) => {
    const active = button.dataset.graphMode === mode;
    button.classList.toggle("is-active", active);
    button.setAttribute("aria-selected", String(active));
  });
  el.modeControls.hidden = mode !== "neighborhood";
  if (el.forceControls) el.forceControls.hidden = mode !== "force";
  el.graphKey.hidden = mode === "matrix" || state.view !== "atlas";
  el.canvasActions.hidden = mode === "matrix" || state.view !== "atlas";
  if (mode === "matrix") {
    el.stageEyebrow.textContent = "Analysis / cluster relationships";
    el.stageTitle.textContent = "Cluster matrix";
  } else if (mode === "neighborhood") {
    el.stageEyebrow.textContent = "Topology / selected neighborhood";
    el.stageTitle.textContent = "Local relationship field";
  } else if (mode === "bridges") {
    el.stageEyebrow.textContent = "Topology / persisted long-range links";
    el.stageTitle.textContent = "Bridge network";
  } else if (mode === "force") {
    el.stageEyebrow.textContent = "Layout / force-directed graph";
    el.stageTitle.textContent = "Organized relationship field";
  } else {
    el.stageEyebrow.textContent = "Topology / all memory";
    el.stageTitle.textContent = "Relationship field";
  }
  if (mode === "force") startForceLayout();
  if (mode === "neighborhood" && state.selectedId === null) {
    el.stageMessage.hidden = false;
    el.stageMessage.querySelector("h2").textContent = "Select a memory";
    el.stageMessage.querySelector("p").textContent = "Neighborhood mode begins with a selected node.";
    el.stageMessage.querySelector(".loader-mark").style.animation = "none";
  } else if (mode === "bridges" && !state.edges.some((edge) => edge.longRange)) {
    el.stageMessage.hidden = false;
    el.stageMessage.querySelector("h2").textContent = "No persisted bridges";
    el.stageMessage.querySelector("p").textContent = "This SMG does not contain long-range links.";
    el.stageMessage.querySelector(".loader-mark").style.animation = "none";
  } else {
    el.stageMessage.hidden = true;
  }
  drawGraph();
}

async function loadFile(file) {
  try {
    const data = JSON.parse(await file.text());
    await normalizeGraph(data);
    el.sourceName.textContent = `${file.name} · ${formatNumber(state.notes.length)} memories${state.semanticDerived ? " · local semantic map" : ""}`;
    el.sourceStatus.classList.remove("is-error");
    toast(`Mapped ${formatNumber(state.notes.length)} notes from ${file.name}`);
  } catch (error) {
    showError(error instanceof Error ? error.message : "Could not read this graph.");
  }
}

function showLoadState() {
  el.sourceName.textContent = "Awaiting SMG JSON";
  el.stageMessage.hidden = false;
  el.stageMessage.querySelector("h2").textContent = "Open an SMG graph";
  el.stageMessage.querySelector("p").textContent = "Choose or drop an smg.json file to begin.";
  el.stageMessage.querySelector(".loader-mark").style.animation = "none";
}

function showError(message) {
  el.sourceStatus.classList.add("is-error");
  toast(message, true);
}

let toastTimer;
function toast(message, error = false) {
  clearTimeout(toastTimer);
  el.toast.textContent = message;
  el.toast.style.borderColor = error ? "var(--danger)" : "var(--signal)";
  el.toast.classList.add("is-visible");
  toastTimer = setTimeout(() => el.toast.classList.remove("is-visible"), 2800);
}

el.loadButton.addEventListener("click", () => el.fileInput.click());
el.fileInput.addEventListener("change", () => el.fileInput.files?.[0] && loadFile(el.fileInput.files[0]));
el.focusSearch.addEventListener("click", () => el.searchInput.focus());
el.searchInput.addEventListener("input", (event) => { state.query = event.target.value; applyFilters(); });
el.showHistory.addEventListener("change", (event) => {
  state.showHistory = event.target.checked;
  configureStructuralThreshold();
  applyFilters();
});
el.clearFilters.addEventListener("click", () => { state.cluster = null; state.query = ""; el.searchInput.value = ""; renderClusters(); applyFilters(); });
el.browseModes.addEventListener("click", (event) => {
  const button = event.target.closest("[data-browse]");
  if (!button) return;
  state.browseMode = button.dataset.browse;
  el.browseModes.querySelectorAll("button").forEach((item) => {
    const active = item === button;
    item.classList.toggle("is-active", active);
    item.setAttribute("aria-selected", String(active));
  });
  el.listLabel.textContent = `${state.browseMode[0].toUpperCase()}${state.browseMode.slice(1)}`;
  renderNoteList();
});
el.timePresets.addEventListener("click", (event) => {
  const button = event.target.closest("[data-days]");
  if (!button || !state.notes.length) return;
  const days = Number(button.dataset.days);
  const max = Math.max(...state.notes.map(timestampOf).filter(Boolean));
  const end = dateInputValue(max);
  const start = days ? dateInputValue(max - days * 86_400) : "";
  el.timeStart.value = start;
  el.timeEnd.value = days ? end : "";
  el.timePresets.querySelectorAll("button").forEach((item) => item.classList.toggle("is-active", item === button));
  applyTimeWindow(start, days ? end : "", days ? `Last ${days} days` : "All history");
});
const updateDateWindow = () => {
  const start = el.timeStart.value;
  const end = el.timeEnd.value;
  if (start && end && start > end) return toast("The start date must be before the end date", true);
  el.timePresets.querySelectorAll("button").forEach((button) => button.classList.remove("is-active"));
  applyTimeWindow(start, end, start || end ? `${start || "Beginning"} → ${end || "Now"}` : "All history");
};
el.timeStart.addEventListener("change", updateDateWindow);
el.timeEnd.addEventListener("change", updateDateWindow);
el.clusterList.addEventListener("click", (event) => {
  const button = event.target.closest("[data-cluster]");
  if (!button) return;
  const cluster = Number(button.dataset.cluster);
  state.cluster = state.cluster === cluster ? null : cluster;
  renderClusters();
  applyFilters();
});
el.edgeFilter.addEventListener("click", (event) => {
  const button = event.target.closest("[data-edge]");
  if (!button) return;
  state.edgeType = button.dataset.edge;
  el.edgeFilter.querySelectorAll("button").forEach((item) => item.classList.toggle("is-active", item === button));
  updateSemanticThresholdInteractivity();
  updateStructuralThresholdInteractivity();
  configureSpectralThreshold();
  drawGraph();
});
el.topologySnapshot?.addEventListener("change", async (event) => {
  state.topologyCommitId = event.target.value;
  if (state.data) await normalizeGraph(state.data);
  drawGraph();
});
el.strengthThreshold.addEventListener("input", (event) => {
  state.minStrength = sliderToStrength(event.target.value);
  el.thresholdValue.textContent = `≥ ${state.minStrength.toFixed(2)}`;
  drawGraph();
});
el.spectralThreshold.addEventListener("input", (event) => {
  state.spectralStrength = sliderToSpectral(event.target.value);
  el.spectralThresholdValue.textContent = `≥ ${state.spectralStrength.toFixed(2)}`;
  drawGraph();
});
el.structuralThreshold.addEventListener("input", (event) => {
  state.minStructuralDegree = Number(event.target.value);
  el.structuralThresholdValue.textContent = `≥ ${state.minStructuralDegree}`;
  drawGraph();
});
el.noteList.addEventListener("click", selectClickedNote);
el.connectionList.addEventListener("click", selectClickedNote);
el.visualStage.addEventListener("click", selectClickedNote);
el.sortButton.addEventListener("click", () => { state.newestFirst = !state.newestFirst; el.sortButton.style.transform = state.newestFirst ? "none" : "scaleY(-1)"; renderNoteList(); });
el.closeInspector.addEventListener("click", () => { state.selectedId = null; renderInspector(); renderNoteList(); drawGraph(); });
el.copyCommit.addEventListener("click", async () => {
  const note = state.noteById.get(state.selectedId);
  const commit = note?.source_commit_ids?.find(Boolean);
  if (!commit) return toast("No commit recorded for this memory", true);
  if (!navigator.clipboard?.writeText) return toast("Clipboard access is unavailable here", true);
  try {
    await navigator.clipboard.writeText(commit);
    toast("Commit hash copied");
  } catch {
    toast("Could not copy the commit hash", true);
  }
});
el.fitGraph.addEventListener("click", fitGraph);
el.toggleLabels.addEventListener("click", () => { state.labels = !state.labels; el.toggleLabels.classList.toggle("is-active", state.labels); drawGraph(); });
el.neighborhoodDepth.addEventListener("input", (event) => { state.neighborhoodDepth = Number(event.target.value); el.neighborhoodDepthValue.textContent = event.target.value; drawGraph(); });
el.neighborhoodBudget.addEventListener("input", (event) => { state.neighborhoodBudget = Number(event.target.value); el.neighborhoodBudgetValue.textContent = event.target.value; drawGraph(); });
el.graphModes.addEventListener("click", (event) => { const button = event.target.closest("[data-graph-mode]"); if (button) setGraphMode(button.dataset.graphMode); });
el.restartForce?.addEventListener("click", startForceLayout);
el.toggleForce?.addEventListener("click", () => {
  state.force.paused = !state.force.paused;
  state.force.worker?.postMessage({ type: state.force.paused ? "pause" : "resume" });
  if (state.force.paused) state.force.inline?.pause(); else state.force.inline?.resume();
  el.toggleForce.textContent = state.force.paused ? "Resume" : "Pause";
});
document.querySelectorAll("[data-view]").forEach((button) => button.addEventListener("click", () => setView(button.dataset.view)));

canvas.addEventListener("pointerdown", (event) => {
  if (state.graphMode === "matrix") return;
  canvas.setPointerCapture(event.pointerId);
  if (state.graphMode === "force") {
    const rect = canvas.getBoundingClientRect();
    const point = { x: event.clientX - rect.left, y: event.clientY - rect.top };
    const nearest = state.graph.nodes.reduce((best, node) => {
      const screen = screenPosition(node), distance = Math.hypot(screen.x - point.x, screen.y - point.y);
      return distance < (best?.distance ?? 14) ? { node, distance } : best;
    }, null);
    if (nearest) { state.force.draggingId = nearest.node.id; state.force.worker?.postMessage({ type: "pin", id: nearest.node.id, x: nearest.node.x, y: nearest.node.y }); }
  }
  state.graph.drag = { x: event.clientX, y: event.clientY, originX: state.graph.transform.x, originY: state.graph.transform.y, moved: false };
});
canvas.addEventListener("pointermove", (event) => {
  if (!state.graph.drag) return;
  const dx = event.clientX - state.graph.drag.x;
  const dy = event.clientY - state.graph.drag.y;
  if (Math.abs(dx) + Math.abs(dy) > 3) state.graph.drag.moved = true;
  state.graph.transform.x = state.graph.drag.originX + dx;
  state.graph.transform.y = state.graph.drag.originY + dy;
  if (state.graphMode === "force" && state.force.draggingId !== null) {
    const node = state.graph.nodes.find((item) => item.id === state.force.draggingId);
    if (node) { node.x += dx / Math.max(.01, state.graph.transform.scale); node.y += dy / Math.max(.01, state.graph.transform.scale); state.force.worker?.postMessage({ type: "pin", id: node.id, x: node.x, y: node.y }); state.graph.drag.x = event.clientX; state.graph.drag.y = event.clientY; state.graph.drag.originX = state.graph.transform.x; state.graph.drag.originY = state.graph.transform.y; }
  }
  drawGraph();
});
canvas.addEventListener("pointerup", (event) => {
  if (state.graphMode === "matrix") return;
  if (!state.graph.drag?.moved) {
    const rect = canvas.getBoundingClientRect();
    const point = { x: event.clientX - rect.left, y: event.clientY - rect.top };
    const visible = new Set(state.filtered.map((note) => note.note_id));
    let nearest = null;
    let distance = 12;
    for (const node of state.graph.nodes) {
      if (!visible.has(node.id)) continue;
      const screen = screenPosition(node);
      const nextDistance = Math.hypot(screen.x - point.x, screen.y - point.y);
      if (nextDistance < distance) { nearest = node; distance = nextDistance; }
    }
    if (nearest) {
      selectNote(nearest.id);
    } else if (state.selectedId !== null) {
      state.selectedId = null;
      renderInspector();
      renderNoteList();
      drawGraph();
    }
  }
  state.graph.drag = null;
  if (state.force.draggingId !== null) { state.force.worker?.postMessage({ type: "release", id: state.force.draggingId }); state.force.draggingId = null; }
});
canvas.addEventListener("wheel", (event) => {
  event.preventDefault();
  const rect = canvas.getBoundingClientRect();
  const oldScale = state.graph.transform.scale;
  const nextScale = Math.max(.18, Math.min(3.5, oldScale * Math.exp(-event.deltaY * .001)));
  const px = event.clientX - rect.left - rect.width / 2 - state.graph.transform.x;
  const py = event.clientY - rect.top - rect.height / 2 - state.graph.transform.y;
  state.graph.transform.x -= px * (nextScale / oldScale - 1);
  state.graph.transform.y -= py * (nextScale / oldScale - 1);
  state.graph.transform.scale = nextScale;
  drawGraph();
}, { passive: false });

window.addEventListener("keydown", (event) => {
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") { event.preventDefault(); el.searchInput.focus(); }
  if (event.key === "Escape") { state.selectedId = null; renderInspector(); drawGraph(); }
});
window.addEventListener("resize", resizeCanvas);
window.addEventListener("dragenter", (event) => { event.preventDefault(); el.dropZone.classList.add("is-visible"); });
window.addEventListener("dragover", (event) => event.preventDefault());
el.dropZone.addEventListener("dragleave", (event) => { if (event.target === el.dropZone) el.dropZone.classList.remove("is-visible"); });
el.dropZone.addEventListener("drop", (event) => { event.preventDefault(); el.dropZone.classList.remove("is-visible"); const file = event.dataTransfer?.files?.[0]; if (file) loadFile(file); });

const tooltips = {
  focusSearch: "Search memories (⌘K)", loadButton: "Open an SMG JSON file", clearFilters: "Clear cluster and search filters",
  sortButton: "Reverse chronological order", closeInspector: "Close inspector", copyCommit: "Copy full commit hash",
  strengthThreshold: "Hide similarity links below this score (spectral for persisted links, cosine for derived links)", structuralThreshold: "Hide structural links below this node degree",
  neighborhoodDepth: "Maximum hops from the selected memory", neighborhoodBudget: "Maximum displayed edges per neighborhood node",
};
for (const [id, title] of Object.entries(tooltips)) if (el[id]) el[id].title = title;
document.querySelectorAll("[data-view], [data-browse], [data-edge], [data-graph-mode]").forEach((button) => {
  if (!button.title) button.title = `Switch to ${button.textContent.trim()} view`;
});

function installPanelResizer(element, side, storageKey) {
  const root = document.documentElement;
  const readWidth = () => Number.parseFloat(getComputedStyle(root).getPropertyValue(`--${side}-width`));
  const apply = (width) => {
    const clamped = Math.max(side === "library" ? 240 : 260, Math.min(side === "library" ? 520 : 560, width));
    root.style.setProperty(`--${side}-width`, `${clamped}px`);
    localStorage.setItem(storageKey, String(clamped));
    resizeCanvas();
  };
  const saved = Number(localStorage.getItem(storageKey));
  if (Number.isFinite(saved) && saved > 0) apply(saved);
  element.addEventListener("pointerdown", (event) => {
    if (window.matchMedia("(max-width: 1180px)").matches) return;
    event.preventDefault();
    element.setPointerCapture(event.pointerId);
    element.classList.add("is-resizing");
    const startX = event.clientX;
    const startWidth = readWidth();
    const onMove = (move) => apply(startWidth + (side === "library" ? move.clientX - startX : startX - move.clientX));
    const onEnd = () => { element.classList.remove("is-resizing"); element.removeEventListener("pointermove", onMove); element.removeEventListener("pointerup", onEnd); };
    element.addEventListener("pointermove", onMove);
    element.addEventListener("pointerup", onEnd);
  });
  element.addEventListener("keydown", (event) => {
    if (!['ArrowLeft', 'ArrowRight'].includes(event.key)) return;
    event.preventDefault();
    const delta = event.key === "ArrowRight" ? 16 : -16;
    apply(readWidth() + (side === "library" ? delta : -delta));
  });
}
installPanelResizer(el.libraryResizer, "library", "spectral-cortex.library-width");
installPanelResizer(el.inspectorResizer, "inspector", "spectral-cortex.inspector-width");

new ResizeObserver(resizeCanvas).observe(el.visualStage);
showLoadState();
