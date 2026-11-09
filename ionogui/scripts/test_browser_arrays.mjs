import { test } from "node:test";
import assert from "node:assert/strict";
import { ArrayCache, readArray } from "../web/arrays.js";
function response(bytes, sizes) {
  let offset = 0,
    i = 0;
  return new Response(
    new ReadableStream({
      pull(c) {
        if (offset === bytes.length) {
          c.close();
          return;
        }
        const n = sizes[i++ % sizes.length],
          end = Math.min(bytes.length, offset + n);
        c.enqueue(bytes.slice(offset, end));
        offset = end;
      },
    }),
  );
}
test("misaligned stream chunks preserve Float32 values and gaps", async () => {
  const input = new Float32Array([1, NaN, 3, 4, 5, 6, 7, 8]);
  const blocks = [];
  const values = await readArray(
    response(new Uint8Array(input.buffer), [1, 7, 3, 2]),
    4,
    2,
    { onRows: (row, v) => blocks.push([row, [...v]]) },
  );
  assert.deepEqual(values, input);
  assert.equal(blocks[0][0], 0);
  assert.equal(blocks.at(-1)[0] + blocks.at(-1)[1].length / 4, 2);
});
test("truncated and oversized bodies cannot become cached observations", async () => {
  await assert.rejects(
    readArray(response(new Uint8Array(15), [8]), 2, 2),
    /Incomplete/,
  );
  await assert.rejects(
    readArray(response(new Uint8Array(17), [17]), 2, 2),
    /exceeds/,
  );
});
test("abort stops a progressive transfer", async () => {
  const controller = new AbortController();
  controller.abort();
  await assert.rejects(
    readArray(response(new Uint8Array(16), [4]), 2, 2, {
      signal: controller.signal,
    }),
    (e) => e.name === "AbortError",
  );
});
test("cache evicts least recently used arrays at its strict byte ceiling", () => {
  const c = new ArrayCache(32);
  c.set("a", new Float32Array(4));
  c.set("b", new Float32Array(4));
  c.get("a");
  c.set("c", new Float32Array(4));
  assert.equal(c.get("b"), undefined);
  assert.equal(c.bytes, 32);
  for (let i = 0; i < 100; i++) c.set(String(i), new Float32Array(4));
  assert.equal(c.entries.size, 2);
  assert.equal(c.bytes, 32);
});
