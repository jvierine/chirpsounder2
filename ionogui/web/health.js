// Scheduler losses are explicit receiver counters, not inferred from CPU usage.
export function cpuTelemetry(h, station, now = Date.now() / 1000) {
  const time = h?.time ?? h?.generated_unix;
  const age = Number.isFinite(time) ? Math.max(0, now - time) : null;
  const stale = age != null && (age > (station === "DOB" ? 5400 : 900) || time > now + 120);
  const used = typeof h?.cpu_percent === "number" && Number.isFinite(h.cpu_percent) ? h.cpu_percent : null;
  return {used, free:used == null ? null : Math.max(0, 100-used), stale, age};
}
export function publicationIssues(bundle, day, now = Date.now() / 1000) {
  if (day !== new Date(now * 1000).toISOString().slice(0, 10)) return [];
  const t = bundle?.bulk_generated_unix;
  if (!Number.isFinite(t) || t <= 0 || t > now + 120 || now - t <= 1800) return [];
  const minutes = Math.floor((now - t) / 60);
  const age = minutes >= 60 ? `${Math.floor(minutes / 60)} h ${minutes % 60} min` : `${minutes} minutes`;
  return [`Full-day RTI/spectrum publication is ${age} old. Latest ionograms may be newer.`];
}
export function processingIssues(h, day, now = Date.now() / 1000) {
  if (day !== new Date(now * 1000).toISOString().slice(0, 10)) return [];
  const p = h?.processing;
  if (!p || !Number.isFinite(p.generated_unix) || p.generated_unix > now + 120 || now - p.generated_unix > 1200) return [];
  return (p.workers || []).filter(w => Number.isFinite(w.last_miss_unix) && w.last_miss_unix <= now + 120 && now - w.last_miss_unix <= 900 && (w.missed_soundings > 0 || w.missed_dwells > 0)).map(w => {
    const count = [w.missed_soundings > 0 ? `${w.missed_soundings} sounding${w.missed_soundings === 1 ? "" : "s"}` : null, w.missed_dwells > 0 ? `${w.missed_dwells} frequency dwell${w.missed_dwells === 1 ? "" : "s"}` : null].filter(Boolean).join(" and ");
    const cause = w.cpu_limited_losses > 0 ? "CPU capacity or scheduling limits detected" : "processing fell behind; CPU cause unconfirmed";
    return `${w.worker}: missed ${count} in the last 15 minutes — ${cause}.`;
  });
}
// Operational alerts apply to current observations, never to archived health.
export function receiverIssues(h, station, day, now = Date.now() / 1000) {
  if (!h || day !== new Date(now * 1000).toISOString().slice(0, 10)) return [];
  const issues = [], age = t => Math.max(0, now - t);
  if (h.time > now + 120 || h.generated_unix > now + 120)
    issues.push("Receiver clock is more than two minutes ahead.");
  const sensorAge = h.time ? age(h.time) : 0;
  if (sensorAge > (station === "DOB" ? 5400 : 900))
    issues.push(`CPU and sensor reports are ${Math.round(sensorAge / 60)} minutes old.`);
  const statusAge = h.generated_unix ? age(h.generated_unix) : 0;
  if (h.generated_unix && statusAge > (station === "DOB" ? 5400 : 600)) {
    issues.push(`Acquisition and disk status is ${Math.round(statusAge / 60)} minutes old.`);
    return [...issues, ...processingIssues(h, day, now)];
  }
  if (h.ringbuffer?.digital_rf_ok === false)
    issues.push("Acquisition is not reporting valid Digital RF data.");
  // Transit delay cannot establish that a receiver stopped after reporting.
  // Evaluate its measured sample age; report stale telemetry separately above.
  else if (h.ringbuffer?.digital_rf_ok && h.ringbuffer.newest_age_s > 120)
    issues.push(`Receiver reported samples ${Math.round(h.ringbuffer.newest_age_s / 60)} minutes old.`);
  if (h.output?.exists === false)
    issues.push("Science output directory is unavailable.");
  else if (h.output?.exists && h.output.newest_age_s > 1200)
    issues.push("Receiver reported no new science products for more than 20 minutes.");
  for (const d of h.disks || [])
    if (d.used_percent >= 95 || d.free_bytes < 512 * 2 ** 20)
      issues.push(`${d.label}: ${d.used_percent.toFixed(1)}% used, ${(d.free_bytes / 2 ** 30).toFixed(1)} GiB free.`);
  const hot = (h.temperatures || []).filter(t => t.celsius >= 95);
  if (hot.length) {
    const peak = hot.reduce((a,b) => a.celsius > b.celsius ? a : b);
    issues.push(`High temperature: ${peak.sensor}, ${peak.celsius.toFixed(1)} °C.`);
  }
  const m = h.memory;
  if (m?.MemTotal > 0 && m.MemAvailable / m.MemTotal < 0.01)
    issues.push("Less than 1% of receiver memory is available.");
  return [...issues, ...processingIssues(h, day, now)];
}
