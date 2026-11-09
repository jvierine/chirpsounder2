import {test} from "node:test";
import assert from "node:assert/strict";
import {readFileSync} from "node:fs";
import vm from "node:vm";
import {bindMapLinks, greatCircle, pathSourceLabel} from "../web/map-links.js";

// Exercise the actual map renderer with a recording plotting backend. A
// permanent canvas label is a regression even if tooltip hit-testing works.
const app = readFileSync(new URL("../web/app.js", import.meta.url), "utf8");
const renderer = app.slice(app.indexOf("async function drawNetworkMap("), app.indexOf("async function loadNetworkPropagation("));
for (const [region, width, height] of [["norway",1200,650],["world",1200,650],["norway",390,844]]) test(`${region} map at ${width}px draws links and stations without permanent annotations`, async () => {
  const series = [], bindings = [], keys = [];
  const view = {
    set_font_size(){}, set_color_key(...args){keys.push(args)}, set_color_scale(){}, begin_map(){}, set_view(){},
    project_points: points => points,
    series: (...args) => series.push(args),
    label: () => assert.fail("Station and frequency annotations must only appear on hover"),
  };
  const element = {classList:{contains:()=>false}, getBoundingClientRect:()=>({width:1200,height:650}), setAttribute(){}};
  const context = vm.createContext({
    innerWidth:width, innerHeight:height, networkData:null, networkMapView:view, networkGeneration:1, activeView:"propagation", networkRegion:region,
    networkPresetPending:true, networkProjection:'2d', networkColorLimits:null, networkNavigation:null, unbindNetworkLinks:null, unbindNetworkColor:null,
    networkViewport:{reset(){}, bounds:()=>[0,0,1,1]}, devicePixelRatio:1, coastlines:[0,0,1,1],
    $:()=>element, plotFont:()=>16, mapStatusKey:[["#74828a","No band"],["#b6bec3","Stale / unverified"],["#8c8899","Excluded: cross-modulation"]], fitMapRegion(){}, greatCircle, pathSourceLabel,
    mapPathStyle:()=>({valid:true,color:"#123456"}), bindPlotNavigation:()=>()=>{}, redrawNetworkMap(){},
    bindMapLinks:(...args)=>{bindings.push(args); return ()=>{};}, openIonogram(){}, bindColorbarEditor:()=>()=>{},
  });
  vm.runInContext(renderer, context);
  await context.drawNetworkMap([{transmitter:"SGO",receiver:"DOB",freshness:"current",lower_mhz:3,upper_mhz:9}],
    {SGO:{lon:26.6,lat:67.4},DOB:{lon:9.1,lat:62.1}},1);
  assert.deepEqual(Array.from(keys[0][0]),["#74828a","#b6bec3","#8c8899"]);
  if(width<650) assert.deepEqual(Array.from(keys[0][1]),["No band","Stale","Excluded"]);
  assert.equal(series.filter(([, , connected])=>!connected).length,2,"station dots remain visible");
  assert.equal(bindings[0][2].length,1,"line remains interactive");
  assert.equal(bindings[0][4].length,2,"both stations have hover targets");
});

test("hover reveals only the target and clears on leave, drag, or redraw", () => {
  class Target {
    handlers = new Map(); style = {}; offsetWidth=260; offsetHeight=80;
    addEventListener(name, fn){this.handlers.set(name,fn);}
    removeEventListener(name){this.handlers.delete(name);}
    setAttribute(){} removeAttribute(){} remove(){this.removed=true;}
    getBoundingClientRect(){return {left:0,top:0};}
    fire(name,event={}){this.handlers.get(name)?.({clientX:100,clientY:50,buttons:0,...event});}
  }
  const previousDocument=globalThis.document, previousWindow=globalThis.window;
  const tooltip=new Target(), canvas=new Target();
  globalThis.document={createElement:()=>tooltip,body:{append(){}}};
  globalThis.window=Object.assign(new Target(),{innerWidth:800,innerHeight:600});
  let unbind;
  try {
    const row={transmitter:"SGO",receiver:"DOB",lower_mhz:3,upper_mhz:9,freshness:"current"};
    let opened;
    unbind=bindMapLinks(canvas,{plot_rect:()=>[0,0,300,200]},[{row,pixels:[20,50,200,50]}],r=>opened=r,[{label:"SGO",pixels:[20,50]}]);
    assert.equal(tooltip.style.display,"none");
    canvas.fire("pointermove");
    assert.match(tooltip.textContent,/SGO → DOB/);
    assert.match(tooltip.textContent,/3.00–9.00 MHz/);
    assert.equal(tooltip.style.display,"block");
    canvas.fire("pointermove",{clientX:20}); assert.equal(tooltip.textContent,"SGO");
    for (const event of ["pointerleave","pointerdown","pointercancel","wheel"]) {
      canvas.fire("pointermove"); canvas.fire(event); assert.equal(tooltip.style.display,"none");
    }
    canvas.fire("pointermove",{clientY:150}); assert.equal(tooltip.style.display,"none");
    canvas.fire("pointermove",{buttons:1}); assert.equal(tooltip.style.display,"none");
    canvas.fire("click"); assert.equal(opened,row);
    unbind(); unbind=null;
    assert.ok(tooltip.removed); assert.equal(canvas.handlers.size,0); assert.equal(window.handlers.size,0);
  } finally {
    unbind?.(); globalThis.document=previousDocument; globalThis.window=previousWindow;
  }
});

test("provisional sounders use unverified gray rather than the cross-modulation key", () => {
  const style = app.slice(app.indexOf("const mapStatusKey ="), app.indexOf("let mapColorLimits"));
  const c = vm.createContext({});
  vm.runInContext(style + ';this.style=mapPathStyle', c);
  const view = {scale_color:()=>"turbo"};
  assert.equal(c.style({measurement_quality:"commissioning",quantitative_eligible:false},view).color,"#b6bec4");
  assert.equal(c.style({measurement_quality:"cross-modulation-excluded",quantitative_eligible:false},view).color,"#8d8a9b");
  assert.equal(pathSourceLabel({transmitter:"THJ76"}),"Pituffik (Thule; provisional)");
});
