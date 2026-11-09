export const panelKey = p => `${p.station}:${p.kind}:${p.transmitter}:${p.quantity}`;
export const groupKey = p => ["rti", "detections"].includes(p.kind)
  ? `${p.station}:${p.kind}:${p.transmitter}` : panelKey(p);
export function groupProducts(products, order = []) {
  const byKey = new Map(), rank = {spectrum:0,rti:1,detections:2,digisonde:3,lfm:4};
  for (const p of products) {
    const key=groupKey(p);
    if (!byKey.has(key)) byKey.set(key,{key,kind:p.kind,station:p.station,transmitter:p.transmitter,panels:[]});
    byKey.get(key).panels.push(p);
  }
  const position=g => {
    const groupIndex = order.indexOf(g.key);
    if (groupIndex >= 0) return groupIndex;
    const indices=g.panels.map(panelKey).map(k=>order.indexOf(k)).filter(i=>i>=0);
    return indices.length ? Math.min(...indices) : Infinity;
  };
  for(const g of byKey.values()) g.panels.sort((a,b)=>
    ["snr","peak-frequency","range-frequency"].indexOf(a.quantity)-["snr","peak-frequency","range-frequency"].indexOf(b.quantity));
  return [...byKey.values()].sort((a,b)=>(position(a)-position(b)) || (rank[a.kind]??9)-(rank[b.kind]??9) || a.transmitter.localeCompare(b.transmitter));
}
// The timing evidence includes non-detections, so display support never carries
// a neighboring echo across an explicitly recorded unsuccessful sounding.
export function soundingColumns(records, station, transmitter, day) {
  const start=Date.parse(day+"T00:00:00Z")/1000;
  return new Uint32Array([...new Set(records.filter(r=>r.receiver===station && r.transmitter===transmitter)
    .map(r=>Math.floor((r.time-start)/60)).filter(x=>x>=0 && x<1440))].sort((a,b)=>a-b));
}
