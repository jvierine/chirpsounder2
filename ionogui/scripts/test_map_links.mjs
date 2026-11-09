import {test} from "node:test";
import assert from "node:assert/strict";
import {nearestLink, latestIonogram, fitMapRegion, pathSourceLabel, detectedBandLabel, greatCircle} from "../web/map-links.js";

test("a regional preset preserves aspect ratio and leaves room to pan and zoom out", () => {
  let fitted;
  const view = {plot_rect:()=>[50,30,1000,500],project_points:()=>[450,330,550,80]};
  const viewport = {set_bounds:(...bounds)=>{fitted=bounds; return true;}};
  assert.equal(fitMapRegion(view,viewport,[0,35,52,81]),true);
  const [x0,y0,x1,y1] = fitted;
  assert.ok(x0>0 && y0>0 && x1<1 && y1<1);
  assert.ok(Math.abs((x1-x0)-(y1-y0))<1e-12);
  assert.ok(x0<=0.4 && x1>=0.5 && y0<=0.4 && y1>=0.9);
  assert.ok(x1-x0<1);
});

test("map hit testing selects the nearest visible segment and excludes axes/colorbar", () => {
  const a = {transmitter:"SGO",receiver:"DOB"}, b = {transmitter:"Juliusruh",receiver:"TGO"};
  const links = [{row:a,pixels:[-100,50,200,50]}, {row:b,pixels:[0,65,200,65]}];
  assert.equal(nearestLink(links,100,53,[10,10,190,100]),a);
  assert.equal(nearestLink(links,100,62,[10,10,190,100]),b);
  assert.equal(nearestLink(links,100,90,[10,10,190,100]),null);
  assert.equal(nearestLink(links,5,50,[10,10,190,100]),null);
  assert.equal(nearestLink(links,220,50,[10,10,190,100]),null);
  assert.equal(nearestLink([{row:a,pixels:[NaN,0,0,0]}],50,50,[0,0,100,100]),null);
});
test("popup chooses the newest ionogram for the exact transmitter/receiver", () => {
  const row = {transmitter:"SGO",receiver:"DOB"};
  const p = {quantity:"ionogram",transmitter:"SGO",station:"DOB",timestamp:100};
  const latest = {...p,timestamp:200};
  assert.equal(latestIonogram([latest,p,{...p,station:"TGO",timestamp:400},{...p,transmitter:"Juliusruh",timestamp:500},{...p,quantity:"snr",timestamp:600}],row),latest);
  assert.equal(latestIonogram([p],{...row,receiver:"KHO"}),null);
  assert.equal(latestIonogram([latest,p], row, 150), p);
  assert.equal(latestIonogram([latest,p], row, 99), null);
  assert.equal(latestIonogram([latest,p], row, 200), latest);
});
test("assumed associations and detected MHz limits remain explicit", () => {
  const row={transmitter:"US",source_label:"US (assumed)",assumed:true,lower_mhz:5.1,upper_mhz:12.35};
  assert.equal(pathSourceLabel(row),"US (assumed)");
  assert.equal(detectedBandLabel(row),"Assumed detected range: 5.10–12.35 MHz");
  assert.equal(detectedBandLabel({...row,lower_mhz:null,upper_mhz:null}),"No supported frequency range detected");
  assert.equal(detectedBandLabel({lower_mhz:2,upper_mhz:6}),"Detected frequency range: 2.00–6.00 MHz");
  assert.equal(detectedBandLabel({measurement_quality:"commissioning",lower_mhz:null,upper_mhz:null}),"Automatic frequency limits withheld during commissioning");
});
test("archived PRN ionograms use the requested 0–20 dB display default",()=>{
  const p={station:"DOB",transmitter:"Kiruna",kind:"prn",quantity:"ionogram",timestamp:100,low:13,high:30,id:"unchanged-data"};
  const selected=latestIonogram([p],{receiver:"DOB",transmitter:"Kiruna"});
  assert.equal(selected.low,0);assert.equal(selected.high,20);assert.equal(selected.id,p.id);
  assert.equal(p.low,13);
});
test("great circles bow poleward, remain clickable along curves, and split the date line",()=>{
  const points=greatCircle({lon:-75,lat:40},{lon:18,lat:70});
  assert.ok(points.length>8);assert.ok(Math.max(...Array.from(points).filter((_,i)=>i%2))>70);
  const row={transmitter:"US",receiver:"TGO"};
  const i=Math.floor(points.length/4)*2;
  assert.equal(nearestLink([{row,pixels:points}],points[i],points[i+1],[-180,-90,360,180],1),row);
  const seam=greatCircle({lon:170,lat:40},{lon:-170,lat:40});
  assert.ok(Array.from(seam).some(Number.isNaN));
  for(let i=0;i+3<seam.length;i+=2)if(Number.isFinite(seam[i]+seam[i+2]))assert.ok(Math.abs(seam[i+2]-seam[i])<=180);
  for(const [a,b] of [[{lon:0,lat:0},{lon:180,lat:0}],[{lon:0,lat:90},{lon:0,lat:90}]])assert.ok(Array.from(greatCircle(a,b)).filter(Number.isFinite).length>=4);
});
