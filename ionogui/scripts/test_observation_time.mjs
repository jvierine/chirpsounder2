import {test} from "node:test";
import assert from "node:assert/strict";
import {utcDay, utcMinute, selectionUnix, calendarDays} from "../web/observation-time.js";

test("UTC calendar and cutoff stay independent of browser timezone", () => {
  const ms = Date.parse("2026-11-03T00:05:00Z");
  assert.equal(utcDay(ms), "2026-11-03");
  assert.equal(utcMinute(ms), "00:05");
  assert.equal(selectionUnix("2026-11-03", "00:05"), ms / 1000);
  assert.equal(selectionUnix("2026-02-30", "12:00"), null);
  assert.equal(selectionUnix("2026-11-03", "24:00"), null);
  assert.equal(selectionUnix("2026-11-03", ""), null);
});
test("calendar aligns Monday-first including leap days and year boundary", () => {
  assert.deepEqual(calendarDays("2026-11").slice(0, 7), [null,null,null,null,null,null,"2026-11-01"]);
  assert.equal(calendarDays("2024-02").at(-1), "2024-02-29");
  assert.equal(calendarDays("2027-01").filter(Boolean).length, 31);
});
