import init, {
  HeatmapView,
  PlotViewport,
  SeriesView,
} from "./pkg/bd814209a52e4eb9d9eeca5b6836e5c41dc2fa26/rustplotlib_web.js";
import { bindPlotNavigation, bindColorbarEditor } from "./navigation.js";
import { ArrayCache, readArray, MAX_CELLS } from "./arrays.js";
import { receiverIssues } from "./health.js?v=20261023-0700";
import { panelKey as key, groupProducts, soundingColumns } from "./groups.js";
await init();
await document.fonts.load("18px RustplotlibPagella").catch(() => {});
await document.fonts.ready;
const $ = (s) => document.querySelector(s),
  root = $("#plots"),
  cards = new Map(),
  groups = new Map(),
  support = new Map(),
  cache = new ArrayCache();
const day = $("#day"),
  station = $("#station"),
  kind = $("#product-kind");
const saved = (name, fallback) => {
  try {
    return JSON.parse(localStorage.getItem(name)) ?? fallback;
  } catch {
    return fallback;
  }
};
let order = saved("iono2-order", []),
  hidden = new Set(saved("iono2-hidden", [])),
  products = [],
  bands = [],
  generation = 0,
  timer,
  queue = [],
  busy = false,
  loading = false;
let limitData = null,
  mapView = null,
  historyView = null,
  selectedPath = null;
// Two columns can expose three stacked panels apiece, plus adjoining rows.
// Keep a fixed ceiling; offscreen plots still dispose both canvas buffers.
const MAX_VIEWS = 8;
const faults = new Map();
let currentHealth = null, requestFailures = 0;
const receiverViews = ["TGO", "DOB", "DMB", "KHO", "W2NAF"];
const networkReceivers = receiverViews.filter(name => name !== "DMB");
const tabHost = $("#station-tabs");
let activeView = localStorage.getItem("iono2-active-view") || localStorage.getItem("iono2-station") || "TGO";
if (![...receiverViews, "propagation"].includes(activeView)) activeView = "TGO";
let networkMapView = null, unbindNetworkColor = null, networkColorLimits = null, networkGeneration = 0;
const healthByStation = new Map();

function updateStationTab(name, health) {
  if (!health) return;
  healthByStation.set(name, health);
  const button = tabHost.querySelector(`[data-view="${name}"]`), value = button?.querySelector("span");
  if (!button || !value) return;
  const used = Number(health.cpu_percent), free = Number.isFinite(used) ? Math.max(0, 100 - used) : null;
  value.textContent = free == null ? "CPU unavailable" : `CPU ${used.toFixed(0)}% · ${free.toFixed(0)}% free`;
  button.dataset.health = !health.ok || used >= 92 ? "bad" : used >= 80 ? "warn" : "ok";
  button.title = free == null ? `${name} CPU telemetry unavailable` : `${name}: ${used.toFixed(1)}% CPU used, ${free.toFixed(1)}% remaining`;
}

function saveTabOrder() {
  localStorage.setItem("iono2-tab-order", JSON.stringify([...tabHost.querySelectorAll(".station-tab")].map(b => b.dataset.view)));
}

function installStationTabs() {
  const savedOrder = saved("iono2-tab-order", []), buttons = new Map([...tabHost.querySelectorAll(".station-tab")].map(b => [b.dataset.view, b]));
  for (const name of [...savedOrder, ...buttons.keys()]) if (buttons.has(name)) tabHost.insertBefore(buttons.get(name), tabHost.querySelector(".tab-hint"));
  for (const button of buttons.values()) {
    button.onclick = () => activateView(button.dataset.view, true);
    button.ondragstart = e => { e.dataTransfer.setData("text/plain", button.dataset.view); button.classList.add("dragging"); };
    button.ondragend = () => { button.classList.remove("dragging"); for (const b of buttons.values()) b.classList.remove("drop-target"); };
    button.ondragover = e => { e.preventDefault(); button.classList.add("drop-target"); };
    button.ondragleave = () => button.classList.remove("drop-target");
    button.ondrop = e => {
      e.preventDefault(); button.classList.remove("drop-target");
      const moved = buttons.get(e.dataTransfer.getData("text/plain"));
      if (moved && moved !== button) { tabHost.insertBefore(moved, button); saveTabOrder(); }
    };
  }
}

function showView(name) {
  const network = name === "propagation";
  root.hidden = network;
  $("#station-limits").hidden = network;
  $("#station-health").hidden = network;
  $("#propagation-view").hidden = !network;
  if (network) { $("#empty").hidden = true; $("#progress-wrap").hidden = true; }
  for (const button of tabHost.querySelectorAll(".station-tab")) {
    const selected = button.dataset.view === name;
    button.setAttribute("aria-selected", String(selected));
    button.tabIndex = selected ? 0 : -1;
  }
}

function activateView(name, reload = false) {
  activeView = name;
  localStorage.setItem("iono2-active-view", name);
  showView(name);
  clearTimeout(timer);
  if (name === "propagation") {
    for (const c of cards.values()) release(c);
    if (day.value) loadNetworkPropagation();
    return;
  }
  station.value = name;
  localStorage.setItem("iono2-station", name);
  if (reload && day.value) loadDay(true);
}

async function refreshStationHealth() {
  if (!day.value) return;
  await Promise.all(networkReceivers.map(async name => {
    try { const response = await json(`api/day?day=${day.value}&station=${name}`); updateStationTab(name, response.health); } catch {}
  }));
}
function updateAttention() {
  const issues = [...faults.values(), ...receiverIssues(currentHealth, station.value, day.value)];
  for (const c of cards.values())
    if (c.alive && c.failures >= 3)
      issues.push(`${c.el.querySelector("h2").textContent}: repeated data-transfer failures.`);
  const unique = [...new Set(issues)], button = $("#attention-open"), list = $("#attention-list");
  button.hidden = unique.length === 0;
  button.textContent = `Needs attention (${unique.length})`;
  $("#attention-context").textContent = `${station.value} · ${day.value} UTC`;
  list.replaceChildren();
  for (const message of unique.length ? unique : ["No active faults."]) {
    const li = document.createElement("li"); li.textContent = message; list.append(li);
  }
}
setInterval(updateAttention, 15000);
station.value = localStorage.getItem("iono2-station") || "TGO";
kind.value = new URLSearchParams(location.search).get("product") || "";
const json = async (url, signal) => {
  const r = await fetch(url, { cache: "no-cache", signal });
  if (!r.ok) throw Error(`Service HTTP ${r.status}`);
  return r.json();
};
const stamp = (t) =>
  new Date(t * 1000).toISOString().replace("T", " ").slice(0, 19) + " UTC";
const title = (p) =>
  p.kind === "spectrum"
    ? `${p.station} full-band spectrum`
    : p.kind === "rti"
      ? `${p.transmitter} → ${p.station} · ${p.quantity === "snr" ? "SNR" : p.quantity === "peak-frequency" ? "Frequency" : "Frequency / range"}`
      : p.kind === "detections"
        ? `${p.station} · ${p.transmitter} sweeps · ${p.quantity === "peak-frequency" ? "Range" : "Frequency"}`
        : `${p.transmitter} → ${p.station} · Ionogram`;
const limits = (a) => {
  const d = (a.at(-1) - a[0]) / (a.length - 1);
  return [a[0] - d / 2, a.at(-1) + d / 2];
};
function save() {
  order = [...root.children].map((e) => e.dataset.key);
  localStorage.setItem("iono2-order", JSON.stringify(order));
  localStorage.setItem("iono2-hidden", JSON.stringify([...hidden]));
}
function release(c) {
  c.abort?.abort();
  c.resize?.disconnect();
  c.unbind?.();
  c.uncolor?.();
  if (c.renderer) {
    c.savedView = Array.from(c.viewport.bounds());
    c.renderer.free();
    c.viewport.free();
  }
  c.renderer = null;
  c.viewport = null;
  root.dataset.activeViews = String(
    [...cards.values()].filter((v) => v.renderer).length,
  );
  c.resize = null;
  c.unbind = null;
  c.uncolor = null;
  c.layout = null;
  c.redraw = null;
  c.supportRevision = null;
  for (const name of [".field", ".axes"]) {
    const canvas = c.el.querySelector(name),
      replacement = canvas.cloneNode(false);
    replacement.width = 1;
    replacement.height = 1;
    canvas.replaceWith(replacement);
  }
  c.el.querySelector(".plot-progress").hidden = true;
}
function dispose(c) {
  observer.unobserve(c.el);
  c.alive = false;
  release(c);
  cards.delete(c.el.dataset.key);
  c.el.remove();
}
function enqueue(c) {
  if (!queue.includes(c)) queue.push(c);
  drain();
}
function drain() {
  if (busy || document.hidden) return;
  while (queue.length) {
    const c = queue.shift();
    if (!c.alive || !c.visible || c.busy) continue;
    if (
      !c.renderer &&
      [...cards.values()].filter((v) => v.renderer).length >= MAX_VIEWS
    ) {
      queue.unshift(c);
      return;
    }
    busy = true;
    c.busy = true;
    render(c).finally(() => {
      c.busy = false;
      busy = false;
      if (c.alive && c.visible && (!c.renderer || c.loaded !== c.current.id))
        queue.push(c);
      setTimeout(drain, 0);
    });
    return;
  }
}
const observer = new IntersectionObserver(
  (entries) => {
    for (const e of entries) {
      const c = cards.get(e.target.dataset.key);
      if (!c) continue;
      c.visible = e.isIntersecting;
      if (c.visible) {
        if (!c.renderer || c.loaded !== c.current.id) enqueue(c);
      } else release(c);
    }
    drain();
  },
  { rootMargin: "0px" },
);
function initialize(c, p) {
  const field = c.el.querySelector(".field"),
    axes = c.el.querySelector(".axes");
  c.renderer = new HeatmapView(field, axes);
  root.dataset.activeViews = String(
    [...cards.values()].filter((v) => v.renderer).length,
  );
  c.viewport = new PlotViewport();
  c.renderer.set_font_size(innerWidth < 650 ? 16 : 18);
  c.renderer.set_markers(false);
  c.renderer.set_palette(p.quantity === "ionogram" ? "greys" : p.palette);
  c.renderer.set_x_labels_visible(c.bottom);
  c.renderer.set_reduction(
    ["snr", "spectrum"].includes(p.quantity) ? "max" : "nearest",
  );
  const caption = p.xlabel.includes("UTC")
    ? `Time (UTC), ${new Date(day.value + "T12:00:00Z").toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric", timeZone: "UTC" })}`
    : p.xlabel;
  c.renderer.set_labels(
    caption,
    p.ylabel,
    p.color_label,
    p.xlabel.includes("UTC"),
  );
  c.renderer.set_scale(c.low ?? p.low, c.high ?? p.high);
  if (p.kind === "detections" && p.quantity === "peak-frequency")
    for (const b of bands) c.renderer.add_band(b.min_km, b.max_km, b.label);
  const resize = () => {
    if (!c.renderer) return;
    const r = c.el.querySelector(".plot").getBoundingClientRect();
    c.renderer.resize(
      Math.max(270, r.width),
      r.height,
      Math.min(1.5, devicePixelRatio || 1),
    );
    const [left, top, width, height] = c.renderer.plot_rect();
    Object.assign(field.style, {
      left: `${left}px`,
      top: `${top}px`,
      width: `${width}px`,
      height: `${height}px`,
    });
  };
  c.resize = new ResizeObserver(resize);
  c.layout = resize;
  c.resize.observe(c.el.querySelector(".plot"));
  resize();
  const redraw = () => {
    if (!c.viewport) return;
    const bounds = Array.from(c.viewport.bounds());
    c.group.timeView = [bounds[0], bounds[2]];
    for (const member of c.group.members) {
      const view = member === c ? bounds : member.viewport
        ? Array.from(member.viewport.bounds()) : (member.savedView || [0, 0, 1, 1]);
      view[0] = bounds[0]; view[2] = bounds[2];
      if (member.viewport) {
        member.viewport.set_bounds(...view);
        member.renderer.set_view(...view);
      } else member.savedView = view;
    }
  };
  c.redraw = redraw;
  c.unbind = bindPlotNavigation(field, c.viewport, redraw);
  c.uncolor = bindColorbarEditor(axes, c.renderer, {
    automaticLimits: () => [c.current.low, c.current.high],
    onChange: (lo, hi, auto) => {
      c.low = auto ? undefined : lo;
      c.high = auto ? undefined : hi;
    },
  });
  field.ondblclick = () => {
    c.viewport.reset();
    redraw();
  };
}
async function render(c) {
  const p = c.current,
    gen = generation,
    abort = new AbortController();
  c.abort = abort;
  const bar = c.el.querySelector(".plot-progress"),
    label = bar.querySelector("span"),
    progress = bar.querySelector("progress");
  try {
    if (p.nx * p.ny > MAX_CELLS) throw Error("Array too large");
    const hadRenderer = Boolean(c.renderer);
    if (!c.renderer) initialize(c, p);
    c.renderer.set_scale(c.low ?? p.low, c.high ?? p.high);
    const oldView = c.savedView || Array.from(c.viewport.bounds()),
      xb = limits(p.x),
      yb = limits(p.y);
    if (c.group.timeView) [oldView[0], oldView[2]] = c.group.timeView;
    c.supportRevision = null;
    let values = cache.get(p.id);
    const geometry = JSON.stringify([p.nx, p.ny, p.x, p.y]);
    if (values || !hadRenderer || c.geometry !== geometry) {
      if (p.quantity === "ionogram")
        c.renderer.set_rectilinear_data(
          new Float64Array(p.x),
          new Float64Array(p.y),
          values || new Float32Array(p.nx * p.ny).fill(NaN),
        );
      else
        c.renderer.set_data(
          p.nx,
          p.ny,
          values || new Float32Array(p.nx * p.ny).fill(NaN),
          ...xb,
          ...yb,
        );
    }
    c.geometry = geometry;
    c.viewport.set_bounds(...oldView);
    c.renderer.set_view(...oldView);
    c.savedView = null;
    if (!values) {
      bar.hidden = false;
      label.textContent = "Loading";
      progress.value = 0;
      const response = await fetch(`api/data/${p.id}`, {
        signal: abort.signal,
        cache: "force-cache",
      });
      if (response.status === 404) {
        loadDay();
        throw Error("Array cache expired");
      }
      values = await readArray(response, p.nx, p.ny, {
        signal: abort.signal,
        onRows: (row, block) => {
          if (
            c.alive &&
            c.renderer &&
            !abort.signal.aborted &&
            generation === gen
          )
            c.renderer.update_rows(row, block);
        },
        onProgress: (f) => {
          progress.value = f;
          label.textContent = `Loading ${Math.round(f * 100)}%`;
        },
      });
      cache.set(p.id, values);
      root.dataset.cachedArrayBytes = String(cache.bytes);
    }
    if (abort.signal.aborted || !c.alive || gen !== generation) return;
    c.loaded = p.id;
    applyTimeSupport(c);
    c.failures = 0;
    updateAttention();
    bar.hidden = true;
    updateCoverage(c.group);
  } catch (e) {
    if (e.name !== "AbortError") {
      c.failures = (c.failures || 0) + 1;
      updateAttention();
      c.loaded = p.id;
      label.textContent = "Connection interrupted · retrying";
      progress.removeAttribute("value");
      bar.hidden = false;
      setTimeout(
        () => {
          if (c.alive && c.visible) {
            c.loaded = null;
            enqueue(c);
          }
        },
        Math.min(30000, 2000 * c.failures),
      );
    }
  }
}
function create(p) {
  const el = $("#card").content.firstElementChild.cloneNode(true),
    k = key(p),
    c = { el, current: p, visible: false, busy: false, alive: true };
  el.dataset.key = k;
  el.className = "plot-panel";
  cards.set(k, c);
  observer.observe(el);
  return c;
}
function createGroup(description) {
  const el = document.createElement("article"),
    header = $("#card").content.querySelector(".card-head").cloneNode(true),
    panels = document.createElement("div"),
    coverage = document.createElement("p"),
    g = {el, panels, coverage, members: [], key:description.key};
  el.className = "card plot-group";
  el.dataset.key = g.key;
  panels.className = "panels";
  coverage.className = "coverage";
  el.append(header, panels, coverage);
  groups.set(g.key, g);
  const head = header;
  head.ondragstart = (e) => {
    e.dataTransfer.setData("text/plain", g.key);
    el.classList.add("dragging");
  };
  head.ondragend = () => el.classList.remove("dragging");
  el.ondragover = (e) => e.preventDefault();
  el.ondrop = (e) => {
    e.preventDefault();
    const moved = groups.get(e.dataTransfer.getData("text/plain"));
    if (moved && moved !== g) {
      root.insertBefore(moved.el, el);
      save();
    }
  };
  return g;
}
function updateCoverage(g) {
  const p = g.members.find(c => c.current.quantity === "snr")?.current || g.members[0]?.current;
  if (!p) return;
  g.coverage.textContent = p.xlabel.includes("UTC")
    ? `${p.occupied_minutes} ${p.kind === "detections" ? "minutes with detections" : "sampled minutes"}`
    : stamp(p.timestamp);
}
function applyTimeSupport(c) {
  if (c.current.kind !== "rti" || !c.renderer || c.loaded !== c.current.id) return;
  const columns = support.get(c.group.key);
  if (!columns) return;
  const revision = c.current.id + ":" + columns.join(",");
  if (revision === c.supportRevision) return;
  c.renderer.set_column_support(columns, 15);
  c.supportRevision = revision;
}
function apply() {
  const list = products.filter(
      (p) => (!kind.value || p.kind === kind.value) && !hidden.has(key(p)),
    ),
    wanted = new Set(list.map(key));
  for (const c of [...cards.values()])
    if (!wanted.has(c.el.dataset.key)) dispose(c);
  const descriptions = groupProducts(list, order), groupKeys = new Set(descriptions.map(g=>g.key));
  for (const [k, g] of groups) if (!groupKeys.has(k)) { g.el.remove(); groups.delete(k); }
  for (const [i, description] of descriptions.entries()) {
    const g = groups.get(description.key) || createGroup(description);
    g.members = [];
    if (description.kind === "rti" && limitData)
      support.set(g.key, soundingColumns(limitData.records, description.station, description.transmitter, day.value));
    g.el.classList.toggle("group-multiple", description.panels.length > 1);
    g.el.querySelector("h2").textContent = description.kind === "rti"
      ? `${description.transmitter} → ${description.station} · RTI`
      : description.kind === "detections" ? `${description.station} · ${description.transmitter} sweeps`
      : title(description.panels[0]);
    for (const [j, p] of description.panels.entries()) {
      const c = cards.get(key(p)) || create(p), changed = c.current.id !== p.id;
      c.current = p;
      c.group = g;
      g.members.push(c);
      c.bottom = j === description.panels.length - 1;
      c.el.querySelector("h2").textContent = title(p);
      c.el.setAttribute("aria-label", title(p));
      if (g.panels.children[j] !== c.el) g.panels.insertBefore(c.el, g.panels.children[j] || null);
      if (c.renderer) { c.renderer.set_x_labels_visible(c.bottom); c.layout(); }
      if (changed && c.visible) enqueue(c);
    }
    if (root.children[i] !== g.el) root.insertBefore(g.el, root.children[i] || null);
    updateCoverage(g);
  }
  $("#empty").hidden = list.length > 0 || loading;
  settingsList();
}
function settingsList() {
  if (!$("#settings").open) return;
  const orderHost = $("#plot-order-settings");
  const host = $("#plot-settings");
  if (host.contains(document.activeElement) || orderHost.contains(document.activeElement)) return;
  orderHost.replaceChildren();
  let draggedOrderRow = null;
  const commitOrder = () => {
    order = [...orderHost.children].map((row) => row.dataset.key);
    localStorage.setItem("iono2-order", JSON.stringify(order));
    for (const k of order) {
      const group = groups.get(k);
      if (group) root.append(group.el);
    }
    for (const [index, row] of [...orderHost.children].entries()) {
      const [up, down] = row.querySelectorAll("button");
      up.disabled = index === 0;
      down.disabled = index === orderHost.children.length - 1;
    }
  };
  const moveRow = (row, direction) => {
    if (direction < 0 && row.previousElementSibling)
      orderHost.insertBefore(row, row.previousElementSibling);
    else if (direction > 0 && row.nextElementSibling)
      orderHost.insertBefore(row.nextElementSibling, row);
    commitOrder();
    row.querySelector("button")?.focus();
  };
  for (const [index, groupElement] of [...root.children].entries()) {
    const row = document.createElement("li"),
      grip = document.createElement("span"),
      label = document.createElement("span"),
      controls = document.createElement("span");
    row.className = "plot-order-row";
    row.dataset.key = groupElement.dataset.key;
    row.draggable = true;
    grip.className = "plot-order-grip";
    grip.textContent = "⠿";
    grip.setAttribute("aria-hidden", "true");
    label.className = "plot-order-label";
    label.textContent = groupElement.querySelector(":scope > .card-head h2")?.textContent || groupElement.dataset.key;
    controls.className = "plot-order-controls";
    for (const [symbol, direction, name] of [["↑", -1, "Move up"], ["↓", 1, "Move down"]]) {
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = symbol;
      button.title = name;
      button.setAttribute("aria-label", `${name}: ${label.textContent}`);
      button.disabled = direction < 0 ? index === 0 : index === root.children.length - 1;
      button.onclick = () => moveRow(row, direction);
      controls.append(button);
    }
    row.append(grip, label, controls);
    row.ondragstart = (event) => {
      event.dataTransfer.effectAllowed = "move";
      event.dataTransfer.setData("text/plain", row.dataset.key);
      draggedOrderRow = row;
      row.classList.add("dragging");
    };
    row.ondragend = () => {
      draggedOrderRow = null;
      row.classList.remove("dragging");
      commitOrder();
      settingsList();
    };
    row.ondragover = (event) => {
      event.preventDefault();
      const moved = draggedOrderRow;
      if (!moved || moved === row) return;
      const before = event.clientY < row.getBoundingClientRect().top + row.offsetHeight / 2;
      orderHost.insertBefore(moved, before ? row : row.nextElementSibling);
    };
    orderHost.append(row);
  }
  host.replaceChildren();
  for (const p of products) {
    const k = key(p),
      c = cards.get(k),
      row = document.createElement("div");
    row.className = "plot-setting";
    const label = document.createElement("h3");
    label.textContent = title(p);
    row.append(label);
    for (const [name, fn] of [
      [
        hidden.has(k) ? "Show" : "Hide",
        () => {
          if (hidden.has(k)) hidden.delete(k);
          else hidden.add(k);
          save();
          apply();
        },
      ],
      [
        "Reset view",
        () => {
          c?.viewport?.reset();
          c?.redraw?.();
        },
      ],
    ]) {
      const b = document.createElement("button");
      b.textContent = name;
      b.onclick = fn;
      row.append(b);
    }
    host.append(row);
  }
}
const age = (t) => ageSeconds(Date.now() / 1000 - t);
const ageSeconds = (value) => {
  const seconds = Math.max(0, value);
  return seconds < 120
    ? `${Math.round(seconds)} s ago`
    : seconds < 7200
      ? `${Math.round(seconds / 60)} min ago`
      : `${(seconds / 3600).toFixed(1)} h ago`;
};
function renderHealth(h) {
  currentHealth = h;
  updateAttention();
  const host = $("#health-data");
  host.replaceChildren();
  if (!h) {
    host.textContent = "Receiver health has not arrived.";
    return;
  }
  const memory = h.memory || {},
    used = memory.MemTotal
      ? 100 * (1 - memory.MemAvailable / memory.MemTotal)
      : null,
    temperatures = (h.temperatures || [])
      .map((v) => `${v.sensor}: ${v.celsius.toFixed(1)} °C`)
      .join(" · ");
  for (const [name, value] of [
    [
      "CPU",
      h.cpu_percent == null
        ? "Sampling"
        : `${h.cpu_percent.toFixed(1)}% used · ${Math.max(0, 100 - h.cpu_percent).toFixed(1)}% free`,
    ],
    [
      "Load (1 / 5 / 15 min)",
      (h.load_average || []).map((v) => v.toFixed(2)).join(" / "),
    ],
    ["Logical CPUs", h.logical_cpus],
    [
      "Memory",
      used == null
        ? "Unavailable"
        : `${used.toFixed(1)}% · ${(memory.MemAvailable / 2 ** 30).toFixed(1)} GiB available`,
    ],
    ["Temperatures", temperatures || "Hardware sensors unavailable"],
    ...(h.disks || []).map((d) => [
      d.label,
      `${d.used_percent.toFixed(1)}% used · ${(d.free_bytes / 2 ** 30).toFixed(1)} GiB free`,
    ]),
    [
      "Acquisition",
      h.ringbuffer?.digital_rf_ok
        ? `Receiving · newest samples ${Math.max(0, h.ringbuffer.newest_age_s).toFixed(1)} s old`
        : "Not reported",
    ],
    [
      "Science products",
      h.output?.exists
        ? `${Math.max(0, h.output.newest_age_s).toFixed(0)} s since latest output`
        : "Not reported",
    ],
    ...(h.generated_unix
      ? [["Disk / acquisition status", stamp(h.generated_unix)]]
      : []),
    ["CPU / sensors updated", `${stamp(h.time)} · ${age(h.time)}`],
  ]) {
    const term = document.createElement("dt"),
      v = document.createElement("dd");
    term.textContent = name;
    v.textContent = value;
    host.append(term, v);
  }
}
async function frequency() {
  const gen = generation;
  try {
    const p = await json(
      `api/frequency-limits?day=${day.value}&station=${station.value}`,
    );
    if (gen !== generation) return;
    faults.delete("frequency"); updateAttention();
    limitData = p;
    for (const g of groups.values()) {
      const product = g.members[0]?.current;
      if (product?.kind === "rti") support.set(g.key, soundingColumns(p.records, product.station, product.transmitter, day.value));
    }
    for (const c of cards.values()) applyTimeSupport(c);
    p.records.sort((a, b) => a.time - b.time);
    p.paths ||= [...new Map(p.records.map((r) => [r.transmitter, r])).values()];
    const latest = new Map(p.paths.map((r) => [r.transmitter, r]));
    const body = $("#path-rows");
    body.replaceChildren();
    for (const r of latest.values()) {
      const tr = document.createElement("tr");
      for (const value of [
        `${r.transmitter} → ${r.receiver}`,
        r.lower_mhz == null
          ? "No supported echo band"
          : `${r.lower_mhz.toFixed(2)}–${r.upper_mhz.toFixed(2)} MHz`,
        new Date(r.time * 1000).toISOString().slice(11, 19),
        `${Math.round(r.range_gate_km[0])}–${Math.round(r.range_gate_km[1])} km`,
        [
          `${r.support_bins} bins`,
          r.bands.length > 1 ? `${r.bands.length} separate bands` : null,
          r.quality_flags?.includes("sampling-edge-limited")
            ? "Sampling edge reached"
            : null,
          pathFreshness(r),
        ]
          .filter(Boolean)
          .join(" · "),
      ]) {
        const td = document.createElement("td");
        td.textContent = value;
        tr.append(td);
      }
      tr.tabIndex = 0;
      tr.onclick = () => drawHistory(r.transmitter);
      tr.onkeydown = (e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          drawHistory(r.transmitter);
        }
      };
      body.append(tr);
    }
    if (!latest.size) {
      const tr = document.createElement("tr"),
        td = document.createElement("td");
      td.colSpan = 5;
      td.textContent = "No frequency-limit evidence published for this day.";
      tr.append(td);
      body.append(tr);
    }
    drawMap([...latest.values()], gen);
    if (selectedPath) drawHistory(selectedPath);
  } catch {
    if (gen === generation) {
      faults.set("frequency", "Frequency estimates could not be refreshed; map values are unverified.");
      updateAttention();
      $("#path-rows").textContent =
        "Frequency update unavailable; map values are unverified until the next successful update.";
      if (limitData?.paths)
        drawMap(
          limitData.paths.map((r) => ({ ...r, freshness: "unverified" })),
          gen,
        );
    }
  }
}
function pathFreshness(r) {
  if (r.freshness === "archived") return "Archived";
  if (r.freshness === "clock-ahead") return "Receiver clock ahead";
  if (r.freshness === "stale")
    return `Stale · ${ageSeconds(r.age_s)} (limit ${Math.round(r.stale_after_s / 60)} min)`;
  if (r.freshness === "current") return ageSeconds(r.age_s);
  return "Freshness unverified";
}
const echoLimits = [
  ["lower_mhz", "Lower limit", "#3863a3"],
  ["upper_mhz", "Upper limit", "#bd3f4f"],
];
let mapColorLimits = null, unbindMapColor;
function drawHistory(tx) {
  if (!limitData) return;
  selectedPath = tx;
  $("#history-wrap").hidden = false;
  $("#history-title").textContent =
    `${tx} → ${station.value} · Detected echo band (MHz)`;
  const canvas = $("#frequency-history"),
    r = canvas.getBoundingClientRect();
  historyView?.free();
  historyView = new SeriesView(canvas);
  historyView.set_color_key(echoLimits.map(v => v[2]), echoLimits.map(v => v[1]));
  const start = Date.parse(day.value + "T00:00:00Z") / 1000;
  historyView.begin(
    r.width,
    280,
    Math.min(1.5, devicePixelRatio || 1),
    start,
    start + 86400,
    0,
    Math.max(
      25,
      ...limitData.records
        .filter((p) => p.transmitter === tx)
        .map((p) => p.upper_mhz || 0),
    ),
    "",
    "",
  );
  historyView.set_time_axis();
  const pathRecords = limitData.records.filter((p) => p.transmitter === tx),
    deltas = pathRecords
      .slice(1)
      .map((p, i) => p.time - pathRecords[i].time)
      .filter((t) => t > 0)
      .sort((a, b) => a - b),
    gap = Math.max(900, 3 * (deltas[Math.floor(deltas.length / 2)] || 300));
  for (const [field, , color] of echoLimits) {
    const xy = [];
    let previous = null;
    for (const p of pathRecords) {
      if (previous != null && p.time - previous > gap) xy.push(NaN, NaN);
      xy.push(p.time, p[field] ?? NaN);
      previous = p.time;
    }
    historyView.series(new Float64Array(xy), color, true, 1.7);
  }
}
let coastlines;
async function drawMap(records, gen = generation) {
  const canvas = $("#path-map");
  const p = limitData?.positions || {},
    located = records.filter((r) => p[r.transmitter] && p[r.receiver]);
  canvas.hidden = !located.length;
  if (!located.length) {
    unbindMapColor?.();
    mapView?.free();
    mapView = null;
    canvas.width = 1;
    canvas.height = 1;
    return;
  }
  coastlines ??= await json("assets/coastlines.json");
  if (gen !== generation) return;
  const coords = located.flatMap((r) => [p[r.transmitter], p[r.receiver]]),
    west = Math.min(...coords.map((p) => p.lon)) - 8,
    east = Math.max(...coords.map((p) => p.lon)) + 8,
    south = Math.min(...coords.map((p) => p.lat)) - 5,
    north = Math.min(85, Math.max(...coords.map((p) => p.lat)) + 5);
  unbindMapColor?.();
  mapView?.free();
  mapView = new SeriesView(canvas);
  const automaticLimits = () => [0, Math.max(25, ...located
    .filter(row => ["current", "archived"].includes(row.freshness))
    .map(row => Math.ceil((row.upper_mhz || 0) / 5) * 5))];
  mapView.set_color_scale(...(mapColorLimits || automaticLimits()), "viridis", "Upper usable frequency (MHz)");
  const r = canvas.getBoundingClientRect();
  mapView.begin_map(
    r.width,
    420,
    Math.min(1.5, devicePixelRatio || 1),
    west,
    east,
    south,
    north,
  );
  mapView.series(
    new Float64Array(coastlines.map((v) => v ?? NaN)),
    "#a5b0b7",
    true,
    0,
  );
  const labeled = new Set();
  for (const row of located) {
    const a = p[row.transmitter],
      b = p[row.receiver],
      stale = !["current", "archived"].includes(row.freshness),
      color =
        stale || row.upper_mhz == null
          ? "#97a6ae"
          : mapView.scale_color(row.upper_mhz);
    mapView.series(
      new Float64Array([a.lon, a.lat, b.lon, b.lat]),
      color,
      true,
      3,
    );
    for (const [name, point] of [
      [row.transmitter, a],
      [row.receiver, b],
    ])
      if (!labeled.has(name)) {
        mapView.label(point.lon, point.lat, name, "#263b48");
        labeled.add(name);
      }
    mapView.label(
      (a.lon + b.lon) / 2,
      (a.lat + b.lat) / 2,
      `${stale ? (row.freshness === "stale" ? "Stale · " : "Unverified · ") : ""}${row.upper_mhz == null ? "No band" : `${row.upper_mhz.toFixed(2)} MHz${row.quality_flags?.includes("sampling-edge-limited") ? " · sampling edge" : row.bands.length > 1 ? " · separate bands" : ""}`}`,
      color,
    );
  }
  unbindMapColor = bindColorbarEditor(canvas, mapView, {
    automaticLimits,
    onChange: (low, high, automatic) => {
      mapColorLimits = automatic ? null : [low, high];
      drawMap(records, gen).catch(e => { $("#status").textContent = `Map unavailable: ${e.message || e}`; });
    },
  });
}

function renderNetworkTable(records) {
  const body = $("#network-path-rows");
  body.replaceChildren();
  for (const row of records) {
    const tr = document.createElement("tr"), band = row.lower_mhz == null || row.upper_mhz == null
      ? "No supported echo band" : `${row.lower_mhz.toFixed(2)}–${row.upper_mhz.toFixed(2)} MHz`;
    for (const value of [
      `${row.transmitter} → ${row.receiver}`,
      band,
      row.time ? new Date(row.time * 1000).toISOString().slice(11, 19) : "—",
      row.range_gate_km ? `${Math.round(row.range_gate_km[0])}–${Math.round(row.range_gate_km[1])} km` : "—",
      pathFreshness(row),
    ]) {
      const td = document.createElement("td"); td.textContent = value; tr.append(td);
    }
    body.append(tr);
  }
  if (!records.length) {
    const tr = document.createElement("tr"), td = document.createElement("td");
    td.colSpan = 5; td.textContent = "No network propagation estimates are available for this UTC day."; tr.append(td); body.append(tr);
  }
}

async function drawNetworkMap(records, positions, gen) {
  const canvas = $("#network-path-map"), located = records.filter(r => positions[r.transmitter] && positions[r.receiver]);
  unbindNetworkColor?.(); networkMapView?.free(); networkMapView = null;
  if (!located.length) { canvas.width = 1; canvas.height = 1; return; }
  coastlines ??= await json("assets/coastlines.json");
  if (gen !== networkGeneration || activeView !== "propagation") return;
  networkMapView = new SeriesView(canvas);
  const current = located.filter(row => ["current", "archived"].includes(row.freshness) && row.upper_mhz != null);
  const automaticLimits = () => [0, Math.max(25, ...current.map(row => Math.ceil(row.upper_mhz / 5) * 5))];
  networkMapView.set_color_scale(...(networkColorLimits || automaticLimits()), "viridis", "Maximum usable frequency (MHz)");
  const bounds = canvas.getBoundingClientRect();
  networkMapView.begin_map(bounds.width, Math.max(420, Math.min(720, innerHeight - 220)), Math.min(1.5, devicePixelRatio || 1), -180, 180, -62, 85);
  networkMapView.series(new Float64Array(coastlines.map(v => v ?? NaN)), "#a5b0b7", true, 0);
  const labeled = new Set();
  for (const row of located) {
    const a = positions[row.transmitter], b = positions[row.receiver], stale = !["current", "archived"].includes(row.freshness);
    const valid = !stale && row.lower_mhz != null && row.upper_mhz != null;
    const color = valid ? networkMapView.scale_color(row.upper_mhz) : "#97a6ae";
    networkMapView.series(new Float64Array([a.lon, a.lat, b.lon, b.lat]), color, true, valid ? 3 : 1.5);
    for (const [name, point] of [[row.transmitter, a], [row.receiver, b]]) if (!labeled.has(name)) {
      networkMapView.label(point.lon, point.lat, name, "#263b48"); labeled.add(name);
    }
    const band = valid ? `${row.lower_mhz.toFixed(1)}–${row.upper_mhz.toFixed(1)} MHz` : stale ? "stale" : "no band";
    networkMapView.label((a.lon + b.lon) / 2, (a.lat + b.lat) / 2, band, color);
  }
  unbindNetworkColor = bindColorbarEditor(canvas, networkMapView, {
    automaticLimits,
    onChange: (low, high, automatic) => {
      networkColorLimits = automatic ? null : [low, high];
      drawNetworkMap(records, positions, gen).catch(e => { $("#network-status").textContent = `Map unavailable: ${e.message || e}`; });
    },
  });
}

async function loadNetworkPropagation() {
  const gen = ++networkGeneration, status = $("#network-status");
  status.textContent = "Loading all receiver paths…";
  try {
    const responses = await Promise.all(networkReceivers.map(async receiver => {
      try { return await json(`api/frequency-limits?day=${day.value}&station=${receiver}`); }
      catch { return null; }
    }));
    if (gen !== networkGeneration || activeView !== "propagation") return;
    const positions = Object.assign({}, ...responses.filter(Boolean).map(r => r.positions || {}));
    const paths = new Map();
    for (const response of responses.filter(Boolean)) for (const row of response.paths || []) {
      const id = `${row.transmitter}\u0000${row.receiver}`, previous = paths.get(id);
      if (!previous || (row.time || 0) > (previous.time || 0)) paths.set(id, row);
    }
    const records = [...paths.values()].sort((a, b) => a.receiver.localeCompare(b.receiver) || a.transmitter.localeCompare(b.transmitter));
    renderNetworkTable(records);
    await drawNetworkMap(records, positions, gen);
    const supported = records.filter(r => r.lower_mhz != null && r.upper_mhz != null && ["current", "archived"].includes(r.freshness)).length;
    status.textContent = `${records.length} monitored links · ${supported} with a supported band · ${day.value} UTC`;
  } catch (error) {
    if (gen === networkGeneration) status.textContent = `Network propagation unavailable: ${error.message || error}`;
  }
}
async function loadDay(reset = false) {
  if (reset) {
    faults.clear(); requestFailures = 0;
    $("#status").textContent = "Loading receiver arrays";
    generation++;
    queue = [];
    for (const c of [...cards.values()]) dispose(c);
    root.replaceChildren();
    groups.clear();
    support.clear();
    products = [];
    cache.clear();
    root.dataset.cachedArrayBytes = "0";
    $("#empty").hidden = true;
    $("#history-wrap").hidden = true;
    historyView?.free();
    historyView = null;
    selectedPath = null;
    limitData = null;
    unbindMapColor?.();
    mapView?.free();
    mapView = null;
    const canvas = $("#path-map");
    canvas.width = 1;
    canvas.height = 1;
    $("#path-rows").replaceChildren();
    renderHealth(null);
  }
  const gen = generation;
  clearTimeout(timer);
  loading = true;
  if (!products.length) {
    $("#progress-wrap").hidden = false;
    $("#progress").removeAttribute("value");
    $("#progress-label").textContent = "Loading receiver arrays";
  }
  $("#context").textContent = "UTC";
  try {
    const p = await json(`api/day?day=${day.value}&station=${station.value}`);
    if (gen !== generation) return;
    requestFailures = 0;
    faults.delete("connection"); faults.delete("publication");
    loading = p.state === "loading";
    bands = p.bands || [];
    if (p.state === "ready") {
      products = p.products;
      updateStationTab(station.value, p.health);
      renderHealth(p.health);
      frequency();
    } else if (p.state === "unavailable") {
      if (station.value !== "DMB" && day.value === new Date().toISOString().slice(0, 10))
        faults.set("publication", "No receiver-prepared arrays have been published for today.");
      products = [];
      $("#empty").textContent = p.message;
      renderHealth(null);
      limitData = null;
      mapView?.free();
      mapView = null;
      $("#path-map").width = 1;
      $("#path-rows").replaceChildren();
    }
    apply();
    updateAttention();
    $("#progress-wrap").hidden = !loading;
    $("#progress-label").textContent = "Loading receiver arrays";
    $("#status").textContent =
      p.state === "ready"
        ? `${products.length} plots · Updated ${stamp(p.generated_unix)}`
        : p.message || "Loading receiver arrays";
    timer = setTimeout(() => loadDay(), loading ? 1500 : 60000);
  } catch (e) {
    if (gen !== generation) return;
    loading = false;
    if (++requestFailures >= 3) faults.set("connection", "The data service is repeatedly failing to respond. Automatic retries continue.");
    updateAttention();
    $("#status").textContent =
      "Connection interrupted; retrying automatically.";
    timer = setTimeout(() => loadDay(), 5000);
  }
}
for (const name of ["help", "settings", "attention"]) {
  $(`#${name}-open`).onclick = () => {
    $(`#${name}`).showModal();
    if (name === "settings") settingsList();
  };
  $(`#${name} [data-close]`).onclick = () => $(`#${name}`).close();
}
$("#attention-retry").onclick = () => {
  if (activeView === "propagation") loadNetworkPropagation(); else loadDay();
  for (const c of cards.values()) if (c.failures && c.visible) { c.loaded = null; enqueue(c); }
};
$("#attention-health").onclick = () => {
  $("#attention").close();
  const health = $("#health-data").closest("details");
  health.open = true;
  health.scrollIntoView({ behavior: "smooth", block: "start" });
};
station.onchange = () => {
  activateView(station.value, true);
};
day.onchange = () => {
  if (activeView === "propagation") loadNetworkPropagation(); else loadDay(true);
  refreshStationHealth();
};
kind.onchange = apply;
$("#restore").onclick = () => {
  hidden.clear();
  order = [];
  apply();
  save();
};
const columns = $("#columns");
columns.value = localStorage.getItem("iono2-columns") || "1";
const layout = () => {
  root.style.gridTemplateColumns =
    columns.value === "auto"
      ? "repeat(auto-fit,minmax(min(100%,570px),1fr))"
      : `repeat(${columns.value},minmax(0,1fr))`;
  localStorage.setItem("iono2-columns", columns.value);
};
columns.onchange = layout;
layout();
document.addEventListener("visibilitychange", () => {
  if (document.hidden) {
    clearTimeout(timer);
    for (const c of cards.values()) release(c);
  } else {
    for (const c of cards.values()) if (c.visible) enqueue(c);
    if (activeView === "propagation") loadNetworkPropagation(); else loadDay();
    refreshStationHealth();
  }
});
setInterval(() => ($("#clock").textContent = stamp(Date.now() / 1000)), 1000);
setInterval(refreshStationHealth, 60000);
installStationTabs();
showView(activeView);
try {
  const days = await json("api/archive-days");
  for (const d of days) {
    const o = document.createElement("option");
    o.value = d;
    o.textContent = d;
    day.append(o);
  }
  if (days.length) {
    await refreshStationHealth();
    if (activeView === "propagation") await loadNetworkPropagation();
    else { station.value = activeView; await loadDay(true); }
  }
  else {
    $("#empty").hidden = false;
    $("#empty").textContent = "Receiver arrays are being prepared.";
    setTimeout(() => location.reload(), 10000);
  }
} catch {
  faults.set("catalogue", "The archive catalogue is unavailable. Automatic retries continue.");
  updateAttention();
  $("#status").textContent = "Connecting…";
  setTimeout(() => location.reload(), 5000);
}
