// A line view of the same receiver-averaged power already used by the raw
// interference heatmap. Reuse its bounded numeric cache and real missing bins.
export function latestSpectrumLine(p, values, cutoff = null) {
  for (let x = p.nx - 1; x >= 0; x--) {
    if (cutoff != null && p.x[x] + 30 > cutoff) continue;
    let low = Infinity, high = -Infinity;
    const xy = new Float64Array(p.ny * 2);
    for (let y = 0; y < p.ny; y++) {
      const value = values[y * p.nx + x];
      xy[2*y] = p.y[y]; xy[2*y+1] = value;
      if (Number.isFinite(value)) { low = Math.min(low,value); high = Math.max(high,value); }
    }
    if (Number.isFinite(low)) return {xy, start:p.x[x]-30, end:p.x[x]+30,
      low:Math.floor((low-3)/5)*5, high:Math.ceil((high+3)/5)*5};
  }
  return null;
}
