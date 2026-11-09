import {readArray, MAX_CELLS} from "./arrays.js";
import {bindPlotNavigation, bindColorbarEditor} from "./navigation.js";
import {latestIonogram, pathSourceLabel, detectedBandLabel, ionogramLabel} from "./map-links.js?v=20261106-millstone-quality";

// One floating plot; closing or selecting another path cancels and frees it.
export function ionogramPopup({HeatmapView, PlotViewport, cache, json, fontSize, stamp}) {
  let session;
  function close() {
    if (!session) return;
    const s = session;
    session = null;
    s.abort.abort();
    s.resize?.disconnect();
    s.unbind?.();
    s.uncolor?.();
    s.renderer?.free();
    s.viewport?.free();
    s.el.remove();
    s.previousFocus?.focus({preventScroll:true});
  }
  async function open(row, day, at = null, onBack = null) {
    close();
    const el = document.createElement("dialog");
    el.className = "ionogram-popup";
    el.setAttribute("aria-label", `${row.transmitter} to ${row.receiver} latest ionogram`);
    const head = document.createElement("div"), heading = document.createElement("h2"), button = document.createElement("button");
    head.className = "dialog-head";
    heading.textContent = `${pathSourceLabel(row)} → ${row.receiver} · Ionogram`;
    button.textContent = "×"; button.setAttribute("aria-label", "Close ionogram"); button.onclick = close;
    const actions = document.createElement("div");
    actions.className = "ionogram-actions";
    if (onBack) {
      const back = document.createElement("button");
      back.className = "back-to-rti";
      back.textContent = "← Back to RTI";
      back.onclick = () => { close(); onBack(); };
      actions.append(back);
    }
    actions.append(button);
    head.append(heading, actions);
    const status = document.createElement("p"), progress = document.createElement("progress"), plot = document.createElement("div");
    status.setAttribute("role", "status"); status.textContent = "Loading latest ionogram…";
    progress.max = 1; progress.setAttribute("aria-label", "Ionogram loading progress");
    plot.className = "plot";
    const field = document.createElement("canvas"), axes = document.createElement("canvas");
    field.className = "field"; axes.className = "axes";
    const band = document.createElement("p");
    band.className = "detected-band"; band.setAttribute("role", "status");
    band.textContent = row.assumed ? detectedBandLabel(row) : "Loading detected frequency range…";
    plot.append(field, axes); el.append(head, status, band, progress, plot);
    const s = {el, abort:new AbortController(), previousFocus:document.activeElement};
    session = s;
    document.body.append(el); el.show(); button.focus({preventScroll:true});
    el.addEventListener("keydown", e => { if (e.key === "Escape") { e.preventDefault(); close(); } });
    const current = () => session === s && !s.abort.signal.aborted;
    try {
      const catalogue = await json(`api/day?day=${encodeURIComponent(day)}&station=${encodeURIComponent(row.receiver)}`, s.abort.signal);
      if (!current()) return;
      const p = latestIonogram(catalogue.products || [], row, at);
      progress.hidden = true;
      if (!p) {
        status.textContent = row.assumed ? `Range-gated sweep association · ${stamp(row.time)}. No individual ionogram is retained for this assumed source.` : at == null ? `No ionogram available for this path on ${day} UTC.` : `No retained ionogram at or before ${stamp(at)}. Full-day RTIs remain available in the receiver tab.`;
        if (!row.assumed) band.hidden = true;
        plot.hidden = true; return;
      }
      heading.textContent = `${pathSourceLabel(row)} → ${row.receiver} · ${ionogramLabel(p.kind)}`;
      if (p.nx * p.ny > MAX_CELLS) throw Error("Array too large");
      status.textContent = stamp(p.timestamp);
      // Match the displayed sounding, not a newer map estimate. Keep this small
      // summary request independent so it never delays progressive image data.
      json(`api/frequency-limits?day=${encodeURIComponent(day)}&station=${encodeURIComponent(row.receiver)}&summary=true&at=${p.timestamp}`, s.abort.signal)
        .then(response => {
          if (!current()) return;
          const estimate = response.paths?.find(r => r.transmitter === row.transmitter && Math.abs(r.time - p.timestamp) < 2);
          band.textContent = estimate ? detectedBandLabel(estimate) : "Frequency estimate unavailable for this sounding";
        }).catch(() => { if (current()) band.textContent = "Frequency estimate unavailable for this sounding"; });
      s.renderer = new HeatmapView(field, axes);
      s.viewport = new PlotViewport();
      const renderer = s.renderer;
      renderer.set_font_size(fontSize());
      renderer.set_markers(false);
      renderer.set_palette("greys");
      renderer.set_reduction("max");
      renderer.set_labels(p.xlabel, p.ylabel, p.color_label, false);
      renderer.set_scale(p.low, p.high);
      const values = cache.get(p.id);
      renderer.set_rectilinear_data(new Float64Array(p.x), new Float64Array(p.y), values || new Float32Array(p.nx * p.ny).fill(NaN));
      const resize = () => {
        const r = plot.getBoundingClientRect();
        renderer.resize(r.width, r.height, Math.min(1.5, devicePixelRatio || 1));
        const [left,top,width,height] = renderer.plot_rect();
        Object.assign(field.style, {left:`${left}px`, top:`${top}px`, width:`${width}px`, height:`${height}px`});
      };
      resize(); s.resize = new ResizeObserver(resize); s.resize.observe(plot);
      const redraw = () => renderer.set_view(...s.viewport.bounds());
      s.unbind = bindPlotNavigation(field, s.viewport, redraw);
      field.ondblclick = () => { s.viewport.reset(); redraw(); };
      s.uncolor = bindColorbarEditor(axes, renderer, {automaticLimits:() => [p.low,p.high]});
      if (!values) {
        progress.hidden = false; progress.value = 0;
        const response = await fetch(`api/data/${p.id}`, {signal:s.abort.signal, cache:"force-cache"});
        const loaded = await readArray(response, p.nx, p.ny, {
          signal:s.abort.signal,
          onRows:(row, block) => { if (current()) renderer.update_rows(row, block); },
          onProgress:f => { if (current()) progress.value = f; },
        });
        if (!current()) return;
        cache.set(p.id, loaded); progress.hidden = true;
      }
    } catch (error) {
      if (current()) { status.textContent = `Ionogram unavailable: ${error.message || error}`; progress.hidden = true; plot.hidden = true; }
    }
  }
  return {open, close};
}
