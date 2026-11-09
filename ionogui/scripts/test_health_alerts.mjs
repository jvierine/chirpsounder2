import { test } from "node:test";
import assert from "node:assert/strict";
import { receiverIssues, processingIssues, publicationIssues, cpuTelemetry } from "../web/health.js";
const now = Date.parse("2026-10-22T12:00:00Z") / 1000;
test("an old CPU percentage and an unavailable measurement are never reported as live CPU", () => {
  assert.equal(cpuTelemetry({time:now-3*86400,cpu_percent:73},"KREA",now).stale,true);
  assert.equal(cpuTelemetry({time:now-30,cpu_percent:73},"KREA",now).stale,false);
  assert.equal(cpuTelemetry({time:now,cpu_percent:null},"DOB",now).used,null);
  assert.equal(cpuTelemetry({time:now,cpu_percent:0},"DOB",now).used,0);
});
test("fresh ionograms cannot conceal a stalled full-day publication", () => {
  const b={generated_unix:now-30,bulk_generated_unix:now-5*3600};
  assert.match(publicationIssues(b,"2026-10-22",now)[0],/5 h 0 min.*Latest ionograms/);
  assert.deepEqual(publicationIssues({...b,bulk_generated_unix:now-900},"2026-10-22",now),[]);
});
test("archive and unavailable timestamps never invent a current publication fault", () => {
  assert.deepEqual(publicationIssues({bulk_generated_unix:now-86400},"2026-10-21",now),[]);
  for(const t of [undefined,0,NaN,now+121])assert.deepEqual(publicationIssues({bulk_generated_unix:t},"2026-10-22",now),[]);
});
const healthy = () => ({time:now-300, generated_unix:now-30, cpu_percent:100,
  ringbuffer:{digital_rf_ok:true,newest_age_s:1}, output:{exists:true,newest_age_s:30},
  disks:[{label:"Data disk",used_percent:80,free_bytes:100*2**30}],
  memory:{MemTotal:16*2**30,MemAvailable:4*2**30}, temperatures:[{sensor:"CPU",celsius:80}]});
test("busy healthy receivers and ordinary absent echo bands do not alarm", () => {
  assert.deepEqual(receiverIssues({...healthy(),paths:[{lower_mhz:null}]},"TGO","2026-10-22",now),[]);
});
test("historical telemetry never produces live hardware alarms", () => {
  assert.deepEqual(receiverIssues({time:now-86400,ringbuffer:{digital_rf_ok:false}},"TGO","2026-10-21",now),[]);
});
test("DOB publication latency is allowed without hiding stale ordinary stations", () => {
  const h={...healthy(),time:now-3600,generated_unix:now-1200};
  assert.deepEqual(receiverIssues(h,"DOB","2026-10-22",now),[]);
  assert.match(receiverIssues(h,"KHO","2026-10-22",now)[0],/sensor reports/);
});
test("transport delay does not invent stopped acquisition or absent products", () => {
  assert.deepEqual(receiverIssues({...healthy(),generated_unix:now-300},"TGO","2026-10-22",now),[]);
  const h={...healthy(),generated_unix:now-1200,ringbuffer:{digital_rf_ok:true,newest_age_s:180}};
  assert.match(receiverIssues(h,"DOB","2026-10-22",now)[0],/Receiver reported samples 3 minutes old/);
});
test("actionable acquisition, disk, temperature and memory faults are explicit", () => {
  const h={...healthy(),ringbuffer:{digital_rf_ok:false},disks:[{label:"Data disk",used_percent:99,free_bytes:100*2**20}],
    memory:{MemTotal:1000,MemAvailable:5},temperatures:[{sensor:"CPU",celsius:98}]};
  const issues=receiverIssues(h,"TGO","2026-10-22",now);
  assert.equal(issues.length,4);
  for(const pattern of [/Acquisition/,/Data disk/,/High temperature/,/memory/]) assert.ok(issues.some(v=>pattern.test(v)));
});
test("stale status reports do not manufacture an acquisition failure", () => {
  const issues=receiverIssues({...healthy(),generated_unix:now-900,ringbuffer:{digital_rf_ok:false}},"TGO","2026-10-22",now);
  assert.equal(issues.length,1);
  assert.match(issues[0],/status is 15 minutes old/);
});

test("explicit scheduler losses warn even when current CPU is idle", () => {
  const h={...healthy(),cpu_percent:10,processing:{generated_unix:now-30,workers:[{worker:"prn-Kiruna",last_miss_unix:now-60,missed_soundings:1,missed_dwells:4,cpu_limited_losses:3}]}};
  const issues=receiverIssues(h,"DOB","2026-10-22",now);
  assert.equal(issues.length,1); assert.match(issues[0],/1 sounding.*4 frequency dwell.*CPU capacity/);
});
test("backlog without CPU evidence remains a distinct warning", () => {
  const h={processing:{generated_unix:now,workers:[{worker:"digisonde-Dourbes",last_miss_unix:now-60,missed_soundings:2,missed_dwells:0,cpu_limited_losses:0}]}};
  assert.match(processingIssues(h,"2026-10-22",now)[0],/CPU cause unconfirmed/);
  assert.deepEqual(processingIssues(h,"2026-10-21",now),[]);
  h.processing.workers[0].last_miss_unix=now-901;
  assert.deepEqual(processingIssues(h,"2026-10-22",now),[]);
});
test("stale or future telemetry cannot manufacture current CPU-loss warnings", () => {
  for (const t of [now-1201,now+121]) {
    assert.deepEqual(processingIssues({processing:{generated_unix:t,workers:[{last_miss_unix:now,missed_soundings:1}]}},"2026-10-22",now),[]);
  }
});
