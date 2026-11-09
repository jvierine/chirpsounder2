import {test} from "node:test";
import assert from "node:assert/strict";
import {linkQuality,remapRows} from "../web/link-history-popup.js";
test("W2NAF cross-modulation is excluded by default while US sources retain a warning",()=>{
  for(const transmitter of ["Australia","Cyprus","JORN","NIC"])assert.equal(linkQuality({receiver:"W2NAF",transmitter}).excluded,true);
  for(const transmitter of ["US","ROTHR1","ROTHR2","ROTHR3"]) {const q=linkQuality({receiver:"W2NAF",transmitter});assert.equal(q.excluded,false);assert.equal(q.limited,true);}
  assert.equal(linkQuality({receiver:"TGO",transmitter:"Australia"}).excluded,false);
});
test("commissioning withholds quantitative estimates without hiding diagnostic plots or inventing cross-modulation",()=>{
  const q=linkQuality({receiver:"W2NAF",transmitter:"Millstone",measurement_quality:"commissioning",quantitative_eligible:false});
  assert.equal(q.excluded,false);assert.equal(q.commissioning,true);assert.ok(!q.label.includes("cross-modulation"));
  assert.equal(linkQuality({receiver:"W2NAF",transmitter:"Cyprus",measurement_quality:"commissioning",quantitative_eligible:false}).excluded,true);
});
test("48-hour remapping preserves real gaps and excludes future samples",()=>{
  const end=1794000000,p={nx:4,x:[end-172900,end-172740,end-60,end+60]};
  const v=remapRows(p,0,new Float32Array([9,5,8,12]),end);
  assert.equal(v.length,1440);assert.equal(v[0],5);assert.equal(v[1439],8);assert.ok(Number.isNaN(v[1]));
  assert.ok(!v.includes(9)&&!v.includes(12));
});

test("overlapping ROTHR site gates remain diagnostic candidates",()=>{
  const q=linkQuality({receiver:"W2NAF",transmitter:"ROTHR2",association_ambiguous:true,measurement_quality:"commissioning",quantitative_eligible:false});
  assert.equal(q.excluded,false);assert.equal(q.limited,true);assert.match(q.label,/overlaps another transmitter/);
  assert.ok(!q.label.includes("cross-modulation"));
});
