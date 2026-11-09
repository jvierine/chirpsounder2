import {test} from 'node:test';
import assert from 'node:assert/strict';
import {bindPlotNavigation} from '../web/navigation.js';
class Target {
 handlers=new Map();style={};clientWidth=400;clientHeight=600;
 addEventListener(n,f){const a=this.handlers.get(n)||[];a.push(f);this.handlers.set(n,a)}
 removeEventListener(n,f){this.handlers.set(n,(this.handlers.get(n)||[]).filter(x=>x!==f))}
 setAttribute(){} setPointerCapture(){} remove(){} getBoundingClientRect(){return {left:0,top:0}}
 fire(type,props={}){const e={type,button:0,pointerId:1,pointerType:'touch',clientX:100,clientY:200,preventDefault(){},stopImmediatePropagation(){this.stopped=true},...props};for(const f of this.handlers.get(type)||[])f(e);return e}
}
function setup(run){
 const doc=globalThis.document,win=globalThis.window;
 globalThis.document={createElement:()=>new Target(),body:{append(){}}};globalThis.window=new Target();
 const c=new Target(),calls=[],v={pan:(...a)=>calls.push(['pan',...a]),zoom:(...a)=>calls.push(['zoom',...a]),zoom_box:(...a)=>calls.push(['box',...a]),reset:()=>calls.push(['reset'])};
 const free=bindPlotNavigation(c,v,()=>calls.push(['draw']));
 try{run(c,calls)}finally{free();assert.equal(c.style.touchAction,undefined);globalThis.document=doc;globalThis.window=win}
}
test('pinch spreads to zoom in; midpoint moves pan; lifting one finger continues smoothly',()=>setup((c,a)=>{
 c.fire('pointerdown');c.fire('pointerdown',{pointerId:2,clientX:300});
 c.fire('pointermove',{pointerId:2,clientX:350});
 const z=a.find(x=>x[0]==='zoom'); assert.equal(z[1],.5); assert.ok(Math.abs(z[2]-2/3)<1e-12); assert.equal(z[3],.8);
 c.fire('pointerup',{pointerId:2,clientX:350});a.length=0;
 c.fire('pointermove',{clientX:120});
 assert.ok(Math.abs(a.find(x=>x[0]==='pan')[1]+.05)<1e-12);
 c.fire('pointerup',{clientX:120});assert.equal(c.fire('click').stopped,true);
}));
test('a touch tap remains clickable; cancellation clears the gesture',()=>setup((c,a)=>{
 c.fire('pointerdown');c.fire('pointerup');assert.equal(c.fire('click').stopped,undefined);
 c.fire('pointerdown');c.fire('pointerdown',{pointerId:2,clientX:300});
 c.fire('pointercancel',{pointerId:2});c.fire('pointercancel');a.length=0;
 c.fire('pointermove',{clientX:150});assert.equal(a.length,0);
}));
test('desktop wheel still requires a modifier, and box zoom is preserved',()=>setup((c,a)=>{
 c.fire('wheel',{pointerType:'mouse',deltaY:100});assert.equal(a.length,0);
 c.fire('wheel',{pointerType:'mouse',ctrlKey:true,deltaY:100});assert.ok(a.some(x=>x[0]==='zoom'));
 a.length=0;c.fire('pointerdown',{pointerType:'mouse',ctrlKey:true});c.fire('pointermove',{pointerType:'mouse',clientX:200,clientY:300});c.fire('pointerup',{pointerType:'mouse',clientX:200,clientY:300});assert.ok(a.some(x=>x[0]==='box'));
}));
