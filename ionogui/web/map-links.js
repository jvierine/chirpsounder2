// Hit-test the exact CSS coordinates projected by Rustplotlib, including zoom.
// Fit a preset inside an already initialized world map. Projection stays in
// Rustplotlib; a square normalized viewport preserves the map's aspect ratio.
// The preset is a starting viewport, not a navigation boundary.
export function fitMapRegion(view, viewport, [west, east, south, north]) {
  const [left, top, width, height] = view.plot_rect();
  const [x0,y0,x1,y1] = view.project_points(new Float64Array([west,south,east,north]));
  const x = [(x0-left)/width, (x1-left)/width];
  const y = [1-(y0-top)/height, 1-(y1-top)/height];
  const size = Math.min(1, Math.max(0.001, x[1]-x[0], y[1]-y[0]));
  const low = pair => Math.max(0, Math.min(1-size, (pair[0]+pair[1]-size)/2));
  return viewport.set_bounds(low(x), low(y), low(x)+size, low(y)+size);
}
export function nearestLink(links, x, y, rect, tolerance = 10) {
  const [left, top, width, height] = rect;
  if (x < left || x > left + width || y < top || y > top + height) return null;
  let best = null, distance = tolerance;
  for (const link of links) {
    for (let i = 0; i + 3 < link.pixels.length; i += 2) {
    const ax=link.pixels[i], ay=link.pixels[i+1], bx=link.pixels[i+2], by=link.pixels[i+3];
    if (![ax, ay, bx, by].every(Number.isFinite)) continue;
    const dx = bx - ax, dy = by - ay;
    const t = Math.max(0, Math.min(1, ((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy || 1)));
    const d = Math.hypot(x - ax - t * dx, y - ay - t * dy);
    if (d < distance) { best = link.row; distance = d; }
    }
  }
  return best;
}
export function bindMapLinks(canvas, view, links, open, stations = []) {
  const tooltip = document.createElement("div");
  tooltip.setAttribute("role", "tooltip");
  Object.assign(tooltip.style, {position:"fixed", display:"none", pointerEvents:"none", zIndex:"2147483647",
    whiteSpace:"pre-line", maxWidth:"min(340px, calc(100vw - 16px))", padding:"8px 11px",
    border:"1px solid #aebfc6", borderRadius:"6px", background:"#fff", color:"#263b48",
    boxShadow:"0 2px 8px #0002", font:"14px system-ui"});
  document.body.append(tooltip);
  const stationTargets = stations.map(station => ({row:station, pixels:[...station.pixels, ...station.pixels]}));
  const hit = event => {
    const bounds = canvas.getBoundingClientRect();
    return nearestLink(links, event.clientX - bounds.left, event.clientY - bounds.top, view.plot_rect());
  };
  const move = event => {
    if (event.buttons) { hide(); return; }
    const row = hit(event);
    const bounds = canvas.getBoundingClientRect();
    const station = nearestLink(stationTargets, event.clientX - bounds.left, event.clientY - bounds.top, view.plot_rect());
    canvas.style.cursor = row ? "pointer" : "grab";
    tooltip.textContent = station ? station.label : row
      ? `${pathSourceLabel(row)} → ${row.receiver}\n${detectedBandLabel(row)}\n${row.freshness || "Freshness unverified"}\nClick to open 48-hour link plots` : "";
    tooltip.style.display = tooltip.textContent ? "block" : "none";
    tooltip.style.left = `${Math.max(8, Math.min(event.clientX + 14, window.innerWidth - tooltip.offsetWidth - 8))}px`;
    tooltip.style.top = `${Math.max(8, Math.min(event.clientY + 14, window.innerHeight - tooltip.offsetHeight - 8))}px`;
  };
  const hide = () => { tooltip.style.display = "none"; canvas.style.cursor = "grab"; };
  const click = event => { const row = hit(event); if (row) open(row); };
  canvas.addEventListener("pointermove", move);
  canvas.addEventListener("click", click);
  const hideEvents = ["pointerleave", "pointerdown", "pointercancel", "wheel"];
  hideEvents.forEach(name => canvas.addEventListener(name, hide));
  window.addEventListener("blur", hide);
  window.addEventListener("scroll", hide, true);
  return () => {
    canvas.removeEventListener("pointermove", move);
    canvas.removeEventListener("click", click);
    hideEvents.forEach(name => canvas.removeEventListener(name, hide));
    window.removeEventListener("blur", hide);
    window.removeEventListener("scroll", hide, true);
    tooltip.remove();
    canvas.style.cursor = "";
    canvas.removeAttribute("title");
  };
}
// Short great-circle arc on the sphere. Split at the date line so Mercator
// never draws or hit-tests a spurious chord across the entire world.
export function greatCircle(a, b, step = 2) {
  const rad = Math.PI / 180, vector = p => [Math.cos(p.lat*rad)*Math.cos(p.lon*rad),Math.cos(p.lat*rad)*Math.sin(p.lon*rad),Math.sin(p.lat*rad)];
  const u=vector(a), v=vector(b), dot=Math.max(-1,Math.min(1,u.reduce((s,x,i)=>s+x*v[i],0)));
  let tangent=v.map((x,i)=>x-dot*u[i]), length=Math.hypot(...tangent);
  const angle=Math.atan2(length,dot);
  if (length<1e-12) {
    if (dot>0) return new Float64Array([a.lon,a.lat,a.lon,a.lat]);
    const k=u.map(Math.abs).indexOf(Math.min(...u.map(Math.abs)));
    tangent=u.map((x,i)=>(i===k?1:0)-u[k]*x); length=Math.hypot(...tangent);
  }
  tangent=tangent.map(x=>x/length);
  const n=Math.max(1,Math.ceil(angle/rad/step)), result=[];
  let previous;
  for(let i=0;i<=n;i++) {
    const t=angle*i/n, p=u.map((x,k)=>x*Math.cos(t)+tangent[k]*Math.sin(t));
    const lon=Math.atan2(p[1],p[0])/rad, lat=Math.atan2(p[2],Math.hypot(p[0],p[1]))/rad;
    if(previous && Math.abs(lon-previous[0])>180) {
      const edge=previous[0]>0?180:-180, unwrapped=lon+(lon<previous[0]?360:-360);
      const y=previous[1]+(lat-previous[1])*(edge-previous[0])/(unwrapped-previous[0]);
      result.push(edge,y,NaN,NaN,-edge,y);
    }
    result.push(lon,lat); previous=[lon,lat];
  }
  return new Float64Array(result);
}
export function latestIonogram(products, row, at = null) {
  return plotDisplayDefaults(products.filter(p => p.quantity === "ionogram" && p.station === row.receiver && p.transmitter === row.transmitter && (at == null || p.timestamp <= at))
    .reduce((latest, p) => !latest || p.timestamp > latest.timestamp ? p : latest, null));
}
// Also apply the chosen PRN display scale to archived/capped-transfer bundles
// made before the receiver default changed. Manual colorbar edits still win.
export const plotDisplayDefaults = p => p?.kind === "prn" ? {...p,low:0,high:20} : p;
export const pathSourceLabel = row => row.source_label || (row.transmitter === "THJ76" ? "Pituffik (Thule; provisional)" : row.transmitter);
export function detectedBandLabel(row) {
  if(row?.association_ambiguous) return "ROTHR site unresolved: overlapping propagation-range gates";
  if(row?.measurement_quality === "commissioning") return "Automatic frequency limits withheld during commissioning";
  if(row?.measurement_quality === "cross-modulation-excluded") return "Frequency limits excluded: cross-modulation";
  const prefix = row?.assumed ? "Assumed detected range" : "Detected frequency range";
  return row?.lower_mhz != null && row?.upper_mhz != null
    ? `${prefix}: ${row.lower_mhz.toFixed(2)}–${row.upper_mhz.toFixed(2)} MHz`
    : "No supported frequency range detected";
}

// Keep the three supported sounder families explicit in cards and popups.
export const ionogramLabel = kind => ({digisonde:"Digisonde ionogram",lfm:"LFM ionogram",prn:"PRN ionogram"}[kind] || "Ionogram");
