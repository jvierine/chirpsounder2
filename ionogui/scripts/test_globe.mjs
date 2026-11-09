import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import {GlobeCamera,globeProjection} from '../web/globe-camera.js';
import {greatCircle,nearestLink,pathSourceLabel} from '../web/map-links.js';
import {bindGlobeNavigation} from '../web/propagation-globe.js';

test('the globe is default, with an explicit bookmarkable 2D fallback',()=>{
  assert.equal(globeProjection('https://juha.no/iono/?view=propagation'),'3d');
  assert.equal(globeProjection('https://juha.no/iono/?projection=2d'),'2d');
});
test('spherical hit targets hide the back hemisphere and track rotation/zoom',()=>{
  const c=new GlobeCamera();c.yaw=0;c.pitch=0;
  assert.deepEqual(Array.from(c.project([0,0],800,600)),[400,300]);
  assert.ok(Array.from(c.project([180,0],800,600)).every(Number.isNaN));
  const path=greatCircle({lon:-30,lat:10},{lon:30,lat:10}),row={transmitter:'A',receiver:'B'};
  let pixels=c.project(path,800,600);assert.equal(nearestLink([{row,pixels}],pixels[0],pixels[1],[0,0,800,600]),row);
  c.yaw=Math.PI;pixels=c.project(path,800,600);assert.equal(nearestLink([{row,pixels}],400,300,[0,0,800,600]),null);
  c.reset('norway');const n=c.project([17,67],800,600);assert.ok(Math.hypot(n[0]-400,n[1]-300)<1e-8);
  for(const p of [[15,78],[13.4,54.6]])assert.ok(Array.from(c.project(p,800,600)).every(Number.isFinite));
  c.zoomBy(1e9);assert.equal(c.zoom,15);c.zoomBy(0);assert.equal(c.zoom,.5);
});
test('HEIMDALL spherical shader and white background are actually reused',()=>{
  const s=readFileSync(new URL('../web/vendor/heimdall-globe.js',import.meta.url),'utf8');
  assert.match(s,/HEIMDALL GIM WebglVtecMap/);assert.match(s,/v_visible=p.z/);assert.match(s,/gl.clearColor\(1,1,1,1\)/);
  assert.doesNotMatch(s,/new ResizeObserver/);assert.doesNotMatch(s,/void this.loadCoastlines\(\)/);
});
test('3D renderer preserves the shared palette, station targets and path callback',async()=>{
  const app=readFileSync(new URL('../web/app.js',import.meta.url),'utf8'),renderer=app.slice(app.indexOf('async function drawNetworkMap('),app.indexOf('async function loadNetworkPropagation('));
  const elements=new Map();const el=s=>{if(!elements.has(s))elements.set(s,{hidden:false,style:{},classList:{contains:()=>false},getBoundingClientRect:()=>({left:0,top:40,width:1000,height:650}),setAttribute(){}});return elements.get(s);};
  const view={set_font_size(){},set_color_key(){},set_color_scale(){},begin_map(){},set_view(){},plot_rect:()=>[50,90,900,500]};
  let bindings,entries;const globe={camera:{reset(){}},setCoastline(){},setEntries:v=>entries=v,project_points:p=>p};
  const c=vm.createContext({networkData:null,networkGeneration:1,activeView:'propagation',networkProjection:'3d',networkRegion:'world',networkPresetPending:true,networkMapView:view,networkGlobe:globe,networkColorLimits:null,networkViewport:{reset(){},bounds:()=>[0,0,1,1]},innerWidth:1000,innerHeight:700,devicePixelRatio:1,coastlines:[0,0,1,1],$:el,plotFont:()=>18,mapStatusKey:[],mapPathStyle:()=>({color:'#123456'}),greatCircle,pathSourceLabel,redrawNetworkMap(){},openIonogram(){},unbindNetworkLinks:null,unbindNetworkColor:null,bindMapLinks:(...v)=>{bindings=v;return()=>{};},bindColorbarEditor:()=>()=>{}});
  vm.runInContext(renderer,c);await c.drawNetworkMap([{transmitter:'SGO',receiver:'DOB',freshness:'current',upper_mhz:12}],{SGO:{lat:67,lon:26},DOB:{lat:62,lon:9}},1);
  assert.equal(entries[0].color,'#123456');assert.equal(bindings[2].length,1);assert.equal(bindings[4].length,2);assert.equal(el('#network-globe').hidden,false);
});
test('drag/pinch zoom suppresses accidental path clicks, simple clicks remain usable',()=>{
  class Target{handlers=new Map();setPointerCapture(){}addEventListener(n,f){if(!this.handlers.has(n))this.handlers.set(n,[]);this.handlers.get(n).push(f);}removeEventListener(n,f){this.handlers.set(n,this.handlers.get(n).filter(v=>v!==f));}fire(n,e={}){let stopped=false;for(const f of this.handlers.get(n)||[])f({button:0,pointerId:1,clientX:10,clientY:10,preventDefault(){},stopImmediatePropagation(){stopped=true;},...e});return stopped;}}
  const canvas=new Target(),camera=new GlobeCamera();let changed=0;const unbind=bindGlobeNavigation(canvas,camera,()=>changed++,()=>{});
  canvas.fire('pointerdown');canvas.fire('pointermove',{clientX:80});canvas.fire('pointerup',{clientX:80});assert.equal(canvas.fire('click'),true);assert.ok(changed);
  canvas.fire('pointerdown');canvas.fire('pointerup');assert.equal(canvas.fire('click'),false);
  const z=camera.zoom;canvas.fire('wheel',{deltaY:-100});assert.equal(camera.zoom,z);canvas.fire('wheel',{deltaY:-100,ctrlKey:true});assert.ok(camera.zoom>z);
  unbind();assert.ok([...canvas.handlers.values()].every(v=>v.length===0));
});
