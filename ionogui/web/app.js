// Dashboard orchestration: fetch prepared arrays, schedule visible plots and
// bind controls. Station DSP/RTI preparation lives in rust/. Companion modules
// own array bounds, geometry/hit testing and popup lifetimes. CPU usage,
// publication freshness and supported echo bands are separate status signals.
import init, {
  HeatmapView,
  PlotViewport,
  SeriesView,
} from "./pkg/1d7662e543d9238ecc0a0bfcc15983bf75323542/rustplotlib_web.js";
import { bindPlotNavigation, bindColorbarEditor } from "./navigation.js?v=20261107-touch";
import { ArrayCache, readArray, MAX_CELLS } from "./arrays.js";
import { receiverIssues, processingIssues, publicationIssues, cpuTelemetry } from "./health.js?v=20261107-publication-2";
import { panelKey as key, groupProducts, soundingColumns } from "./groups.js?v=20261102-2";
import { figureConfig, panelControls } from "./presentation.js?v=20261101";
import { receiverViews, viewFromUrl, urlForView, mapRegion, urlForMapRegion, cleanPlotOrder, readPlotOrderCookie, plotOrderCookie, mergePlotOrder } from "./preferences.js?v=20261104-krea";
import {bindMapLinks, fitMapRegion, pathSourceLabel, greatCircle, plotDisplayDefaults, ionogramLabel} from "./map-links.js?v=20261106-rothr-candidates";
import {ionogramPopup} from "./ionogram-popup.js?v=20261106-rothr-candidates";
import {linkHistoryPopup} from "./link-history-popup.js?v=20261106-rothr-candidates";
import {observationTime} from "./observation-time.js?v=20261103-2";
import {latestSpectrumLine} from "./spectrum-line.js?v=20261104";
import {PropagationGlobe} from "./propagation-globe.js?v=20261108";
import {globeProjection} from "./globe-camera.js";
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
const observation = observationTime({day, button:$("#day-open"), calendar:$("#day-calendar"), time:$("#utc-time"), nowButton:$("#time-now"), onChange:selectionChanged, getDays:() => json("api/archive-days")});
let selectionDay = day.value;
const saved = (name, fallback) => {
  try {
    return JSON.parse(localStorage.getItem(name)) ?? fallback;
  } catch {
    return fallback;
  }
};
let order = readPlotOrderCookie(document.cookie) ?? cleanPlotOrder(saved("iono2-order", [])),
  hidden = new Set(saved("iono2-hidden", [])),
  products = [],
  bands = [],
  generation = 0,
  timer,
  queue = [],
  busy = 0,
  loading = false;
let figure = figureConfig(saved("iono2-figure", {}));
const plotFont = () => figure.fontSize ?? (innerWidth < 650 ? 16 : 18);
let limitData = null,
  mapView = null,
  historyView = null,
  selectedPath = null;
// Dense receiver layouts can expose four columns of three stacked panels.
// Stay below the usual 16-context browser ceiling, including one popup;
// offscreen plots still dispose both canvas buffers and the cache stays bounded.
const MAX_VIEWS = 12;
const faults = new Map();
let currentHealth = null, requestFailures = 0;
const networkReceivers = receiverViews;
const tabHost = $("#station-tabs");
let activeView = viewFromUrl(location.href, localStorage.getItem("iono2-active-view") || localStorage.getItem("iono2-station"));
function saveViewUrl(name, mode = "push") {
  const href = urlForView(location.href, name);
  if (href !== location.href) history[`${mode}State`](null, "", href);
}
saveViewUrl(activeView, "replace");
document.cookie = plotOrderCookie(order, location.protocol === "https:");
localStorage.removeItem("iono2-order");
let networkMapView = null, unbindNetworkColor = null, networkColorLimits = null, networkGeneration = 0;
let networkNavigation, networkData, networkAbort, healthRequest;
let networkRegion = mapRegion(location.href);
let networkPresetPending = true;
let networkProjection = globeProjection(location.href), networkGlobe = null;
const networkViewport = new PlotViewport();
const healthByStation = new Map();

function updateStationTab(name, health) {
  if (!health) return;
  healthByStation.set(name, health);
  const button = tabHost.querySelector(`[data-view="${name}"]`), value = button?.querySelector("span");
  if (!button || !value) return;
  const {used,free,stale,age} = cpuTelemetry(health,name);
  if (stale) {
    value.textContent="Telemetry stale";button.dataset.health="bad";
    button.title=`${name}: last CPU report ${Math.round(age/60)} minutes ago`;
    return;
  }
  value.textContent = free == null ? "CPU unavailable" : `CPU ${used.toFixed(0)}% · ${free.toFixed(0)}% free`;
  const losses = processingIssues(health, new Date().toISOString().slice(0, 10));
  if (losses.length) value.textContent += " · missed ionogram data";
  button.dataset.health = losses.length || health.ok === false || used >= 92 ? "bad" : used >= 80 ? "warn" : "ok";
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
  if (!network && networkGlobe) { unbindNetworkLinks?.(); unbindNetworkLinks=null; networkGlobe.free(); networkGlobe=null; }
  document.body.classList.toggle("propagation-page", network);
  if (!network) { document.body.classList.remove("map-options-open"); $("#network-options").setAttribute("aria-expanded", "false"); }
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

function activateView(name, reload = false, historyMode = "push") {
  popup.close();
  networkAbort?.abort();
  document.querySelector(".panel-maximized")?.dispatchEvent(new Event("panel-restore"));
  if (activeView !== name) generation++;
  networkGeneration++;
  activeView = name;
  saveViewUrl(name, historyMode);
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
  refreshPanelLayout();
}

async function refreshStationHealth() {
  if (document.hidden) return;
  if (healthRequest) return healthRequest;
  healthRequest = json("api/health").then(response => {
    for (const row of response.stations || []) updateStationTab(row.station, row.health);
    updateAttention();
  }).catch(() => {}).finally(() => { healthRequest = null; });
  return healthRequest;
}
function updateAttention() {
  const issues = [...faults.values(), ...(activeView === "propagation" ? [] : receiverIssues(currentHealth, station.value, day.value))];
  for (const [name, health] of healthByStation)
    if (name !== station.value || activeView === "propagation")
      issues.push(...receiverIssues(health, name, day.value).map(message => `${name}: ${message}`));
  for (const c of cards.values())
    if (c.alive && c.failures >= 3)
      issues.push(`${c.el.querySelector("h2").textContent}: repeated data-transfer failures.`);
  const unique = [...new Set(issues)], button = $("#attention-open"), list = $("#attention-list");
  button.hidden = unique.length === 0;
  button.textContent = `Needs attention (${unique.length})`;
  $("#network-attention").hidden = unique.length === 0;
  $("#network-attention").setAttribute("aria-label", `Needs attention (${unique.length})`);
  $("#attention-context").textContent = `${activeView === "propagation" ? "All receivers" : station.value} · ${day.value} UTC`;
  list.replaceChildren();
  for (const message of unique.length ? unique : ["No active faults."]) {
    const li = document.createElement("li"); li.textContent = message; list.append(li);
  }
}
setInterval(updateAttention, 15000);
station.value = activeView === "propagation" ? viewFromUrl(location.origin, localStorage.getItem("iono2-station")) : activeView;
kind.value = new URLSearchParams(location.search).get("product") || "";
const json = async (url, signal) => {
  const r = await fetch(url, { cache: "no-cache", signal });
  if (!r.ok) throw Error(`Service HTTP ${r.status}`);
  return r.json();
};
const stamp = (t) =>
  new Date(t * 1000).toISOString().replace("T", " ").slice(0, 19) + " UTC";
const ionograms = ionogramPopup({HeatmapView, PlotViewport, cache, json, fontSize:plotFont, stamp});
const histories = linkHistoryPopup({HeatmapView, PlotViewport, cache, json, fontSize:plotFont, stamp, openIonogram:(...args)=>ionograms.open(...args)});
const popup = {open:(...args)=>{ionograms.close();return histories.open(...args);},close:()=>{histories.close();ionograms.close();}};
const openIonogram = row => popup.open(row, day.value, observation.cutoff());
function pathButton(row) {
  const button = document.createElement("button");
  button.className = "path-link";
  button.textContent = `${pathSourceLabel(row)} → ${row.receiver}`;
  button.title = "Open 48-hour link plots";
  button.onclick = e => { e.stopPropagation(); openIonogram(row); };
  return button;
}
const title = (p) =>
  p.kind === "spectrum"
    ? p.quantity === "latest-spectrum" ? `${p.station} · Latest RF spectrum`
    : p.quantity === "background-spectrum"
      ? `${p.station} · RF interference background`
      : `${p.station} full-band spectrum`
    : p.kind === "rti"
      ? `${pathSourceLabel(p)} → ${p.station} · ${p.quantity === "snr" ? "SNR" : p.quantity === "peak-frequency" ? "Frequency" : "Frequency / range"}`
      : p.kind === "detections"
        ? `${p.station} · ${p.transmitter} sweeps · ${p.quantity === "peak-frequency" ? "Range" : "Frequency"}`
        : `${pathSourceLabel(p)}${p.station==='KREA'&&['Cyprus','Australia'].includes(p.transmitter)?' (assumed)':''} → ${p.station} · ${ionogramLabel(p.kind)}`;
const limits = (a) => {
  const d = (a.at(-1) - a[0]) / (a.length - 1);
  return [a[0] - d / 2, a.at(-1) + d / 2];
};
function saveOrder(keys) {
  order = mergePlotOrder(order, keys);
  document.cookie = plotOrderCookie(order, location.protocol === "https:");
}
function save() {
  saveOrder([...root.children].map((e) => e.dataset.key));
  localStorage.setItem("iono2-hidden", JSON.stringify([...hidden]));
}
// DOM removal alone does not release WASM/WebGL buffers or event bindings.
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
  c.controls?.dispose();
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
  if (document.hidden) return;
  // Two bounded progressive transfers keep a slow array from blocking every
  // other visible plot, without starting a burst for the whole receiver day.
  while (queue.length && busy < 2) {
    const c = queue.shift();
    if (!c.alive || !c.visible || c.busy) continue;
    if (
      !c.renderer &&
      [...cards.values()].filter((v) => v.renderer).length >= MAX_VIEWS
    ) {
      queue.unshift(c);
      return;
    }
    busy++;
    c.busy = true;
    render(c).finally(() => {
      c.busy = false;
      busy--;
      if (c.alive && c.visible && (!c.renderer || c.loaded !== c.current.id))
        queue.push(c);
      setTimeout(drain, 0);
    });
  }
}
const observer = new IntersectionObserver(
  (entries) => {
    for (const e of entries) {
      const c = cards.get(e.target.dataset.key);
      if (!c) continue;
      c.visible = e.isIntersecting && !c.el.classList.contains("panel-minimized") &&
        (!document.body.classList.contains("has-maximized-panel") || c.el.classList.contains("panel-maximized"));
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
  c.renderer.set_font_size(plotFont());
  c.renderer.set_markers(false);
  c.renderer.set_palette(p.quantity === "ionogram" ? "greys" : p.palette);
  c.renderer.set_x_labels_visible(c.bottom || c.el.classList.contains("panel-maximized"));
  c.renderer.set_reduction(
    ["snr", "spectrum", "background-spectrum"].includes(p.quantity) ? "max" : "nearest",
  );
  const caption = p.xlabel.includes("UTC")
    ? `Time (UTC), ${new Date(day.value + "T12:00:00Z").toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric", timeZone: "UTC" })}`
    : p.xlabel;
  c.renderer.set_labels(
    caption,
    p.quantity === 'background-spectrum' ? 'Frequency (MHz)' : p.ylabel,
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
function initializeSpectrumLine(c) {
  const axes=c.el.querySelector('.axes');
  c.el.querySelector('.field').hidden=true;
  axes.style.pointerEvents='auto';
  c.renderer=new SeriesView(axes);
  c.viewport=new PlotViewport();
  if(c.savedView)c.viewport.set_bounds(...c.savedView);
  c.renderer.set_numeric_labels('Frequency (MHz)','Power (dB, uncalibrated)');
  c.redraw=()=>{
    if(!c.renderer||!c.line)return;
    const r=c.el.querySelector('.plot').getBoundingClientRect(), line=c.line;
    c.renderer.set_font_size(plotFont());
    c.renderer.begin(Math.max(270,r.width),r.height,Math.min(1.5,devicePixelRatio||1),0,25,line.low,line.high,'','');
    c.renderer.set_view(...c.viewport.bounds());
    c.renderer.series(line.xy,'#176fa3',true,0);
  };
  c.layout=c.redraw;
  c.resize=new ResizeObserver(c.redraw);
  c.resize.observe(c.el.querySelector('.plot'));
  c.unbind=bindPlotNavigation(axes,c.viewport,c.redraw,()=>{
    const [x,y,width,height]=c.renderer.plot_rect();return {x,y,width,height};
  });
  axes.ondblclick=()=>{c.viewport.reset();c.redraw();};
}
async function renderSpectrumLine(c) {
  const p=c.current,gen=generation,abort=new AbortController();c.abort=abort;
  const bar=c.el.querySelector('.plot-progress'),label=bar.querySelector('span'),progress=bar.querySelector('progress');
  try {
    let values=cache.get(p.id);
    if(!values){
      bar.hidden=false;label.textContent='Loading';progress.value=0;
      const response=await fetch(`api/data/${p.id}`,{signal:abort.signal,cache:'force-cache'});
      values=await readArray(response,p.nx,p.ny,{signal:abort.signal,onProgress:f=>{progress.value=f;label.textContent=`Loading ${Math.round(f*100)}%`;}});
      cache.set(p.id,values);
    }
    if(abort.signal.aborted||!c.alive||gen!==generation)return;
    c.line=latestSpectrumLine(p,values,observation.cutoff());
    if(!c.renderer)initializeSpectrumLine(c);
    c.redraw();c.loaded=p.id;c.failures=0;bar.hidden=true;
    c.group.coverage.textContent=c.line?`${stamp(c.line.start)} · 60 s mean`:'No complete RF observation before the selected UTC time.';
    updateAttention();
  }catch(e){
    if(e.name!=='AbortError'){
      c.failures=(c.failures||0)+1;c.loaded=p.id;label.textContent='Connection interrupted · retrying';bar.hidden=false;progress.removeAttribute('value');updateAttention();
      setTimeout(()=>{if(c.alive&&c.visible){c.loaded=null;enqueue(c);}},Math.min(30000,2000*c.failures));
    }
  }
}
async function render(c) {
  if(c.current.quantity==='latest-spectrum')return renderSpectrumLine(c);
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
  const toolbar = document.createElement("div"), label = document.createElement("span");
  toolbar.className = "panel-toolbar";
  label.className = "panel-name";
  label.textContent = p.quantity === "snr" ? "SNR" : p.quantity === "peak-frequency" ? "Frequency" : p.quantity === "range-frequency" ? "Frequency / range" : "";
  toolbar.append(label);
  c.panelName = label;
  el.prepend(toolbar);
  c.controls = panelControls(el, toolbar, () => title(c.current), refreshPanelLayout);
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
  if(p.quantity==='latest-spectrum'){
    const line=g.members[0]?.line;
    g.coverage.textContent=line?`${stamp(line.start)} · 60 s mean`:'';
    return;
  }
  if(p.kind==='prn') {
    const values=cache.get(p.id);
    g.coverage.textContent=values && !values.some(Number.isFinite)
      ? `${stamp(p.timestamp)} · No PRN echoes pass the thresholds`
      : stamp(p.timestamp);
    return;
  }
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
      (p) => p.kind !== "link-history" && (!kind.value || p.kind === kind.value) && !hidden.has(key(p)),
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
      ? `${pathSourceLabel(description)} → ${description.station} · RTI`
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
      c.controls.update();
      if ((changed || p.quantity === 'latest-spectrum') && c.visible) enqueue(c);
    }
    if (root.children[i] !== g.el) root.insertBefore(g.el, root.children[i] || null);
    updateCoverage(g);
  }
  $("#empty").hidden = list.length > 0 || loading;
  refreshPanelLayout();
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
    saveOrder([...orderHost.children].map((row) => row.dataset.key));
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
    ["Ionogram processing (last 15 min)", h.processing?.workers?.length
      ? processingIssues(h, new Date().toISOString().slice(0, 10)).join(" ") || "No scheduler losses reported."
      : "Deadline monitoring not yet reported."],
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
      `api/frequency-limits?day=${day.value}&station=${station.value}${observation.query()}`,
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
    if ([...latest.keys()].some(tx => tx.startsWith("ROTHR"))) latest.delete("US");
    const body = $("#path-rows");
    body.replaceChildren();
    for (const r of latest.values()) {
      const tr = document.createElement("tr");
      for (const value of [
        `${pathSourceLabel(r)} → ${r.receiver}`,
        r.association_ambiguous ? "Unresolved ROTHR site" : r.quantitative_eligible === false ? (r.measurement_quality === "commissioning" ? "Commissioning" : "Excluded: cross-modulation") : r.lower_mhz == null
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
          r.measurement_quality === "limited-reliability" ? "Limited reliability" : null,
        ]
          .filter(Boolean)
          .join(" · "),
      ]) {
        const td = document.createElement("td");
        td.textContent = value;
        tr.append(td);
      }
      tr.tabIndex = 0;
      tr.firstElementChild.replaceChildren(pathButton(r));
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
const mapStatusKey = [
  ["#75828b", "No band"],
  ["#b6bec4", "Stale / unverified"],
  ["#8d8a9b", "Excluded: cross-modulation"],
];
function mapPathStyle(row, view) {
  if (row.measurement_quality === "commissioning") return {valid:false,color:mapStatusKey[1][0]};
  if (row.quantitative_eligible === false) return {valid:false,color:"#8d8a9b"};
  if (!["current", "archived"].includes(row.freshness))
    return {valid:false, color:mapStatusKey[1][0]};
  if (!Number.isFinite(row.lower_mhz) || !Number.isFinite(row.upper_mhz))
    return {valid:false, color:mapStatusKey[0][0]};
  return {valid:true, color:view.scale_color(row.upper_mhz)};
}
let mapColorLimits = null, unbindMapColor;
let mapNavigation, unbindMapLinks, unbindNetworkLinks, mapRecords = [];
const mapViewport = new PlotViewport();
function drawHistory(tx) {
  if (!limitData) return;
  selectedPath = tx;
  $("#history-wrap").hidden = false;
  if ($("#history-wrap").classList.contains("panel-minimized")) return;
  $("#history-title").textContent =
    `${tx} → ${station.value} · Detected echo band (MHz)`;
  const canvas = $("#frequency-history"),
    r = canvas.getBoundingClientRect();
  historyView?.free();
  historyView = new SeriesView(canvas);
  historyView.set_font_size(plotFont());
  historyView.set_color_key(echoLimits.map(v => v[2]), echoLimits.map(v => v[1]));
  const start = Date.parse(day.value + "T00:00:00Z") / 1000;
  historyView.begin(
    r.width,
    r.height,
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
let coastlines, coastlinesRequest;
async function loadCoastlines() {
  if (!coastlinesRequest) coastlinesRequest = json("assets/coastlines.json")
    .then(data => { coastlines = data; }).catch(error => { coastlinesRequest = null; throw error; });
  return coastlinesRequest;
}
async function drawMap(records, gen = generation) {
  if ($("#map-panel").classList.contains("panel-minimized")) return;
  const canvas = $("#path-map");
  const p = limitData?.positions || {},
    located = records.filter((r) => p[r.transmitter] && p[r.receiver]);
  canvas.hidden = !located.length;
  if (!located.length) {
    unbindMapLinks?.();
    mapNavigation?.(); mapNavigation = null;
    unbindMapColor?.();
    mapView?.free();
    mapView = null;
    canvas.width = 1;
    canvas.height = 1;
    return;
  }
  if (!coastlines) await loadCoastlines();
  if (gen !== generation) return;
  const coords = located.flatMap((r) => [p[r.transmitter], p[r.receiver]]),
    west = Math.min(...coords.map((p) => p.lon)) - 8,
    east = Math.max(...coords.map((p) => p.lon)) + 8,
    south = Math.min(...coords.map((p) => p.lat)) - 5,
    north = Math.min(85, Math.max(...coords.map((p) => p.lat)) + 5);
  mapRecords = records;
  if (!mapView) mapView = new SeriesView(canvas);
  mapView.set_font_size(plotFont());
  mapView.set_color_key(mapStatusKey.map(k => k[0]), mapStatusKey.map(k => k[1]));
  const automaticLimits = () => [0, Math.max(25, ...located
    .filter(row => ["current", "archived"].includes(row.freshness))
    .map(row => Math.ceil((row.upper_mhz || 0) / 5) * 5))];
  mapView.set_color_scale(...(mapColorLimits || automaticLimits()), "turbo", "Upper usable frequency (MHz)");
  const r = canvas.getBoundingClientRect();
  mapView.begin_map(
    r.width,
    r.height,
    Math.min(1.5, devicePixelRatio || 1),
    west,
    east,
    south,
    north,
  );
  mapView.set_view(...mapViewport.bounds());
  mapView.series(
    new Float64Array(coastlines.map((v) => v ?? NaN)),
    "#a5b0b7",
    true,
    0,
  );
  const stations = new Map();
  const links = [];
  for (const row of located) {
    const a = p[row.transmitter],
      b = p[row.receiver],
      {color} = mapPathStyle(row, mapView);
    const path=greatCircle(a,b);
    links.push({row, pixels:mapView.project_points(path)});
    mapView.series(
      path,
      color,
      true,
      0,
    );
    for (const [name, point] of [
      [row.transmitter, a],
      [row.receiver, b],
    ])
      if (!stations.has(name)) {
        const coords = new Float64Array([point.lon, point.lat]);
        stations.set(name, {coords, label:name === row.transmitter ? pathSourceLabel(row) : name,
          pixels:mapView.project_points(coords)});
      }
  }
  for (const station of stations.values()) mapView.series(station.coords, "#263b48", false, 3);
  if (!mapNavigation) mapNavigation = bindPlotNavigation(canvas, mapViewport, redrawMap, () => {
    const [x,y,width,height] = mapView.plot_rect(); return {x,y,width,height};
  });
  unbindMapLinks?.();
  unbindMapLinks = bindMapLinks(canvas, mapView, links, openIonogram, [...stations.values()]);
  unbindMapColor?.();
  unbindMapColor = bindColorbarEditor(canvas, mapView, {
    automaticLimits,
    onChange: (low, high, automatic) => {
      mapColorLimits = automatic ? null : [low, high];
      redrawMap();
    },
  });
}

async function drawNetworkMap(records, positions, gen) {
  networkData = {records, positions, gen};
  if ($("#network-panel").classList.contains("panel-minimized")) return;
  const canvas = $("#network-path-map"), located = records.filter(r => positions[r.transmitter] && positions[r.receiver]);
  if (!located.length) { unbindNetworkLinks?.(); $("#network-globe").hidden = true; canvas.width = 1; canvas.height = 1; return; }
  if (!coastlines) await loadCoastlines();
  if (gen !== networkGeneration || activeView !== "propagation") return;
  if (!networkMapView) networkMapView = new SeriesView(canvas);
  networkMapView.set_font_size(plotFont());
  const phoneMap = typeof innerWidth !== "undefined" && (innerWidth <= 650 || innerHeight <= 500);
  networkMapView.set_color_key(mapStatusKey.map(k => k[0]), phoneMap ? ["No band", "Stale", "Excluded"] : mapStatusKey.map(k => k[1]));
  const current = located.filter(row => ["current", "archived"].includes(row.freshness) && row.upper_mhz != null);
  const automaticLimits = () => [0, Math.max(25, ...current.map(row => Math.ceil(row.upper_mhz / 5) * 5))];
  networkMapView.set_color_scale(...(networkColorLimits || automaticLimits()), "turbo", phoneMap ? "Upper frequency (MHz)" : "Maximum usable frequency (MHz)");
  const bounds = canvas.getBoundingClientRect();
  // Keep world bounds for navigation. Norway is only an initial zoom, with
  // margin below Juliusruh and above KHO so endpoints and labels remain visible.
  networkMapView.begin_map(bounds.width, bounds.height, Math.min(1.5, devicePixelRatio || 1), -180,180,-62,85);
  const applyPreset = networkPresetPending;
  if (networkPresetPending) {
    networkViewport.reset();
    if (networkRegion === "norway") fitMapRegion(networkMapView, networkViewport, [0,35,52,81]);
    networkPresetPending = false;
  }
  $("#network-region").value = networkRegion;
  for (const name of ["norway", "world"]) $("#network-" + name).setAttribute("aria-pressed", String(networkRegion === name));
  networkMapView.set_view(...networkViewport.bounds());
  $("#network-projection").value = networkProjection;
  $("#network-navigation-hint").textContent = networkProjection === "3d" ? "Drag to rotate · Ctrl/Cmd + wheel to zoom" : "Drag to pan · Ctrl/Cmd + wheel to zoom";
  const globeCanvas = $("#network-globe");
  globeCanvas.hidden = networkProjection !== "3d";
  if (networkProjection === "3d") {
    // Keep the exact Rustplotlib legend/colorbar; the reused globe occupies
    // only the map below it. Both views use the same link data and callbacks.
    const panel = $("#network-panel").getBoundingClientRect(), cut = Math.max(0,networkMapView.plot_rect()[1]-8);
    Object.assign(globeCanvas.style,{left:`${bounds.left-panel.left}px`,top:`${bounds.top-panel.top+cut}px`,width:`${bounds.width}px`,height:`${Math.max(1,bounds.height-cut)}px`});
    if (!networkGlobe) { networkGlobe = new PropagationGlobe(globeCanvas,redrawNetworkMap,()=>{networkPresetPending=true;redrawNetworkMap();}); networkGlobe.camera.reset(networkRegion); }
    if (applyPreset) networkGlobe.camera.reset(networkRegion);
    networkGlobe.setCoastline(coastlines);
    const entries=[], links=[], stations=new Map();
    for(const row of located){
      const a=positions[row.transmitter], b=positions[row.receiver], path=greatCircle(a,b), {color}=mapPathStyle(row,networkMapView);
      entries.push({coords:path,color});links.push({row,pixels:networkGlobe.project_points(path)});
      for(const [name,point] of [[row.transmitter,a],[row.receiver,b]])if(!stations.has(name)){
        const coords=new Float64Array([point.lon,point.lat]);
        stations.set(name,{coords,label:name===row.transmitter?pathSourceLabel(row):name,pixels:networkGlobe.project_points(coords)});
      }
    }
    for(const station of stations.values())entries.push({coords:station.coords,color:"#263b48",points:true});
    networkGlobe.setEntries(entries);
    unbindNetworkLinks?.();unbindNetworkLinks=bindMapLinks(globeCanvas,networkGlobe,links,openIonogram,[...stations.values()]);
    unbindNetworkColor?.();unbindNetworkColor=bindColorbarEditor(canvas,networkMapView,{automaticLimits,onChange:(low,high,automatic)=>{networkColorLimits=automatic?null:[low,high];redrawNetworkMap();}});
    return;
  }
  networkMapView.series(new Float64Array(coastlines.map(v => v ?? NaN)), "#a5b0b7", true, 0);
  const stations = new Map();
  const links = [];
  for (const row of located) {
    const a = positions[row.transmitter], b = positions[row.receiver], {color} = mapPathStyle(row, networkMapView);
    const path=greatCircle(a,b);
    links.push({row, pixels:networkMapView.project_points(path)});
    networkMapView.series(path, color, true, 0);
    for (const [name, point] of [[row.transmitter, a], [row.receiver, b]]) if (!stations.has(name)) {
      const coords = new Float64Array([point.lon, point.lat]);
      stations.set(name, {coords, label:name === row.transmitter ? pathSourceLabel(row) : name,
        pixels:networkMapView.project_points(coords)});
    }
  }
  for (const station of stations.values()) networkMapView.series(station.coords, "#263b48", false, 3);
  if (!networkNavigation) networkNavigation = bindPlotNavigation(canvas, networkViewport, redrawNetworkMap, () => {
    const [x,y,width,height] = networkMapView.plot_rect(); return {x,y,width,height};
  });
  unbindNetworkLinks?.();
  unbindNetworkLinks = bindMapLinks(canvas, networkMapView, links, openIonogram, [...stations.values()]);
  unbindNetworkColor?.();
  unbindNetworkColor = bindColorbarEditor(canvas, networkMapView, {
    automaticLimits,
    onChange: (low, high, automatic) => {
      networkColorLimits = automatic ? null : [low, high];
      redrawNetworkMap();
    },
  });
}

// Generation/abort guards stop late replies for a previous date/view from
// repainting the current map. Receivers can complete independently.
async function loadNetworkPropagation() {
  clearTimeout(timer);
  networkAbort?.abort();
  const abort = new AbortController();
  networkAbort = abort;
  const gen = ++networkGeneration, status = $("#network-status");
  const responses = [], positions = {}, paths = new Map();
  let completed = 0;
  const publish = async () => {
    if (gen !== networkGeneration || activeView !== "propagation") return;
    const records = [...paths.values()].sort((a, b) => a.receiver.localeCompare(b.receiver) || a.transmitter.localeCompare(b.transmitter));
    await drawNetworkMap(records, positions, gen);
    if (gen !== networkGeneration) return;
    const supported = records.filter(r => r.lower_mhz != null && r.upper_mhz != null && ["current", "archived"].includes(r.freshness)).length;
    status.textContent = `${records.length} monitored links · ${supported} with a supported band · ${observation.label()}` +
      (completed < networkReceivers.length ? ` · Loading ${completed}/${networkReceivers.length} receivers` : responses.length < completed ? ` · ${completed - responses.length} receivers unavailable` : "");
  };
  status.textContent = "Loading receiver paths…";
  try {
    await Promise.all(networkReceivers.map(async receiver => {
      let response;
      try { response = await json(`api/frequency-limits?day=${day.value}&station=${receiver}&summary=true${observation.query()}`, abort.signal); }
      catch (error) { if (error.name === "AbortError") return; }
      if (gen !== networkGeneration) return;
      completed++;
      if (response) {
        responses.push(response);
        Object.assign(positions, response.positions || {});
        for (const row of response.paths || []) {
          const id = `${row.transmitter}\u0000${row.receiver}`, previous = paths.get(id);
          if (!previous || (row.time || 0) > (previous.time || 0)) paths.set(id, row);
        }
      }
      await publish();
    }));
  } catch (error) {
    if (gen === networkGeneration) status.textContent = `Network propagation unavailable: ${error.message || error}`;
  } finally {
    if (gen === networkGeneration && !document.hidden) timer = setTimeout(loadNetworkPropagation, 60000);
  }
}
async function loadDay(reset = false) {
  if (reset) {
    popup.close();
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
    unbindMapLinks?.();
    mapNavigation?.(); mapNavigation = null;
    mapRecords = []; mapViewport.reset();
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
      const stale = publicationIssues(p, day.value);
      if (stale.length) faults.set("publication", stale[0]);
      // Daily grids keep their full time axis. Snapshot plots must not come
      // from after a fixed selection when older individual files are absent.
      const cutoff = observation.cutoff();
      products = p.products.filter(p => p.kind !== "link-history" && (cutoff == null || p.quantity !== "ionogram" || p.timestamp <= cutoff)).map(plotDisplayDefaults);
      const background=products.find(p=>p.station==='KREA'&&p.quantity==='background-spectrum');
      if(background)products.unshift({...background,quantity:'latest-spectrum'});
      updateStationTab(station.value, p.health);
      renderHealth(p.health);
      frequency();
    } else if (p.state === "unavailable") {
      if (day.value === new Date().toISOString().slice(0, 10))
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
function selectionChanged() {
  popup.close();
  const changedDay = selectionDay !== day.value;
  selectionDay = day.value;
  generation++;
  // Invalidate in-flight responses before an earlier timestamp can be drawn.
  networkGeneration++;
  networkAbort?.abort();
  networkData = null;
  unbindNetworkLinks?.();
  $("#network-path-map").width = 1;
  if (activeView === "propagation") loadNetworkPropagation();
  else {
    limitData = null;
    mapRecords = [];
    unbindMapLinks?.();
    $("#path-map").width = 1;
    const row = document.createElement("tr"), cell = document.createElement("td");
    cell.colSpan = 5; cell.textContent = "Loading path estimates…";
    row.append(cell); $("#path-rows").replaceChildren(row);
    const cutoff = observation.cutoff();
    products = products.filter(p => cutoff == null || p.quantity !== "ionogram" || p.timestamp <= cutoff);
    if (!changedDay) apply();
    loadDay(changedDay);
  }
  refreshStationHealth();
}
kind.onchange = apply;
$("#restore").onclick = () => {
  hidden.clear();
  order = order.filter(k => !k.startsWith(station.value + ":"));
  apply();
  save();
};
const columns = $("#columns");
// Replace the old one/two-column default with packing at the configured width.
// A new explicit layout choice still persists independently of figure sizing.
columns.value = localStorage.getItem("iono2-packed-columns") || "auto";
const layout = () => {
  root.style.gridTemplateColumns =
    columns.value === "auto"
      ? "repeat(auto-fit,minmax(min(100%,var(--figure-width,570px)),1fr))"
      : `repeat(${columns.value},minmax(0,1fr))`;
  localStorage.setItem("iono2-packed-columns", columns.value);
};
columns.onchange = layout;
window.addEventListener("popstate", () => {
  networkPresetPending = true;
  networkRegion = mapRegion(location.href); networkViewport.reset();
  networkProjection = globeProjection(location.href);
  activateView(viewFromUrl(location.href), true, "replace");
});
layout();
function redrawMap() {
  if (activeView !== "propagation" && limitData && mapRecords.length) drawMap(mapRecords).catch(e => {
    $("#status").textContent = `Map unavailable: ${e.message || e}`;
  });
}
function redrawNetworkMap() {
  if (activeView === "propagation" && networkData) {
    const {records,positions,gen} = networkData;
    drawNetworkMap(records,positions,gen).catch(e => { $("#network-status").textContent = `Map unavailable: ${e.message || e}`; });
  }
}
let layoutFrame;
function refreshPanelLayout() {
  cancelAnimationFrame(layoutFrame);
  layoutFrame = requestAnimationFrame(() => {
    const maximized = document.querySelector(".panel-maximized");
    for (const g of groups.values()) {
      const shown = g.members.filter(c => !c.el.classList.contains("panel-minimized"));
      for (const c of g.members) {
        c.panelName.textContent = maximized === c.el ? title(c.current) :
          c.current.quantity === "snr" ? "SNR" : c.current.quantity === "peak-frequency" ? "Frequency" : c.current.quantity === "range-frequency" ? "Frequency / range" : "";
        c.bottom = c === shown.at(-1);
        const r = c.el.getBoundingClientRect();
        c.visible = !c.el.classList.contains("panel-minimized") && (!maximized || maximized === c.el) && r.bottom > 0 && r.top < innerHeight;
        if (!c.visible) { if (c.renderer || c.abort) release(c); continue; }
        if (c.renderer) {
          c.renderer.set_font_size(plotFont());
          c.renderer.set_x_labels_visible(c.bottom || maximized === c.el);
          c.layout();
        } else enqueue(c);
      }
    }
    redrawMap();
    redrawNetworkMap();
    if (selectedPath && activeView !== "propagation") drawHistory(selectedPath);
  });
}
const sizeFields = {width:$("#figure-width"), height:$("#figure-height"), fontSize:$("#figure-font")};
function applyFigureConfig() {
  const style = document.documentElement.style;
  for (const name of ["width", "height"]) {
    if (figure[name] == null) style.removeProperty(`--figure-${name}`);
    else style.setProperty(`--figure-${name}`, `${figure[name]}px`);
  }
  for (const [name, input] of Object.entries(sizeFields)) input.value = figure[name] ?? "";
  refreshPanelLayout();
}
$("#figure-config").onsubmit = e => {
  e.preventDefault();
  figure = figureConfig(Object.fromEntries(Object.entries(sizeFields).map(([k,v]) => [k, v.value === "" ? null : v.valueAsNumber])));
  localStorage.setItem("iono2-figure", JSON.stringify(figure));
  applyFigureConfig();
};
$("#figure-defaults").onclick = () => {
  figure = figureConfig(); localStorage.removeItem("iono2-figure"); applyFigureConfig();
};
applyFigureConfig();
panelControls($("#map-panel"), $("#map-toolbar"), () => "propagation map", refreshPanelLayout).update();
panelControls($("#history-wrap"), $("#history-toolbar"), () => "echo-band history", refreshPanelLayout).update();
panelControls($("#network-panel"), $("#network-toolbar"), () => "full propagation map", refreshPanelLayout).update();
$("#network-options").onclick = () => {
  if ($("#network-panel").classList.contains("panel-maximized")) $("#network-panel").dispatchEvent(new Event("panel-restore"));
  if (document.fullscreenElement) document.exitFullscreen?.().catch(() => {});
  const open = document.body.classList.toggle("map-options-open");
  $("#network-options").setAttribute("aria-expanded", String(open));
  refreshPanelLayout();
};
$("#network-attention").onclick = () => $("#attention-open").click();
$("#network-toolbar").addEventListener("click", e => {
  const button = e.target.closest("button");
  if ((innerWidth > 650 && innerHeight > 500) || !button || !/^(Maximize|Restore) full propagation map$/.test(button.ariaLabel)) return;
  if ($("#network-panel").classList.contains("panel-maximized")) {
    document.body.classList.remove("map-options-open");
    $("#network-options").setAttribute("aria-expanded", "false");
    // Unsupported phone browsers retain the full-window CSS presentation.
    document.documentElement.requestFullscreen?.().catch(() => {});
  } else if (document.fullscreenElement) document.exitFullscreen?.().catch(() => {});
});
document.addEventListener("fullscreenchange", () => {
  if (!document.fullscreenElement) $("#network-panel").dispatchEvent(new Event("panel-restore"));
  refreshPanelLayout();
});
$("#network-region").onchange = e => $("#network-" + e.target.value).click();
$("#network-zoom-in").onclick = () => { if(networkProjection==='3d')networkGlobe?.camera.zoomBy(Math.SQRT2);else networkViewport.zoom(0.5,0.5,0.5); redrawNetworkMap(); };
$("#network-zoom-out").onclick = () => { if(networkProjection==='3d')networkGlobe?.camera.zoomBy(1/Math.SQRT2);else networkViewport.zoom(0.5,0.5,2); redrawNetworkMap(); };
$("#network-projection").onchange = e => {networkProjection=e.target.value;networkPresetPending=true;const u=new URL(location.href);u.searchParams.set('projection',networkProjection);history.pushState(null,'',u);redrawNetworkMap();};
$("#network-reset").onclick = () => { networkPresetPending = true; redrawNetworkMap(); };
for (const region of ["norway", "world"]) $("#network-" + region).onclick = () => {
  networkRegion = region;
  networkPresetPending = true;
  for (const name of ["norway", "world"]) $("#network-" + name).setAttribute("aria-pressed", String(networkRegion === name));
  networkViewport.reset();
  const href = urlForMapRegion(location.href, region);
  if (href !== location.href) history.pushState(null, "", href);
  redrawNetworkMap();
};
for (const name of ["norway", "world"]) $("#network-" + name).setAttribute("aria-pressed", String(networkRegion === name));
$("#map-zoom-in").onclick = () => { mapViewport.zoom(0.5,0.5,0.5); redrawMap(); };
$("#map-zoom-out").onclick = () => { mapViewport.zoom(0.5,0.5,2); redrawMap(); };
$("#map-reset").onclick = () => { mapViewport.reset(); redrawMap(); };
const diagnosticResize = new ResizeObserver(entries => {
  // Phone browser bars and the controls menu change canvas height. Keep the
  // current camera; only the region and reset buttons should reset navigation.
  refreshPanelLayout();
});
diagnosticResize.observe($("#path-map")); diagnosticResize.observe($("#frequency-history"));
diagnosticResize.observe($("#network-path-map"));
window.addEventListener("resize", refreshPanelLayout);
document.addEventListener("visibilitychange", () => {
  if (document.hidden) {
    popup.close();
    networkAbort?.abort();
    clearTimeout(timer);
    for (const c of cards.values()) release(c);
  } else {
    for (const c of cards.values()) if (c.visible) enqueue(c);
    if (activeView === "propagation") loadNetworkPropagation(); else loadDay();
    refreshStationHealth();
  }
});
setInterval(() => { $("#clock").textContent = stamp(Date.now() / 1000); observation.tick(); }, 1000);
setInterval(refreshStationHealth, 60000);
installStationTabs();
showView(activeView);
try {
  const days = await json("api/archive-days");
  observation.setDays(days);
  if (days.length) {
    refreshStationHealth();
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
