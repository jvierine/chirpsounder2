// Bounds are independent of how many cards exist on the page.
export const MAX_CELLS = 1440 * 512;
export class ArrayCache {
  constructor(limit = 12 * 1024 * 1024) {
    this.limit = limit;
    this.bytes = 0;
    this.entries = new Map();
  }
  get(id) {
    const v = this.entries.get(id);
    if (v) {
      this.entries.delete(id);
      this.entries.set(id, v);
    }
    return v;
  }
  set(id, values) {
    const old = this.entries.get(id);
    if (old) this.bytes -= old.byteLength;
    this.entries.delete(id);
    if (values.byteLength > this.limit) return;
    this.entries.set(id, values);
    this.bytes += values.byteLength;
    while (this.bytes > this.limit) {
      const key = this.entries.keys().next().value;
      this.bytes -= this.entries.get(key).byteLength;
      this.entries.delete(key);
    }
  }
  clear() {
    this.entries.clear();
    this.bytes = 0;
  }
}
export async function readArray(
  response,
  nx,
  ny,
  { signal, onRows = () => {}, onProgress = () => {} } = {},
) {
  if (!response.ok) throw Error(`Array HTTP ${response.status}`);
  if (
    !Number.isInteger(nx) ||
    !Number.isInteger(ny) ||
    nx < 2 ||
    ny < 2 ||
    nx * ny > MAX_CELLS
  )
    throw Error("Unsupported display array");
  const values = new Float32Array(nx * ny).fill(NaN),
    target = new Uint8Array(values.buffer),
    reader = response.body.getReader();
  let received = 0,
    uploaded = 0,
    lastDraw = 0;
  try {
    while (true) {
      if (signal?.aborted) throw new DOMException("Canceled", "AbortError");
      const { done, value } = await reader.read();
      if (done) break;
      if (signal?.aborted) throw new DOMException("Canceled", "AbortError");
      if (received + value.length > target.length)
        throw Error("Array exceeds declared dimensions");
      target.set(value, received);
      received += value.length;
      onProgress(received / target.length);
      const rows = Math.floor(received / (nx * 4)),
        now = performance.now();
      if (rows > uploaded && (uploaded === 0 || now - lastDraw > 100)) {
        onRows(uploaded, values.subarray(uploaded * nx, rows * nx));
        uploaded = rows;
        lastDraw = now;
      }
    }
    if (received !== target.length) throw Error("Incomplete array transfer");
    if (uploaded < ny) onRows(uploaded, values.subarray(uploaded * nx));
    return values;
  } finally {
    reader.cancel().catch(() => {});
    reader.releaseLock();
  }
}
