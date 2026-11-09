import {test} from "node:test";
import assert from "node:assert/strict";
import {groupProducts,soundingColumns} from "../web/groups.js";
const p=(kind,transmitter,quantity,station="TGO")=>({kind,transmitter,quantity,station});
test("each path keeps all RTI views in one ordered panel stack",()=>{
  const groups=groupProducts([p("rti","SGO","range-frequency"),p("rti","Ramfjordmoen","snr"),p("rti","SGO","snr"),p("rti","SGO","peak-frequency")]);
  assert.equal(groups.length,2);
  assert.deepEqual(groups.find(g=>g.transmitter==="SGO").panels.map(v=>v.quantity),["snr","peak-frequency","range-frequency"]);
});
test("sweep rates and receiver identities stay separate",()=>{
  const groups=groupProducts([p("detections","100 kHz/s","peak-frequency"),p("detections","100 kHz/s","range-frequency"),p("detections","125 kHz/s","peak-frequency"),p("rti","SGO","snr"),p("rti","SGO","snr","KHO")]);
  assert.equal(groups.length,4);
  assert.equal(groups.find(g=>g.transmitter==="100 kHz/s").panels.length,2);
});
test("explicit non-detections retain their sounding time; other paths and days do not leak",()=>{
  const start=Date.parse("2026-10-22T00:00:00Z")/1000;
  const r=(minute,receiver="TGO",transmitter="SGO")=>({time:start+minute*60,receiver,transmitter,lower_mhz:null});
  assert.deepEqual([...soundingColumns([r(2),r(7),r(7),r(-1),r(1440),r(12,"KHO"),r(15,"TGO","other")],"TGO","SGO","2026-10-22")],[2,7]);
});
