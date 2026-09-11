/* Deterministic force-layout worker. */
const finite = (value, fallback = 0) => Number.isFinite(value) ? value : fallback;
const hash = (value) => [...String(value)].reduce((acc, char) => Math.imul(acc ^ char.charCodeAt(0), 16777619), 2166136261) >>> 0;
const random = (seed) => {
  let value = seed >>> 0;
  return () => { value = Math.imul(value ^ (value >>> 16), 2246822519) >>> 0; return (value / 4294967296) * 2 - 1; };
};

let simulation = null;
function start(input) {
  const nodes = (input.nodes ?? []).map((node, index) => {
    const next = random(hash(node.id) ^ finite(input.seed, 0));
    return { id: node.id, x: finite(node.x, next() * 300), y: finite(node.y, next() * 220), vx: 0, vy: 0, pinned: false, radius: Math.max(2, finite(node.radius, 3)), index };
  });
  const byId = new Map(nodes.map((node) => [String(node.id), node]));
  const edges = (input.edges ?? []).filter((edge) => edge.source !== edge.target && byId.has(String(edge.source)) && byId.has(String(edge.target))).map((edge) => ({ source: byId.get(String(edge.source)), target: byId.get(String(edge.target)), strength: Math.max(0, Math.min(1, finite(edge.normalized_strength))) }));
  simulation = { nodes, edges, width: Math.max(1, finite(input.width, 1000)), height: Math.max(1, finite(input.height, 700)), settings: input.settings ?? {}, alpha: 1, tick: 0, stable: 0, paused: false };
  post();
  run();
}
function run() {
  if (!simulation || simulation.paused) return;
  const s = simulation;
  const repulsion = finite(s.settings.repulsion, 900);
  const damping = finite(s.settings.damping, .82);
  const cap = Math.max(1, finite(s.settings.iterationCap, 600));
  const epsilon = Math.max(.001, finite(s.settings.convergenceEpsilon, .25));
  const forces = s.nodes.map(() => ({ x: 0, y: 0 }));
  for (let left = 0; left < s.nodes.length; left += 1) for (let right = left + 1; right < s.nodes.length; right += 1) {
    const a = s.nodes[left], b = s.nodes[right];
    const dx = b.x - a.x, dy = b.y - a.y, distance = Math.max(1, Math.hypot(dx, dy));
    const force = repulsion / (distance * distance);
    const fx = dx / distance * force, fy = dy / distance * force;
    forces[left].x -= fx; forces[left].y -= fy; forces[right].x += fx; forces[right].y += fy;
  }
  for (const edge of s.edges) {
    const dx = edge.target.x - edge.source.x, dy = edge.target.y - edge.source.y, distance = Math.max(1, Math.hypot(dx, dy));
    const force = (distance - 70) * (.012 + edge.strength * .08);
    const fx = dx / distance * force, fy = dy / distance * force;
    forces[edge.source.index].x += fx; forces[edge.source.index].y += fy; forces[edge.target.index].x -= fx; forces[edge.target.index].y -= fy;
  }
  let displacement = 0;
  for (const [index, node] of s.nodes.entries()) {
    if (node.pinned) { node.vx = 0; node.vy = 0; continue; }
    const centerX = (s.width / 2 - node.x) * .0008, centerY = (s.height / 2 - node.y) * .0008;
    node.vx = (node.vx + (forces[index].x + centerX) * s.alpha) * damping;
    node.vy = (node.vy + (forces[index].y + centerY) * s.alpha) * damping;
    node.x = Math.max(-s.width, Math.min(s.width * 2, finite(node.x + node.vx)));
    node.y = Math.max(-s.height, Math.min(s.height * 2, finite(node.y + node.vy)));
    displacement = Math.max(displacement, Math.hypot(node.vx, node.vy));
  }
  s.tick += 1; s.alpha *= .985;
  if (displacement < epsilon) s.stable += 1; else s.stable = 0;
  const done = s.stable >= Math.max(1, finite(s.settings.stableTicks, 8)) || s.tick >= cap || s.alpha <= .001;
  post(done ? (s.tick >= cap ? "capped" : "converged") : "running");
  if (!done) setTimeout(run, 16);
}
function post(status = "running") { if (simulation) self.postMessage({ type: "positions", positions: simulation.nodes.map(({ id, x, y, vx, vy }) => ({ id, x, y, vx, vy })), alpha: simulation.alpha, tick: simulation.tick, status }); }
self.onmessage = ({ data }) => {
  if (data?.type === "init") start(data);
  else if (data?.type === "stop") simulation = null;
  else if (data?.type === "pause" && simulation) simulation.paused = true;
  else if (data?.type === "resume" && simulation) { simulation.paused = false; run(); }
  else if (data?.type === "pin" && simulation) { const node = simulation.nodes.find((item) => String(item.id) === String(data.id)); if (node) { node.pinned = true; node.x = finite(data.x, node.x); node.y = finite(data.y, node.y); } }
  else if (data?.type === "release" && simulation) { const node = simulation.nodes.find((item) => String(item.id) === String(data.id)); if (node) node.pinned = false; }
};
