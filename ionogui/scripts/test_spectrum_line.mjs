import {test} from 'node:test';
import assert from 'node:assert/strict';
import {latestSpectrumLine} from '../web/spectrum-line.js';
test('latest RF line preserves persistent carriers, missing bins and UTC cutoff',()=>{
  const p={nx:3,ny:3,x:[30,90,150],y:[1,2,3]};
  const values=new Float32Array([20,23,NaN,40,43,NaN,NaN,NaN,NaN]);
  const latest=latestSpectrumLine(p,values);
  assert.equal(latest.start,60); assert.equal(latest.end,120);
  assert.deepEqual(Array.from(latest.xy),[1,23,2,43,3,NaN]);
  assert.equal(latestSpectrumLine(p,values,119).start,0);
  assert.equal(latestSpectrumLine(p,values,59),null);
  assert.equal(latestSpectrumLine(p,new Float32Array(9).fill(NaN)),null);
});
